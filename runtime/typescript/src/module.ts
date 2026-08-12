import { WireReader, WireWriter } from "./wire.js";
import type { WasmWireWriterAllocator } from "./wire.js";
import { StreamPollManager } from "./stream.js";

const EMPTY_BUFFER = new ArrayBuffer(0);

/**
 * Widens bytes to booleans.
 *
 * `Array.from` with a mapper walks the TypedArray through the iterator
 * protocol, which costs an order of magnitude more than indexing it.
 */
function toBoolArray(bytes: Uint8Array): boolean[] {
  const result = new Array<boolean>(bytes.length);
  for (let index = 0; index < bytes.length; index++) {
    result[index] = bytes[index] !== 0;
  }
  return result;
}

const LITTLE_ENDIAN = new Uint8Array(new Uint32Array([1]).buffer)[0] === 1;
const PACKED_LOW = LITTLE_ENDIAN ? 0 : 1;
const PACKED_HIGH = LITTLE_ENDIAN ? 1 : 0;

const FFI_BUF_DESCRIPTOR_SIZE = 16;
const FFI_STATUS_SIZE = 4;
const OPTION_F64_NONE = 0xffff_ffff_ffff_ffffn;
const OPTION_F64_NAN = 0x7ff8_0000_0000_0000n;
const MIN_WRITER_CAPACITY = 64;
const MAX_WRITERS_PER_CAPACITY = 32;

const enum PackedPrimitive {
  Bool,
  I8,
  U8,
  I16,
  U16,
  I32,
  U32,
  I64,
  U64,
  F32,
  F64,
}

export const enum WasmPollStatus {
  Pending = 0,
  Ready = 1,
  Cancelled = -1,
  Panicked = -2,
}

export class BoltFFIPanicError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BoltFFIPanicError";
  }
}

export class BoltFFICancelledError extends Error {
  constructor() {
    super("Future was cancelled");
    this.name = "BoltFFICancelledError";
  }
}

interface PendingFuture {
  resolve: (handle: number) => void;
  reject: (error: Error) => void;
  pollSync: (handle: number) => number;
  panicMessage: (handle: number) => number;
  free: (handle: number) => void;
  cancel: (handle: number) => void;
}

export class AsyncFutureManager {
  private pendingFutures = new Map<number, PendingFuture>();
  private cancelIds = new Map<number, number>();
  private wokenHandles = new Set<number>();
  private drainScheduled = false;
  private _module: BoltFFIModule | null = null;

  setModule(module: BoltFFIModule): void {
    this._module = module;
  }

  wake(handle: number): void {
    this.wokenHandles.add(handle);
    if (!this.drainScheduled) {
      this.drainScheduled = true;
      queueMicrotask(() => this.drainWakes());
    }
  }

  private drainWakes(): void {
    this.drainScheduled = false;
    const batch = [...this.wokenHandles];
    this.wokenHandles.clear();
    for (const handle of batch) {
      this.repollHandle(handle);
    }
  }

  private repollHandle(handle: number): void {
    const entry = this.pendingFutures.get(handle);
    if (!entry) return;

    const status = entry.pollSync(handle);
    if (status === WasmPollStatus.Ready) {
      this.pendingFutures.delete(handle);
      entry.resolve(handle);
    } else if (status < 0) {
      this.pendingFutures.delete(handle);
      entry.reject(
        this.extractAsyncError(handle, status, entry.panicMessage, entry.free)
      );
    }
  }

  // Cancelling only marks the future's state, so this forces a repoll --
  // deferred rather than immediate, since `cancel` can fire synchronously
  // from within a Rust future's own poll (e.g. a callback that aborts its
  // own signal), and repolling right here would reenter it mid-poll.
  private cancel(handle: number): void {
    const entry = this.pendingFutures.get(handle);
    if (!entry) return;
    entry.cancel(handle);
    queueMicrotask(() => this.repollHandle(handle));
  }

  // A lower-level counterpart to `signal` for callers that can pass a
  // plain int but not cheaply build a real AbortController.
  cancelById(callId: number): void {
    const handle = this.cancelIds.get(callId);
    if (handle === undefined) return;
    this.cancel(handle);
  }

  private extractAsyncError(
    handle: number,
    status: number,
    panicMessage: (handle: number) => number,
    free: (handle: number) => void
  ): Error {
    if (status === WasmPollStatus.Panicked && this._module) {
      const bufPtr = panicMessage(handle);
      const reader = this._module.readerFromBuf(bufPtr);
      const message = reader.readString();
      this._module.freeBuf(bufPtr);
      free(handle);
      return new BoltFFIPanicError(message);
    }
    free(handle);
    if (status === WasmPollStatus.Cancelled) {
      return new BoltFFICancelledError();
    }
    return new Error(`Unknown poll status: ${status}`);
  }

  pollAsync(
    handle: number,
    pollSync: (handle: number) => number,
    panicMessage: (handle: number) => number,
    free: (handle: number) => void,
    cancel: (handle: number) => void,
    signal?: AbortSignal,
    cancelId?: number
  ): Promise<number> {
    // Poll before registering. An async fn that never yields — the common
    // case, and the whole of `async_add` — was paying a Map insert, a Map
    // delete, a five-field entry object and a `new Promise` executor to
    // discover on the very next line that it was already done. `wake()` only
    // adds to a set and queues a microtask, so a wake raised from inside
    // `pollSync` cannot observe the window where the entry is absent.
    const status = pollSync(handle);
    if (status === WasmPollStatus.Ready) {
      return Promise.resolve(handle);
    }
    if (status < 0) {
      return Promise.reject(
        this.extractAsyncError(handle, status, panicMessage, free)
      );
    }
    // The signal may have aborted before this poll returned, or reentrantly
    // during it -- AbortSignal never replays a past event, so a listener
    // registered only below would miss it and leave the future running.
    if (signal?.aborted) {
      cancel(handle);
      const cancelledStatus = pollSync(handle);
      return Promise.reject(
        this.extractAsyncError(handle, cancelledStatus, panicMessage, free)
      );
    }
    return new Promise((resolve, reject) => {
      let onAbort: (() => void) | undefined;
      if (signal) {
        onAbort = () => this.cancel(handle);
        signal.addEventListener("abort", onAbort, { once: true });
      }
      if (cancelId !== undefined) {
        this.cancelIds.set(cancelId, handle);
      }
      // Detach on settle: a signal shared across many calls would otherwise
      // accumulate one dead listener per finished call, and a stale cancelId
      // could reach a different, unrelated call once `handle` is recycled.
      const detach = (): void => {
        if (onAbort) signal!.removeEventListener("abort", onAbort);
        if (cancelId !== undefined) this.cancelIds.delete(cancelId);
      };
      this.pendingFutures.set(handle, {
        resolve: (h) => {
          detach();
          resolve(h);
        },
        reject: (error) => {
          detach();
          reject(error);
        },
        pollSync,
        panicMessage,
        free,
        cancel,
      });
    });
  }
}

// 3: byte buffers returned from an exported callable cross unframed. Checked
// at instantiation, so a mismatched artifact fails there instead of handing
// back a payload with the old length prefix still on the front.
export const WASM_ABI_VERSION = 3;

export interface BoltFFIExports {
  memory: WebAssembly.Memory;
  boltffi_wasm_abi_version: () => number;
  boltffi_wasm_alloc: (size: number) => number;
  boltffi_wasm_free: (ptr: number, size: number) => void;
  boltffi_wasm_free_buf: (ptr: number, size: number, align: number) => void;
  boltffi_wasm_realloc: (ptr: number, oldSize: number, newSize: number) => number;
  boltffi_wasm_free_string_return: (ptr: number, len: number) => void;
  boltffi_wasm_alloc_owned_bytes: (len: number) => number;
  boltffi_wasm_return_slot_addr: () => number;
  [key: string]: WebAssembly.ExportValue;
}

export interface StringAlloc {
  ptr: number;
  len: number;
}

export interface PrimitiveBufferAlloc {
  ptr: number;
  len: number;
  allocationSize: number;
}

export type PrimitiveBufferElementType =
  | "bool"
  | "i8"
  | "u8"
  | "i16"
  | "u16"
  | "i32"
  | "u32"
  | "i64"
  | "u64"
  | "isize"
  | "usize"
  | "f32"
  | "f64";

export type WriterAlloc = WireWriter;

export class BoltFFIModule {
  readonly exports: BoltFFIExports;
  readonly asyncManager: AsyncFutureManager;
  readonly streamManager: StreamPollManager;
  private _memory: WebAssembly.Memory;
  /** Lent out by `readPackedBuffer`, one read at a time. */
  private readonly borrowedReader = new WireReader(EMPTY_BUFFER, 0, true, 0);
  private borrowedReaderInUse = false;
  private _encoder: TextEncoder;
  private _decoder: TextDecoder;
  private _writerPool: Map<number, WriterAlloc[]>;
  private _cachedU8: Uint8Array | null = null;
  private _cachedI8: Int8Array | null = null;
  private _cachedI16: Int16Array | null = null;
  private _cachedU16: Uint16Array | null = null;
  private _cachedI32: Int32Array | null = null;
  private _cachedU32: Uint32Array | null = null;
  private _cachedI64: BigInt64Array | null = null;
  private _cachedU64: BigUint64Array | null = null;
  private _cachedF32: Float32Array | null = null;
  private _cachedF64: Float64Array | null = null;
  private _cachedView: DataView | null = null;
  private _viewProbe: Uint8Array | null = null;
  private _memoryBuffer: ArrayBuffer | null = null;
  private _packedStorage = new ArrayBuffer(8);
  private _packedBits = new BigUint64Array(this._packedStorage);
  private _packedHalves = new Uint32Array(this._packedStorage);
  private _optionF64Storage = new ArrayBuffer(8);
  private _optionF64Bits = new BigUint64Array(this._optionF64Storage);
  private _optionF64Values = new Float64Array(this._optionF64Storage);
  private _returnSlotAddr: number = 0;
  private _regionAllocator: WasmWireWriterAllocator | null = null;

  constructor(
    instance: WebAssembly.Instance,
    asyncManager: AsyncFutureManager,
    streamManager: StreamPollManager
  ) {
    this.exports = instance.exports as BoltFFIExports;
    this._memory = this.exports.memory;
    this._encoder = new TextEncoder();
    this._decoder = new TextDecoder("utf-8");
    this._writerPool = new Map();
    this.asyncManager = asyncManager;
    this.streamManager = streamManager;
    asyncManager.setModule(this);
    this._returnSlotAddr = this.exports.boltffi_wasm_return_slot_addr();
  }

  readReturnSlot(): { ptr: number; len: number; cap: number; align: number } {
    const view = this.getU32();
    const idx = this._returnSlotAddr >>> 2;
    return { ptr: view[idx], len: view[idx + 1], cap: view[idx + 2], align: view[idx + 3] };
  }

  writeReturnSlot(allocation: PrimitiveBufferAlloc, alignment: number): void {
    const view = this.getU32();
    const index = this._returnSlotAddr >>> 2;
    view[index] = allocation.ptr;
    view[index + 1] = allocation.allocationSize;
    view[index + 2] = allocation.allocationSize;
    view[index + 3] = alignment;
  }

  writeWriterReturnSlot(writer: WriterAlloc, alignment: number): void {
    const view = this.getU32();
    const index = this._returnSlotAddr >>> 2;
    view[index] = writer.ptr;
    view[index + 1] = writer.len;
    view[index + 2] = writer.capacity;
    view[index + 3] = alignment;
  }

  completeAsync<T>(complete: (statusPtr: number) => T): T {
    const statusPtr = this.allocStatus();
    try {
      const result = complete(statusPtr);
      this.checkStatus(this.readStatus(statusPtr));
      return result;
    } finally {
      this.freeStatus(statusPtr);
    }
  }

  private allocStatus(): number {
    const ptr = this.exports.boltffi_wasm_alloc(FFI_STATUS_SIZE);
    if (ptr === 0) {
      throw new Error("Failed to allocate memory for status");
    }
    this.getView().setInt32(ptr, 0, true);
    return ptr;
  }

  private readStatus(ptr: number): number {
    return this.getView().getInt32(ptr, true);
  }

  private freeStatus(ptr: number): void {
    if (ptr !== 0) {
      this.exports.boltffi_wasm_free(ptr, FFI_STATUS_SIZE);
    }
  }

  checkStatus(status: number): void {
    if (status === 0) {
      return;
    }
    if (status === 3) {
      throw new Error("invalid argument");
    }
    if (status === 4) {
      throw new BoltFFICancelledError();
    }
    throw new Error(`FFI failed in async completion with code ${status}`);
  }

  private getView(): DataView {
    // `DataView.byteLength` throws when detached instead of reporting 0, so
    // this cache cannot test itself and carries a probe.
    const probe = this._viewProbe;
    if (probe !== null && probe.byteLength !== 0) {
      return this._cachedView as DataView;
    }
    const buffer = this._memory.buffer;
    const remapped = new DataView(buffer);
    this._cachedView = remapped;
    this._viewProbe = new Uint8Array(buffer);
    return remapped;
  }

  /** `memory.buffer` is an accessor; this is a field read behind a cheap check. */
  private memoryBuffer(): ArrayBuffer {
    this.getBytes();
    return this._memoryBuffer as ArrayBuffer;
  }

  private getBytes(): Uint8Array {
    const cached = this._cachedU8;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Uint8Array(buffer);
    this._cachedU8 = remapped;
    this._memoryBuffer = buffer;
    return remapped;
  }

  private getI8(): Int8Array {
    const cached = this._cachedI8;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Int8Array(buffer);
    this._cachedI8 = remapped;
    return remapped;
  }

  private getI16(): Int16Array {
    const cached = this._cachedI16;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Int16Array(buffer);
    this._cachedI16 = remapped;
    return remapped;
  }

  private getU16(): Uint16Array {
    const cached = this._cachedU16;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Uint16Array(buffer);
    this._cachedU16 = remapped;
    return remapped;
  }

  private getI32(): Int32Array {
    const cached = this._cachedI32;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Int32Array(buffer);
    this._cachedI32 = remapped;
    return remapped;
  }

  private getU32(): Uint32Array {
    const cached = this._cachedU32;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Uint32Array(buffer);
    this._cachedU32 = remapped;
    return remapped;
  }

  private getI64(): BigInt64Array {
    const cached = this._cachedI64;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new BigInt64Array(buffer);
    this._cachedI64 = remapped;
    return remapped;
  }

  private getU64(): BigUint64Array {
    const cached = this._cachedU64;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new BigUint64Array(buffer);
    this._cachedU64 = remapped;
    return remapped;
  }

  private getF32(): Float32Array {
    const cached = this._cachedF32;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Float32Array(buffer);
    this._cachedF32 = remapped;
    return remapped;
  }

  private getF64(): Float64Array {
    const cached = this._cachedF64;
    if (cached !== null && cached.byteLength !== 0) {
      return cached;
    }
    const buffer = this._memory.buffer;
    const remapped = new Float64Array(buffer);
    this._cachedF64 = remapped;
    return remapped;
  }

  allocString(value: string): StringAlloc {
    const encoded = this._encoder.encode(value);
    const ptr = this.exports.boltffi_wasm_alloc(encoded.length);
    if (ptr === 0 && encoded.length > 0) {
      throw new Error("Failed to allocate memory for string");
    }
    this.getBytes().set(encoded, ptr);
    return { ptr, len: encoded.length };
  }

  allocOwnedBytes(value: Uint8Array): StringAlloc {
    const ptr = this.exports.boltffi_wasm_alloc_owned_bytes(value.length);
    if (ptr === 0 && value.length > 0) {
      throw new Error("Failed to allocate owned bytes");
    }
    this.getBytes().set(value, ptr);
    return { ptr, len: value.length };
  }

  allocOwnedString(value: string): StringAlloc {
    const len = value.length;
    const ptr = this.exports.boltffi_wasm_alloc_owned_bytes(len);
    if (ptr === 0 && len > 0) {
      throw new Error("Failed to allocate owned string");
    }
    const bytes = new Uint8Array(this._memory.buffer, ptr, len);
    const encoded = this._encoder.encodeInto(value, bytes);
    if (encoded.read === value.length && encoded.written === len) {
      return { ptr, len };
    }
    bytes.fill(0, encoded.written);
    this.exports.boltffi_wasm_free_string_return(ptr, len);
    return this.allocOwnedBytes(this._encoder.encode(value));
  }

  allocOwnedWireString(value: string): StringAlloc {
    const encoded = this._encoder.encode(value);
    const allocation = this.allocOwnedBytes(new Uint8Array(4 + encoded.length));
    this.getView().setUint32(allocation.ptr, encoded.length, true);
    this.getBytes().set(encoded, allocation.ptr + 4);
    return allocation;
  }

  allocWireString(value: string): StringAlloc {
    const len = 4 + value.length;
    const ptr = this.exports.boltffi_wasm_alloc(len);
    if (ptr === 0) {
      throw new Error("Failed to allocate memory for wire string");
    }
    const encoded = this._encoder.encodeInto(
      value,
      new Uint8Array(this._memory.buffer, ptr + 4, value.length)
    );
    if (encoded.read !== value.length) {
      this.exports.boltffi_wasm_free(ptr, len);
      return this.allocWireBytes(this._encoder.encode(value));
    }
    this.getView().setUint32(ptr, encoded.written, true);
    return { ptr, len: 4 + encoded.written };
  }

  allocWireBytes(value: Uint8Array): StringAlloc {
    const len = 4 + value.length;
    const ptr = this.exports.boltffi_wasm_alloc(len);
    if (ptr === 0) {
      throw new Error("Failed to allocate memory for wire value");
    }
    this.getView().setUint32(ptr, value.length, true);
    this.getBytes().set(value, ptr + 4);
    return { ptr, len };
  }

  freeAlloc(alloc: StringAlloc): void {
    if (alloc.ptr !== 0 && alloc.len !== 0) {
      this.exports.boltffi_wasm_free(alloc.ptr, alloc.len);
    }
  }

  allocBytes(value: Uint8Array): StringAlloc {
    const ptr = this.exports.boltffi_wasm_alloc(value.length);
    if (ptr === 0 && value.length > 0) {
      throw new Error("Failed to allocate memory for bytes");
    }
    this.getBytes().set(value, ptr);
    return { ptr, len: value.length };
  }

  allocStreamBuffer(itemCapacity: number, itemSize: number): StringAlloc {
    const len = itemCapacity * itemSize;
    const ptr = this.exports.boltffi_wasm_alloc(len);
    if (ptr === 0 && len > 0) {
      throw new Error("Failed to allocate stream buffer");
    }
    return { ptr, len };
  }

  allocI8Array(value: Int8Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    new Int8Array(this._memory.buffer, ptr, len).set(value);
    return { ptr, len, allocationSize: byteLen };
  }

  borrowBoolArray(ptr: number, len: number): boolean[] {
    return toBoolArray(this.getBytes().subarray(ptr, ptr + len));
  }

  borrowI8Array(ptr: number, len: number): Int8Array {
    return this.getI8().subarray(ptr, ptr + len);
  }

  borrowI16Array(ptr: number, len: number): Int16Array {
    return this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + len);
  }

  borrowU16Array(ptr: number, len: number): Uint16Array {
    return this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + len);
  }

  borrowI32Array(ptr: number, len: number): Int32Array {
    return this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + len);
  }

  borrowU32Array(ptr: number, len: number): Uint32Array {
    return this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + len);
  }

  borrowI64Array(ptr: number, len: number): BigInt64Array {
    return this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + len);
  }

  borrowU64Array(ptr: number, len: number): BigUint64Array {
    return this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + len);
  }

  borrowF32Array(ptr: number, len: number): Float32Array {
    return this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + len);
  }

  borrowF64Array(ptr: number, len: number): Float64Array {
    return this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + len);
  }

  allocU8Array(value: Uint8Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const ptr = this.exports.boltffi_wasm_alloc(len);
    this.getBytes().set(value, ptr);
    return { ptr, len, allocationSize: len };
  }

  allocI16Array(value: Int16Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 1;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getI16().set(value, ptr >>> 1);
    return { ptr, len, allocationSize: byteLen };
  }

  allocU16Array(value: Uint16Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 1;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getU16().set(value, ptr >>> 1);
    return { ptr, len, allocationSize: byteLen };
  }

  allocI32Array(value: Int32Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 2;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getI32().set(value, ptr >>> 2);
    return { ptr, len, allocationSize: byteLen };
  }

  allocU32Array(value: Uint32Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 2;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getU32().set(value, ptr >>> 2);
    return { ptr, len, allocationSize: byteLen };
  }

  allocI64Array(value: BigInt64Array | readonly bigint[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 3;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getI64().set(value, ptr >>> 3);
    return { ptr, len, allocationSize: byteLen };
  }

  allocU64Array(value: BigUint64Array | readonly bigint[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 3;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getU64().set(value, ptr >>> 3);
    return { ptr, len, allocationSize: byteLen };
  }

  allocF32Array(value: Float32Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 2;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getF32().set(value, ptr >>> 2);
    return { ptr, len, allocationSize: byteLen };
  }

  allocF64Array(value: Float64Array | readonly number[]): PrimitiveBufferAlloc {
    const len = value.length;
    const byteLen = len << 3;
    const ptr = this.exports.boltffi_wasm_alloc(byteLen);
    this.getF64().set(value, ptr >>> 3);
    return { ptr, len, allocationSize: byteLen };
  }

  allocBoolArray(value: readonly boolean[]): PrimitiveBufferAlloc {
    const len = value.length;
    const ptr = this.exports.boltffi_wasm_alloc(len);
    const view = new Uint8Array(this._memory.buffer, ptr, len);
    for (let i = 0; i < len; i++) {
      view[i] = value[i] ? 1 : 0;
    }
    return { ptr, len, allocationSize: len };
  }

  allocPrimitiveBuffer(
    value: ReadonlyArray<number | bigint | boolean>,
    elementType: PrimitiveBufferElementType
  ): PrimitiveBufferAlloc {
    const bytesPerElement = this.primitiveElementSize(elementType);
    const elementCount = value.length;
    const allocationSize = elementCount * bytesPerElement;
    const ptr = this.exports.boltffi_wasm_alloc(allocationSize);
    if (ptr === 0 && allocationSize > 0) {
      throw new Error("Failed to allocate memory for primitive buffer");
    }
    const view = this.getView();
    value.forEach((entry, index) => {
      const offset = ptr + index * bytesPerElement;
      this.writePrimitiveElement(view, offset, entry, elementType);
    });
    return { ptr, len: elementCount, allocationSize };
  }

  allocCompositeBuffer<T>(
    value: readonly T[],
    elementSize: number,
    writeElement: (writer: WriterAlloc, value: T) => void
  ): WriterAlloc {
    const writer = this.allocWriter(value.length * elementSize);
    value.forEach((entry) => writeElement(writer, entry));
    return writer;
  }

  borrowRecordArray<T>(
    ptr: number,
    byteLen: number,
    stride: number,
    decode: (reader: WireReader) => T
  ): T[] {
    if (ptr === 0 || byteLen === 0) return [];
    if (byteLen % stride !== 0) {
      throw new Error(`Invalid record array byte length ${byteLen} for stride ${stride}`);
    }
    const count = byteLen / stride;
    const result: T[] = new Array(count);
    const reader = new WireReader(this._memory.buffer, ptr);
    for (let index = 0; index < count; index++) {
      result[index] = decode(reader);
    }
    return result;
  }

  freePrimitiveBuffer(allocation: PrimitiveBufferAlloc): void {
    if (allocation.ptr !== 0 && allocation.allocationSize !== 0) {
      this.exports.boltffi_wasm_free(allocation.ptr, allocation.allocationSize);
    }
  }

  copyPrimitiveBufferInto(
    allocation: PrimitiveBufferAlloc,
    target: Int8Array | Int16Array | Uint16Array | Int32Array | Uint32Array | BigInt64Array | BigUint64Array | Float32Array | Float64Array,
    elementType: Exclude<PrimitiveBufferElementType, "bool" | "u8">
  ): void {
    const { ptr, len } = allocation;
    switch (elementType) {
      case "i8":
        (target as Int8Array).set(this.getI8().subarray(ptr, ptr + len));
        return;
      case "i16":
        (target as Int16Array).set(this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + len));
        return;
      case "u16":
        (target as Uint16Array).set(this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + len));
        return;
      case "i32":
      case "isize":
        (target as Int32Array).set(this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + len));
        return;
      case "u32":
      case "usize":
        (target as Uint32Array).set(this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + len));
        return;
      case "i64":
        (target as BigInt64Array).set(this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + len));
        return;
      case "u64":
        (target as BigUint64Array).set(this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + len));
        return;
      case "f32":
        (target as Float32Array).set(this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + len));
        return;
      case "f64":
        (target as Float64Array).set(this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + len));
    }
  }

  allocWriter(size: number): WriterAlloc {
    const requestedCapacity = Math.max(size, MIN_WRITER_CAPACITY);
    const pooled = this._writerPool.get(requestedCapacity);
    if (pooled !== undefined) {
      const writer = pooled.pop();
      if (writer !== undefined) {
        writer.reset();
        return writer;
      }
    }

    const allocator: WasmWireWriterAllocator = {
      alloc: (allocationSize) => this.exports.boltffi_wasm_alloc(allocationSize),
      realloc: (ptr, oldSize, newSize) =>
        this.exports.boltffi_wasm_realloc(ptr, oldSize, newSize),
      free: (ptr, allocationSize) => this.exports.boltffi_wasm_free(ptr, allocationSize),
      buffer: () => this._memory.buffer,
    };
    return WireWriter.withWasmAllocation(requestedCapacity, allocator);
  }

  allocOwnedWriter(size: number): WriterAlloc {
    const allocator: WasmWireWriterAllocator = {
      alloc: (allocationSize) => this.exports.boltffi_wasm_alloc_owned_bytes(allocationSize),
      realloc: () => {
        throw new Error("Owned writer exceeded its size plan");
      },
      free: (ptr, allocationSize) =>
        this.exports.boltffi_wasm_free_string_return(ptr, allocationSize),
      buffer: () => this._memory.buffer,
    };
    return WireWriter.withWasmAllocation(size, allocator);
  }

  freeWriter(writer: WriterAlloc): void {
    const capacity = writer.capacity;
    writer.reset();
    const bucket = this._writerPool.get(capacity) ?? [];
    if (bucket.length < MAX_WRITERS_PER_CAPACITY) {
      bucket.push(writer);
      this._writerPool.set(capacity, bucket);
      return;
    }
    writer.release();
  }

  readerFromWriter(writer: WriterAlloc): WireReader {
    return new WireReader(this._memory.buffer, writer.ptr);
  }

  writerFromMemory(ptr: number, size: number): WriterAlloc {
    // Callback trampolines call this once per element, so the allocator is
    // built once and shared rather than rebuilt with four closures per call.
    let allocator = this._regionAllocator;
    if (allocator === null) {
      allocator = WireWriter.fixedRegionAllocator(() => this._memory.buffer);
      this._regionAllocator = allocator;
    }
    return WireWriter.withWasmRegion(ptr, size, allocator.buffer, allocator);
  }

  allocBufDescriptor(): number {
    const ptr = this.exports.boltffi_wasm_alloc(FFI_BUF_DESCRIPTOR_SIZE);
    if (ptr === 0) {
      throw new Error("Failed to allocate memory for buffer descriptor");
    }
    new Uint8Array(this._memory.buffer, ptr, FFI_BUF_DESCRIPTOR_SIZE).fill(0);
    return ptr;
  }

  freeBufDescriptor(ptr: number): void {
    if (ptr !== 0) {
      this.exports.boltffi_wasm_free(ptr, FFI_BUF_DESCRIPTOR_SIZE);
    }
  }

  readerFromBuf(bufPtr: number): WireReader {
    const view = this.getView();
    const ptr = view.getUint32(bufPtr, true);
    return new WireReader(this._memory.buffer, ptr);
  }

  freeBuf(bufPtr: number): void {
    const { ptr, cap, align } = this.readBufDescriptor(bufPtr);
    if (ptr !== 0 && cap !== 0) {
      this.exports.boltffi_wasm_free_buf(ptr, cap, align);
    }
    this.exports.boltffi_wasm_free(bufPtr, FFI_BUF_DESCRIPTOR_SIZE);
  }

  writeBufDescriptor(bufPtr: number, dataPtr: number, dataLen: number, dataCap: number, dataAlign: number = 1): void {
    const view = this.getView();
    view.setUint32(bufPtr, dataPtr, true);
    view.setUint32(bufPtr + 4, dataLen, true);
    view.setUint32(bufPtr + 8, dataCap, true);
    view.setUint32(bufPtr + 12, dataAlign, true);
  }

  writeCallbackBuffer(bufPtr: number, dataPtr: number, dataLen: number, dataCap: number): void {
    const view = this.getView();
    view.setUint32(bufPtr, dataPtr, true);
    view.setUint32(bufPtr + 4, dataLen, true);
    view.setUint32(bufPtr + 8, dataCap, true);
  }

  private readBufDescriptor(bufPtr: number): { ptr: number; len: number; cap: number; align: number } {
    const view = this.getView();
    return {
      ptr: view.getUint32(bufPtr, true),
      len: view.getUint32(bufPtr + 4, true),
      cap: view.getUint32(bufPtr + 8, true),
      align: view.getUint32(bufPtr + 12, true) || 1,
    };
  }

  takeBufU8Array(bufPtr: number): Uint8Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Uint8Array(0);
    return this.getBytes().subarray(ptr, ptr + len).slice();
  }

  takeBufI8Array(bufPtr: number): Int8Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Int8Array(0);
    return this.getI8().subarray(ptr, ptr + len).slice();
  }

  takeBufI16Array(bufPtr: number): Int16Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Int16Array(0);
    const elemCount = len >>> 1;
    return this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
  }

  takeBufU16Array(bufPtr: number): Uint16Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Uint16Array(0);
    const elemCount = len >>> 1;
    return this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
  }

  takeBufI32Array(bufPtr: number): Int32Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Int32Array(0);
    const elemCount = len >>> 2;
    return this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
  }

  takeBufU32Array(bufPtr: number): Uint32Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Uint32Array(0);
    const elemCount = len >>> 2;
    return this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
  }

  takeBufI64Array(bufPtr: number): BigInt64Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new BigInt64Array(0);
    const elemCount = len >>> 3;
    return this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
  }

  takeBufU64Array(bufPtr: number): BigUint64Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new BigUint64Array(0);
    const elemCount = len >>> 3;
    return this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
  }

  takeBufF32Array(bufPtr: number): Float32Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Float32Array(0);
    const elemCount = len >>> 2;
    return this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
  }

  takeBufF64Array(bufPtr: number): Float64Array {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return new Float64Array(0);
    const elemCount = len >>> 3;
    return this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
  }

  takeBufBoolArray(bufPtr: number): boolean[] {
    const { ptr, len } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return [];
    return toBoolArray(this.getBytes().subarray(ptr, ptr + len));
  }

  takeBufStructArray<T>(bufPtr: number, stride: number, decode: (view: DataView, offset: number) => T): T[] {
    const { ptr, len: byteLen } = this.readBufDescriptor(bufPtr);
    if (ptr === 0) return [];
    const copy = new Uint8Array(this._memory.buffer, ptr, byteLen).slice();
    const view = new DataView(copy.buffer, copy.byteOffset, copy.byteLength);
    const count = (byteLen / stride) | 0;
    const result = new Array<T>(count);
    for (let index = 0; index < count; index++) {
      result[index] = decode(view, index * stride);
    }
    return result;
  }

  writeToMemory(ptr: number, data: Uint8Array): void {
    this.getBytes().set(data, ptr);
  }

  writeI32(ptr: number, value: number): void {
    this.getView().setInt32(ptr, value, true);
  }

  writeU64(ptr: number, value: bigint): void {
    this.getView().setBigUint64(ptr, value, true);
  }

  readFromMemory(ptr: number, len: number): Uint8Array {
    return this.getBytes().slice(ptr, ptr + len);
  }

  readerFromMemory(ptr: number, len: number): WireReader {
    const bytes = this.readFromMemory(ptr, len);
    return new WireReader(bytes.buffer as ArrayBuffer, bytes.byteOffset);
  }

  private unpackPacked(packed: bigint): { pointer: number; length: number } {
    // Masking and shifting a BigInt allocates an intermediate per operation.
    // Storing it once and reading the halves as u32 through an aliased view
    // does no BigInt arithmetic at all — but the aliasing is host-endian, so
    // which half is which is settled once, at load.
    this._packedBits[0] = packed;
    const halves = this._packedHalves;
    return { pointer: halves[PACKED_LOW], length: halves[PACKED_HIGH] };
  }

  private freePacked(pointer: number, length: number): void {
    if (pointer !== 0 && length !== 0) {
      this.exports.boltffi_wasm_free_string_return(pointer, length);
    }
  }

  private takePackedOptionalPrimitive(
    packed: bigint,
    encodedSize: number,
    primitive: PackedPrimitive
  ): boolean | number | bigint | null {
    const { pointer, length } = this.unpackPacked(packed);
    if (pointer === 0 || length === 0) {
      return null;
    }
    try {
      const view = this.getView();
      if (view.getUint8(pointer) === 0) {
        return null;
      }
      if (length < 1 + encodedSize) {
        throw new Error("Invalid packed optional payload");
      }
      const valueOffset = pointer + 1;
      switch (primitive) {
        case PackedPrimitive.Bool:
          return view.getUint8(valueOffset) !== 0;
        case PackedPrimitive.I8:
          return view.getInt8(valueOffset);
        case PackedPrimitive.U8:
          return view.getUint8(valueOffset);
        case PackedPrimitive.I16:
          return view.getInt16(valueOffset, true);
        case PackedPrimitive.U16:
          return view.getUint16(valueOffset, true);
        case PackedPrimitive.I32:
          return view.getInt32(valueOffset, true);
        case PackedPrimitive.U32:
          return view.getUint32(valueOffset, true);
        case PackedPrimitive.I64:
          return view.getBigInt64(valueOffset, true);
        case PackedPrimitive.U64:
          return view.getBigUint64(valueOffset, true);
        case PackedPrimitive.F32:
          return view.getFloat32(valueOffset, true);
        case PackedPrimitive.F64:
          return view.getFloat64(valueOffset, true);
      }
    } finally {
      this.freePacked(pointer, length);
    }
  }

  takePackedOptionalBool(packed: bigint): boolean | null {
    return this.takePackedOptionalPrimitive(packed, 1, PackedPrimitive.Bool) as boolean | null;
  }

  takePackedOptionalI8(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 1, PackedPrimitive.I8) as number | null;
  }

  takePackedOptionalU8(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 1, PackedPrimitive.U8) as number | null;
  }

  takePackedOptionalI16(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 2, PackedPrimitive.I16) as number | null;
  }

  takePackedOptionalU16(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 2, PackedPrimitive.U16) as number | null;
  }

  takePackedOptionalI32(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 4, PackedPrimitive.I32) as number | null;
  }

  takePackedOptionalU32(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 4, PackedPrimitive.U32) as number | null;
  }

  takePackedOptionalI64(packed: bigint): bigint | null {
    return this.takePackedOptionalPrimitive(packed, 8, PackedPrimitive.I64) as bigint | null;
  }

  takePackedOptionalU64(packed: bigint): bigint | null {
    return this.takePackedOptionalPrimitive(packed, 8, PackedPrimitive.U64) as bigint | null;
  }

  takePackedOptionalF32(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 4, PackedPrimitive.F32) as number | null;
  }

  takePackedOptionalF64(packed: bigint): number | null {
    return this.takePackedOptionalPrimitive(packed, 8, PackedPrimitive.F64) as number | null;
  }

  unpackOptionBool(packed: number): boolean | null {
    if (Number.isNaN(packed)) return null;
    return packed !== 0;
  }

  unpackOptionI8(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed | 0;
  }

  unpackOptionU8(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed >>> 0;
  }

  unpackOptionI16(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed | 0;
  }

  unpackOptionU16(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed >>> 0;
  }

  unpackOptionI32(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed | 0;
  }

  unpackOptionU32(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed >>> 0;
  }

  packOptionScalar(value: number | boolean | null): number {
    if (value === null) return Number.NaN;
    if (typeof value === "boolean") return value ? 1 : 0;
    return value;
  }

  packOptionF64Bits(value: number | null): bigint {
    if (value === null) return OPTION_F64_NONE;
    if (Number.isNaN(value)) return OPTION_F64_NAN;
    this._optionF64Values[0] = value;
    return this._optionF64Bits[0];
  }

  unpackOptionF64Bits(packed: bigint): number | null {
    if (packed === OPTION_F64_NONE) return null;
    this._optionF64Bits[0] = packed;
    return this._optionF64Values[0];
  }

  unpackOptionF32(packed: number): number | null {
    if (Number.isNaN(packed)) return null;
    return packed;
  }

  unpackOptionF64(packed: number): number | null {
    if (!Number.isNaN(packed)) return packed;
    const slotIndex = this._returnSlotAddr >>> 2;
    return this.getU32()[slotIndex] === 0 ? null : packed;
  }

  takePackedUtf8String(packed: bigint): string {
    const { pointer, length } = this.unpackPacked(packed);
    if (pointer === 0 || length === 0) {
      return "";
    }
    const bytes = new Uint8Array(this._memory.buffer, pointer, length);
    try {
      return this._decoder.decode(bytes);
    } finally {
      this.freePacked(pointer, length);
    }
  }

  /**
   * Takes a returned byte buffer whose length came with the call.
   *
   * The framed counterpart, `takePackedWireBytes`, reads a `u32` the buffer
   * carries and then checks it equals `length - 4`, so the prefix never told
   * it anything the packed value had not. Writing that prefix costs the Rust
   * side a shift of the whole payload, which is why this shape exists.
   */
  takePackedBytes(packed: bigint): Uint8Array {
    const { pointer, length } = this.unpackPacked(packed);
    if (pointer === 0 || length === 0) {
      return new Uint8Array(0);
    }
    try {
      const bytes = this.getBytes();
      if (pointer + length > bytes.length) {
        throw new Error("Invalid packed bytes length");
      }
      // One allocation: building a view and then copying it made two.
      return bytes.slice(pointer, pointer + length);
    } finally {
      this.freePacked(pointer, length);
    }
  }

  takePackedWireString(packed: bigint): string {
    const { pointer, length } = this.unpackPacked(packed);
    if (pointer === 0 || length < 4) {
      throw new Error("Invalid packed wire string");
    }
    try {
      const bytes = this.getBytes();
      const payloadLength = this.getView().getUint32(pointer, true);
      if (payloadLength !== length - 4 || pointer + length > bytes.length) {
        throw new Error("Invalid packed wire string length");
      }
      const start = pointer + 4;
      return this._decoder.decode(bytes.subarray(start, start + payloadLength));
    } finally {
      this.freePacked(pointer, length);
    }
  }

  takePackedWireBytes(packed: bigint): Uint8Array {
    const { pointer, length } = this.unpackPacked(packed);
    if (pointer === 0 || length < 4) {
      throw new Error("Invalid packed wire bytes");
    }
    try {
      const bytes = this.getBytes();
      const payloadLength = this.getView().getUint32(pointer, true);
      if (payloadLength !== length - 4 || pointer + length > bytes.length) {
        throw new Error("Invalid packed wire bytes length");
      }
      const start = pointer + 4;
      // One allocation: building a view and then copying it made two.
      return bytes.slice(start, start + payloadLength);
    } finally {
      this.freePacked(pointer, length);
    }
  }



  /**
   * Reads a packed return in place, without copying it out of wasm memory.
   *
   * `takePackedBuffer` copies the payload into a fresh `ArrayBuffer` and wraps
   * it in a new `DataView` on every call, which dominates the cost of decoding
   * a small record. Here the reader borrows wasm memory instead, and the
   * payload is freed once `read` returns.
   *
   * Safe because the reader is in borrowed mode: reads that would hand out a
   * view over that memory copy instead, so nothing the callback returns can
   * outlive the free below. The reader spans exactly the payload, so a
   * malformed length throws rather than reading past it, and it is detached
   * afterwards, so a reader that escapes `read` throws rather than reading
   * freed memory. `read` returning a promise is still the caller's problem:
   * the payload is freed when `read` returns, not when the promise settles.
   */
  readPackedBuffer<T>(packed: bigint, read: (reader: WireReader) => T): T {
    const { pointer, length } = this.unpackPacked(packed);
    const empty = pointer === 0 || length === 0;
    const buffer = empty ? EMPTY_BUFFER : this.memoryBuffer();
    const start = empty ? 0 : pointer;
    const size = empty ? 0 : length;

    // A nested call would reset the reader the outer one is still using, so it
    // gets its own. Generated codecs never nest, but the method is public.
    if (this.borrowedReaderInUse) {
      const reader = new WireReader(buffer, start, true, size);
      try {
        return read(reader);
      } finally {
        reader.invalidate();
        if (!empty) this.freePacked(pointer, length);
      }
    }

    this.borrowedReaderInUse = true;
    try {
      return read(this.borrowedReader.reset(buffer, start, size, true));
    } finally {
      this.borrowedReader.invalidate();
      this.borrowedReaderInUse = false;
      if (!empty) this.freePacked(pointer, length);
    }
  }

  takePackedBuffer(packed: bigint): WireReader {
    const { pointer, length } = this.unpackPacked(packed);
    if (pointer === 0 || length === 0) {
      return new WireReader(new ArrayBuffer(0), 0);
    }
    const bytes = new Uint8Array(this._memory.buffer, pointer, length);
    const copy = bytes.slice();
    this.freePacked(pointer, length);
    return new WireReader(copy.buffer, 0);
  }

  takePackedI8Array(packed: bigint): Int8Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Int8Array(0);
    const result = this.getI8().subarray(pointer, pointer + byteLen).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedU8Array(packed: bigint): Uint8Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Uint8Array(0);
    const result = this.getBytes().subarray(pointer, pointer + byteLen).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  private readSlot(): { ptr: number; len: number; cap: number; align: number } {
    const slotView = this.getU32();
    const slotIdx = this._returnSlotAddr >>> 2;
    return {
      ptr: slotView[slotIdx],
      len: slotView[slotIdx + 1],
      cap: slotView[slotIdx + 2],
      align: slotView[slotIdx + 3] || 1,
    };
  }

  private freeSlotBuf(ptr: number, cap: number, align: number): void {
    this.exports.boltffi_wasm_free_buf(ptr, cap, align);
  }

  takeSlotU8Array(): Uint8Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Uint8Array(0);
    const result = this.getBytes().subarray(ptr, ptr + len).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotI8Array(): Int8Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Int8Array(0);
    const result = this.getI8().subarray(ptr, ptr + len).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotI32Array(): Int32Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Int32Array(0);
    const elemCount = len >>> 2;
    const result = this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotU32Array(): Uint32Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Uint32Array(0);
    const elemCount = len >>> 2;
    const result = this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotF32Array(): Float32Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Float32Array(0);
    const elemCount = len >>> 2;
    const result = this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotF64Array(): Float64Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Float64Array(0);
    const elemCount = len >>> 3;
    const result = this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotI16Array(): Int16Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Int16Array(0);
    const elemCount = len >>> 1;
    const result = this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotU16Array(): Uint16Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new Uint16Array(0);
    const elemCount = len >>> 1;
    const result = this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotI64Array(): BigInt64Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new BigInt64Array(0);
    const elemCount = len >>> 3;
    const result = this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotU64Array(): BigUint64Array {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return new BigUint64Array(0);
    const elemCount = len >>> 3;
    const result = this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotBoolArray(): boolean[] {
    const { ptr, len, cap, align } = this.readSlot();
    if (ptr === 0) return [];
    const result = toBoolArray(this.getBytes().subarray(ptr, ptr + len));
    this.freeSlotBuf(ptr, cap, align);
    return result;
  }

  takeSlotStructArray<T>(stride: number, decode: (view: DataView, offset: number) => T): T[] {
    const { ptr, len: byteLen, cap, align } = this.readSlot();
    if (ptr === 0) return [];
    const count = (byteLen / stride) | 0;
    const copy = new Uint8Array(this._memory.buffer, ptr, byteLen).slice();
    this.freeSlotBuf(ptr, cap, align);
    const view = new DataView(copy.buffer, copy.byteOffset, copy.byteLength);
    const result: T[] = new Array(count);
    for (let i = 0; i < count; i++) {
      result[i] = decode(view, i * stride);
    }
    return result;
  }

  takeSlotRecordArray<T>(stride: number, decode: (reader: WireReader) => T): T[] {
    const { ptr, len: byteLen, cap, align } = this.readSlot();
    if (ptr === 0) return [];
    try {
      return this.borrowRecordArray(ptr, byteLen, stride, decode);
    } finally {
      this.freeSlotBuf(ptr, cap, align);
    }
  }

  takePackedI16Array(packed: bigint): Int16Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Int16Array(0);
    const elemCount = byteLen / 2;
    const result = new Int16Array(this._memory.buffer, pointer, elemCount).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedU16Array(packed: bigint): Uint16Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Uint16Array(0);
    const elemCount = byteLen / 2;
    const result = new Uint16Array(this._memory.buffer, pointer, elemCount).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedI32Array(packed: bigint): Int32Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Int32Array(0);
    const elemCount = byteLen / 4;
    const result = this.getI32().subarray(pointer / 4, pointer / 4 + elemCount).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedU32Array(packed: bigint): Uint32Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Uint32Array(0);
    const elemCount = byteLen / 4;
    const result = this.getU32().subarray(pointer / 4, pointer / 4 + elemCount).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedI64Array(packed: bigint): BigInt64Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new BigInt64Array(0);
    const result = new BigInt64Array(this._memory.buffer, pointer, byteLen / 8).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedU64Array(packed: bigint): BigUint64Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new BigUint64Array(0);
    const result = new BigUint64Array(this._memory.buffer, pointer, byteLen / 8).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedF32Array(packed: bigint): Float32Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Float32Array(0);
    const elemCount = byteLen / 4;
    const result = this.getF32().subarray(pointer / 4, pointer / 4 + elemCount).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  takePackedF64Array(packed: bigint): Float64Array {
    const { pointer, length: byteLen } = this.unpackPacked(packed);
    if (pointer === 0 || byteLen === 0) return new Float64Array(0);
    const elemCount = byteLen / 8;
    const result = this.getF64().subarray(pointer / 8, pointer / 8 + elemCount).slice();
    this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
    return result;
  }

  private primitiveElementSize(elementType: PrimitiveBufferElementType): number {
    switch (elementType) {
      case "bool":
      case "i8":
      case "u8":
        return 1;
      case "i16":
      case "u16":
        return 2;
      case "i32":
      case "u32":
      case "isize":
      case "usize":
      case "f32":
        return 4;
      case "i64":
      case "u64":
      case "f64":
        return 8;
    }
  }

  private writePrimitiveElement(
    view: DataView,
    offset: number,
    value: number | bigint | boolean,
    elementType: PrimitiveBufferElementType
  ): void {
    switch (elementType) {
      case "bool":
        view.setUint8(offset, value ? 1 : 0);
        return;
      case "i8":
        view.setInt8(offset, Number(value));
        return;
      case "u8":
        view.setUint8(offset, Number(value));
        return;
      case "i16":
        view.setInt16(offset, Number(value), true);
        return;
      case "u16":
        view.setUint16(offset, Number(value), true);
        return;
      case "i32":
      case "isize":
        view.setInt32(offset, Number(value), true);
        return;
      case "u32":
      case "usize":
        view.setUint32(offset, Number(value), true);
        return;
      case "i64":
        view.setBigInt64(offset, BigInt(value), true);
        return;
      case "u64":
        view.setBigUint64(offset, BigInt(value), true);
        return;
      case "f32":
        view.setFloat32(offset, Number(value), true);
        return;
      case "f64":
        view.setFloat64(offset, Number(value), true);
        return;
    }
  }
}

export interface BoltFFIImports {
  env?: Record<string, WebAssembly.ImportValue>;
}

function createUnimplementedImport(importName: string): (...args: unknown[]) => never {
  return () => {
    throw new Error(`Unimplemented wasm import: ${importName}`);
  };
}

function createImportModuleProxy(moduleName: string): Record<string, WebAssembly.ImportValue> {
  return new Proxy(
    {},
    {
      get: (_target, propertyName) =>
        createUnimplementedImport(`${moduleName}.${String(propertyName)}`),
    }
  );
}

export async function instantiateBoltFFI(
  source: BufferSource | Response | WebAssembly.Module,
  expectedVersion: number,
  imports?: BoltFFIImports
): Promise<BoltFFIModule> {
  const asyncManager = new AsyncFutureManager();
  const streamManager = new StreamPollManager();

  const importObject: WebAssembly.Imports = {
    env: {
      __boltffi_wake: (handle: number) => asyncManager.wake(handle),
      __boltffi_stream_wake: (handle: number, result: number) => streamManager.wake(handle, result),
      ...(imports?.env ?? {}),
    },
    __wbindgen_placeholder__: createImportModuleProxy("__wbindgen_placeholder__"),
    __wbindgen_externref_xform__: createImportModuleProxy("__wbindgen_externref_xform__"),
  };

  let instance: WebAssembly.Instance;
  if (source instanceof WebAssembly.Module) {
    instance = await WebAssembly.instantiate(source, importObject);
  } else {
    const wasmSource = source instanceof Response ? await source.arrayBuffer() : source;
    ({ instance } = await WebAssembly.instantiate(wasmSource, importObject));
  }
  const module = new BoltFFIModule(instance, asyncManager, streamManager);

  const actualVersion = module.exports.boltffi_wasm_abi_version();
  if (actualVersion !== expectedVersion) {
    throw new Error(
      `BoltFFI ABI version mismatch: expected ${expectedVersion}, got ${actualVersion}`
    );
  }

  return module;
}

export function instantiateBoltFFISync(
  source: BufferSource,
  expectedVersion: number,
  imports?: BoltFFIImports
): BoltFFIModule {
  const asyncManager = new AsyncFutureManager();
  const streamManager = new StreamPollManager();

  const importObject: WebAssembly.Imports = {
    env: {
      __boltffi_wake: (handle: number) => asyncManager.wake(handle),
      __boltffi_stream_wake: (handle: number, result: number) => streamManager.wake(handle, result),
      ...(imports?.env ?? {}),
    },
    __wbindgen_placeholder__: createImportModuleProxy("__wbindgen_placeholder__"),
    __wbindgen_externref_xform__: createImportModuleProxy("__wbindgen_externref_xform__"),
  };

  const wasmModule = new WebAssembly.Module(source);
  const instance = new WebAssembly.Instance(wasmModule, importObject);
  const module = new BoltFFIModule(instance, asyncManager, streamManager);

  const actualVersion = module.exports.boltffi_wasm_abi_version();
  if (actualVersion !== expectedVersion) {
    throw new Error(
      `BoltFFI ABI version mismatch: expected ${expectedVersion}, got ${actualVersion}`
    );
  }

  return module;
}
