const UTF8_DECODER = new TextDecoder("utf-8");
const UTF8_ENCODER = new TextEncoder();
// 16 is where the two curves cross: below it building the string wins on time,
// above it TextDecoder does. Allocation favours the fast path at every length.
const ASCII_FAST_PATH_LIMIT = 16;
const ASCII_SCRATCH: number[] = [];

type TypedArrayConstructor<T extends ArrayBufferView> = {
  new (buffer: ArrayBufferLike, byteOffset: number, length: number): T;
  new (buffer: ArrayBufferLike): T;
  readonly BYTES_PER_ELEMENT: number;
};

export class WireReader {
  private view: DataView;
  private offset: number;
  /**
   * True when the view points at wasm memory the caller still owns, rather
   * than at a private copy.
   *
   * Reads that would otherwise hand out a view over that memory copy instead,
   * so a returned array cannot outlive the buffer it borrows from. Everything
   * else — scalars, strings, `readBytes` — already produces owned values.
   */
  private borrowed: boolean;
  private bytes: Uint8Array | null = null;
  /** The reader's buffer, held directly to keep `view.buffer` off hot paths. */
  private bufferRef: ArrayBuffer;
  /**
   * `limit` is the end of the payload. A bounded `DataView` would enforce it
   * for free, but allocating one per call costs more than the checks do.
   */
  private limit: number;

  constructor(
    buffer: ArrayBuffer,
    offset = 0,
    borrowed = false,
    length?: number
  ) {
    this.view = new DataView(buffer);
    this.bufferRef = buffer;
    this.offset = offset;
    this.limit = length === undefined ? buffer.byteLength : offset + length;
    this.borrowed = borrowed;
  }

  /** Points an existing reader at another payload, reusing the instance. */
  reset(
    buffer: ArrayBuffer,
    offset: number,
    length: number,
    borrowed: boolean
  ): this {
    // Comparing `this.view.buffer` here would read two accessors per call.
    if (this.bufferRef !== buffer) {
      this.view = new DataView(buffer);
      this.bufferRef = buffer;
      // `asBytes` only notices detachment, not a swap between two live
      // buffers, so the cache has to be dropped here.
      this.bytes = null;
    }
    this.offset = offset;
    this.limit = offset + length;
    this.borrowed = borrowed;
    return this;
  }

  /**
   * Detaches the reader from whatever it was pointing at, so a reader that
   * outlives its payload throws rather than reading freed memory. Costs
   * nothing — the empty view is shared.
   */
  invalidate(): void {
    // Every read goes through `take`, so an empty window is enough to make one
    // throw. Keeping the view and buffer lets the next `reset` reuse them.
    this.offset = 0;
    this.limit = 0;
  }

  /**
   * Reserves `byteLength` bytes and returns their absolute index in the
   * underlying buffer. Reads that build a typed array bypass the view's own
   * bounds check, so they go through here to keep it.
   */
  private take(byteLength: number): number {
    const start = this.offset;
    const end = start + byteLength;
    if (byteLength < 0 || end > this.limit) {
      throw new RangeError("Wire read past the end of the payload");
    }
    this.offset = end;
    return start;
  }

  readBool(): boolean {
    const value = this.view.getUint8(this.take(1));
    return value !== 0;
  }

  skip(n: number): void {
    this.take(n);
  }

  readI8(): number {
    const value = this.view.getInt8(this.take(1));
    return value;
  }

  readU8(): number {
    const value = this.view.getUint8(this.take(1));
    return value;
  }

  readI16(): number {
    const value = this.view.getInt16(this.take(2), true);
    return value;
  }

  readU16(): number {
    const value = this.view.getUint16(this.take(2), true);
    return value;
  }

  readI32(): number {
    const value = this.view.getInt32(this.take(4), true);
    return value;
  }

  readU32(): number {
    const value = this.view.getUint32(this.take(4), true);
    return value;
  }

  readI64(): bigint {
    const value = this.view.getBigInt64(this.take(8), true);
    return value;
  }

  readU64(): bigint {
    const value = this.view.getBigUint64(this.take(8), true);
    return value;
  }

  readISize(): number {
    return this.readI32();
  }

  readUSize(): number {
    return this.readU32();
  }

  readF32(): number {
    const value = this.view.getFloat32(this.take(4), true);
    return value;
  }

  readF64(): number {
    const value = this.view.getFloat64(this.take(8), true);
    return value;
  }

  readString(): string {
    const len = this.readU32();
    const start = this.take(len);
    // Short ASCII strings are cheaper to build from their char codes than to
    // decode, on both time and allocation. Every code has to be passed at
    // once: appending one at a time builds a cons-string chain that allocates
    // more than TextDecoder does.
    if (len <= ASCII_FAST_PATH_LIMIT) {
      const bytes = this.asBytes();
      const scratch = ASCII_SCRATCH;
      let i = 0;
      for (; i < len; i++) {
        const byte = bytes[start + i]!;
        if (byte > 0x7f) break;
        scratch[i] = byte;
      }
      if (i === len) {
        scratch.length = len;
        return String.fromCharCode.apply(null, scratch);
      }
    }
    return UTF8_DECODER.decode(new Uint8Array(this.view.buffer, start, len));
  }

  /** Whole-buffer view of the reader's memory, for the fast path above. */
  private asBytes(): Uint8Array {
    let bytes = this.bytes;
    if (bytes === null || bytes.byteLength === 0) {
      bytes = new Uint8Array(this.bufferRef);
      this.bytes = bytes;
    }
    return bytes;
  }

  readBytes(): Uint8Array {
    const len = this.readU32();
    const bytes = new Uint8Array(this.view.buffer, this.take(len), len);
    return bytes.slice();
  }

  readI8Array(): Int8Array {
    const len = this.readU32();
    const result = new Int8Array(this.view.buffer, this.take(len), len);
    return this.borrowed ? result.slice() : result;
  }

  readU8Array(): Uint8Array {
    const len = this.readU32();
    const result = new Uint8Array(this.view.buffer, this.take(len), len);
    return this.borrowed ? result.slice() : result;
  }

  readBoolArray(): boolean[] {
    const len = this.readU32();
    const values = new Uint8Array(this.view.buffer, this.take(len), len);
    // `Array.from` with a mapper walks a TypedArray through the iterator
    // protocol, which costs an order of magnitude more than indexing it.
    const result = new Array<boolean>(len);
    for (let index = 0; index < len; index++) {
      result[index] = values[index] !== 0;
    }
    return result;
  }

  private readTypedArray<T extends ArrayBufferView>(
    typedArray: TypedArrayConstructor<T>,
    len: number
  ): T {
    const byteLength = len * typedArray.BYTES_PER_ELEMENT;
    const byteOffset = this.take(byteLength);
    if (!this.borrowed && byteOffset % typedArray.BYTES_PER_ELEMENT === 0) {
      return new typedArray(this.view.buffer, byteOffset, len);
    }
    const copy = new Uint8Array(this.view.buffer, byteOffset, byteLength).slice().buffer;
    return new typedArray(copy);
  }

  readI16Array(): Int16Array {
    const len = this.readU32();
    return this.readTypedArray(Int16Array, len);
  }

  readU16Array(): Uint16Array {
    const len = this.readU32();
    return this.readTypedArray(Uint16Array, len);
  }

  readI32Array(): Int32Array {
    const len = this.readU32();
    return this.readTypedArray(Int32Array, len);
  }

  readU32Array(): Uint32Array {
    const len = this.readU32();
    return this.readTypedArray(Uint32Array, len);
  }

  readISizeArray(): Int32Array {
    return this.readI32Array();
  }

  readUSizeArray(): Uint32Array {
    return this.readU32Array();
  }

  readI64Array(): BigInt64Array {
    const len = this.readU32();
    return this.readTypedArray(BigInt64Array, len);
  }

  readU64Array(): BigUint64Array {
    const len = this.readU32();
    return this.readTypedArray(BigUint64Array, len);
  }

  readF32Array(): Float32Array {
    const len = this.readU32();
    return this.readTypedArray(Float32Array, len);
  }

  readF64Array(): Float64Array {
    const len = this.readU32();
    return this.readTypedArray(Float64Array, len);
  }

  readOptional<T>(readValue: () => T): T | null {
    const tag = this.readU8();
    if (tag === 0) {
      return null;
    }
    return readValue();
  }

  readArray<T>(readElement: () => T): T[] {
    const len = this.readU32();
    // The length is known, so the array is sized once rather than grown.
    const result = new Array<T>(len);
    for (let index = 0; index < len; index++) {
      result[index] = readElement();
    }
    return result;
  }

  readMap<K, V>(readKey: () => K, readValue: () => V): Map<K, V> {
    const len = this.readU32();
    const result = new Map<K, V>();
    let index = 0;
    while (index < len) {
      result.set(readKey(), readValue());
      index += 1;
    }
    return result;
  }

  readResult<T, E>(readOk: () => T, readErr: () => E): T {
    const tag = this.readU8();
    if (tag === 0) {
      return readOk();
    }
    throw readErr();
  }

  readDuration(): Duration {
    const secs = this.readU64();
    const nanos = this.readU32();
    return { secs, nanos };
  }

  readTimestamp(): Date {
    const secs = this.readI64();
    const nanos = this.readU32();
    const ms = Number(secs) * 1000 + Math.floor(nanos / 1_000_000);
    return new Date(ms);
  }

  readUuid(): string {
    const hi = this.readU64();
    const lo = this.readU64();
    const hiHex = hi.toString(16).padStart(16, "0");
    const loHex = lo.toString(16).padStart(16, "0");
    const hex = hiHex + loHex;
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }

  readUrl(): string {
    return this.readString();
  }
}

export interface Duration {
  secs: bigint;
  nanos: number;
}

export type WireOk<T> = { tag: "ok"; value: T };
export type WireErr<E> = { tag: "err"; error: E };
export type WireResult<T, E> = WireOk<T> | WireErr<E>;

export interface WasmWireWriterAllocator {
  alloc(size: number): number;
  realloc(ptr: number, oldSize: number, newSize: number): number;
  free(ptr: number, size: number): void;
  buffer(): ArrayBuffer;
}

export function wireOk<T>(value: T): WireOk<T> {
  return { tag: "ok", value };
}

export function wireErr<E>(error: E): WireErr<E> {
  return { tag: "err", error };
}

export function matchWireResult<T, E, R>(
  value: T | WireResult<T, E> | Error,
  ok: (value: T) => R,
  err: (error: E) => R
): R {
  if (
    typeof value === "object" &&
    value !== null &&
    "tag" in value &&
    value.tag === "ok" &&
    "value" in value
  ) {
    return ok(value.value as T);
  }
  if (
    typeof value === "object" &&
    value !== null &&
    "tag" in value &&
    value.tag === "err" &&
    "error" in value
  ) {
    return err(value.error as E);
  }
  if (value instanceof Error) {
    return err(value as E);
  }
  // A typed array is never a `WireResult`: it carries no `tag`, and a caller
  // cannot have meant it as one. Without this a callback returning
  // `Result<Vec<u8>, E>` hits the ambiguity below, the throw is reported as
  // completion code -2, and the Rust side decodes that message as the error
  // enum — a decode failure that aborts the process rather than surfacing.
  //
  // Only views with a numeric `length`, which is what the bytes codec sizes
  // and writes through. A bare `ArrayBuffer` or a `DataView` has none, so
  // exempting them would size the payload as `NaN` and throw inside the
  // encoder instead — the same abort, one step later.
  if (ArrayBuffer.isView(value) && typeof (value as { length?: unknown }).length === "number") {
    return ok(value as T);
  }
  if (typeof value === "object" && value !== null) {
    throw new Error(
      "Ambiguous Result object. Pass wireOk(value) or wireErr(error) for object payloads."
    );
  }
  return ok(value as T);
}

export class WireWriter {
  /**
   * Only used when the writer is not bound to a wasm region. A region-bound
   * writer never reads either, so both are left unallocated for it — they are
   * two allocations per call on the callback trampoline, which builds one
   * writer per element.
   */
  private localBuffer: ArrayBuffer | null;
  private localView: DataView | null;
  private wasmAllocator: WasmWireWriterAllocator | null;
  private wasmPtr: number;
  private allocationSize: number;
  private offset: number;
  private cachedWasmView: DataView | null;
  private cachedWasmBuffer: ArrayBuffer | null;
  /**
   * Detachment probe for the two caches above. `allocator.buffer()` reads the
   * `WebAssembly.Memory.prototype.buffer` *accessor*, and every scalar write
   * went through it — 21 accessor reads per record on `countActiveUsers`.
   * Growing wasm memory detaches the old buffer, so a cached `Uint8Array` over
   * it reports `byteLength === 0`, which is a plain field read. It has to be a
   * `Uint8Array`: `DataView.prototype.byteLength` throws when detached rather
   * than reporting 0, so the view cannot probe itself.
   */
  private wasmProbe: Uint8Array | null;

  constructor(initialSize = 256, allocateLocal = true) {
    const normalizedSize = Math.max(initialSize, 1);
    this.localBuffer = allocateLocal ? new ArrayBuffer(normalizedSize) : null;
    this.localView =
      this.localBuffer === null ? null : new DataView(this.localBuffer);
    this.wasmAllocator = null;
    this.wasmPtr = 0;
    this.allocationSize = normalizedSize;
    this.offset = 0;
    this.cachedWasmView = null;
    this.cachedWasmBuffer = null;
    this.wasmProbe = null;
  }

  static withWasmAllocation(
    initialSize: number,
    allocator: WasmWireWriterAllocator
  ): WireWriter {
    const normalizedSize = Math.max(initialSize, 1);
    const pointer = allocator.alloc(normalizedSize);
    if (pointer === 0 && normalizedSize > 0) {
      throw new Error("Failed to allocate memory for writer");
    }
    const writer = new WireWriter(1);
    writer.wasmAllocator = allocator;
    writer.wasmPtr = pointer;
    writer.allocationSize = normalizedSize;
    return writer;
  }

  /** Allocator for a caller-owned region: it can only report the buffer. */
  static fixedRegionAllocator(buffer: () => ArrayBuffer): WasmWireWriterAllocator {
    return {
      alloc: () => {
        throw new Error("Fixed WASM region cannot allocate");
      },
      realloc: () => {
        throw new Error("Fixed WASM region exceeded its capacity");
      },
      free: () => {},
      buffer,
    };
  }

  static withWasmRegion(
    pointer: number,
    size: number,
    buffer: () => ArrayBuffer,
    allocator?: WasmWireWriterAllocator
  ): WireWriter {
    const writer = new WireWriter(1, false);
    // `allocator` lets the caller hand in one shared object instead of building
    // a fresh one with four closures per call.
    writer.wasmAllocator = allocator ?? WireWriter.fixedRegionAllocator(buffer);
    writer.wasmPtr = pointer;
    writer.allocationSize = size;
    return writer;
  }

  release(): void {
    if (this.wasmAllocator !== null && this.wasmPtr !== 0 && this.allocationSize !== 0) {
      this.wasmAllocator.free(this.wasmPtr, this.allocationSize);
      this.wasmPtr = 0;
      this.allocationSize = 0;
      this.offset = 0;
    }
  }

  reset(): void {
    if (this.allocationSize === 0) {
      throw new Error("Cannot reset a released WireWriter");
    }
    this.offset = 0;
  }

  get capacity(): number {
    return this.allocationSize;
  }

  private ensureLocalBuffer(): ArrayBuffer {
    let buffer = this.localBuffer;
    if (buffer === null) {
      buffer = new ArrayBuffer(Math.max(this.allocationSize, 1));
      this.localBuffer = buffer;
      this.localView = new DataView(buffer);
    }
    return buffer;
  }

  private inWasmMemory(): boolean {
    return this.wasmAllocator !== null;
  }

  /** Only reached when the probe says the cached buffer is gone. */
  private refreshWasmCaches(): ArrayBuffer {
    const buffer = this.wasmAllocator!.buffer();
    this.cachedWasmBuffer = buffer;
    this.cachedWasmView = new DataView(buffer);
    // Growing a *shared* memory replaces `memory.buffer` without detaching the
    // old `SharedArrayBuffer`, so a probe over it would stay nonzero and the
    // stale view would be reused — and if `realloc` moved the allocation past
    // the old length, the next write throws. Leaving the probe unset makes the
    // fast check fail every time, so shared memory keeps the buffer-identity
    // refresh above, which handles it. Costs the unshared path nothing.
    this.wasmProbe =
      typeof SharedArrayBuffer !== "undefined" &&
      buffer instanceof SharedArrayBuffer
        ? null
        : new Uint8Array(buffer);
    return buffer;
  }

  private currentBuffer(): ArrayBuffer {
    if (this.wasmAllocator === null) {
      return this.ensureLocalBuffer();
    }
    const probe = this.wasmProbe;
    if (probe !== null && probe.byteLength !== 0) {
      return this.cachedWasmBuffer as ArrayBuffer;
    }
    return this.refreshWasmCaches();
  }

  private currentView(): DataView {
    if (this.wasmAllocator === null) {
      this.ensureLocalBuffer();
      return this.localView as DataView;
    }
    const probe = this.wasmProbe;
    if (probe !== null && probe.byteLength !== 0) {
      return this.cachedWasmView as DataView;
    }
    this.refreshWasmCaches();
    return this.cachedWasmView as DataView;
  }

  private writePosition(): number {
    return this.inWasmMemory() ? this.wasmPtr + this.offset : this.offset;
  }

  private ensureCapacity(additionalBytes: number): void {
    if (this.allocationSize === 0) {
      throw new Error("Cannot write using a released WireWriter");
    }
    const required = this.offset + additionalBytes;
    if (required <= this.allocationSize) {
      return;
    }
    let newSize = this.allocationSize;
    while (newSize < required) {
      newSize *= 2;
    }
    if (this.inWasmMemory()) {
      const newPointer = this.wasmAllocator!.realloc(this.wasmPtr, this.allocationSize, newSize);
      if (newPointer === 0 && newSize > 0) {
        throw new Error("Failed to reallocate memory for writer");
      }
      this.wasmPtr = newPointer;
      this.allocationSize = newSize;
      return;
    }
    const newBuffer = new ArrayBuffer(newSize);
    new Uint8Array(newBuffer).set(new Uint8Array(this.ensureLocalBuffer()));
    this.localBuffer = newBuffer;
    this.localView = new DataView(newBuffer);
    this.allocationSize = newSize;
  }

  get ptr(): number {
    return this.wasmPtr;
  }

  get len(): number {
    return this.offset;
  }

  getBytes(): Uint8Array {
    const start = this.inWasmMemory() ? this.wasmPtr : 0;
    return new Uint8Array(this.currentBuffer(), start, this.offset).slice();
  }

  writeBool(value: boolean): void {
    this.ensureCapacity(1);
    this.currentView().setUint8(this.writePosition(), value ? 1 : 0);
    this.offset += 1;
  }

  skip(n: number): void {
    this.ensureCapacity(n);
    const view = this.currentView();
    const pos = this.writePosition();
    for (let i = 0; i < n; i++) {
      view.setUint8(pos + i, 0);
    }
    this.offset += n;
  }

  writeI8(value: number): void {
    this.ensureCapacity(1);
    this.currentView().setInt8(this.writePosition(), value);
    this.offset += 1;
  }

  writeU8(value: number): void {
    this.ensureCapacity(1);
    this.currentView().setUint8(this.writePosition(), value);
    this.offset += 1;
  }

  writeI16(value: number): void {
    this.ensureCapacity(2);
    this.currentView().setInt16(this.writePosition(), value, true);
    this.offset += 2;
  }

  writeU16(value: number): void {
    this.ensureCapacity(2);
    this.currentView().setUint16(this.writePosition(), value, true);
    this.offset += 2;
  }

  writeI32(value: number): void {
    this.ensureCapacity(4);
    this.currentView().setInt32(this.writePosition(), value, true);
    this.offset += 4;
  }

  writeU32(value: number): void {
    this.ensureCapacity(4);
    this.currentView().setUint32(this.writePosition(), value, true);
    this.offset += 4;
  }

  writeI64(value: bigint): void {
    this.ensureCapacity(8);
    this.currentView().setBigInt64(this.writePosition(), value, true);
    this.offset += 8;
  }

  writeU64(value: bigint): void {
    this.ensureCapacity(8);
    this.currentView().setBigUint64(this.writePosition(), value, true);
    this.offset += 8;
  }

  writeISize(value: number): void {
    this.writeI32(value);
  }

  writeUSize(value: number): void {
    this.writeU32(value);
  }

  writeF32(value: number): void {
    this.ensureCapacity(4);
    this.currentView().setFloat32(this.writePosition(), value, true);
    this.offset += 4;
  }

  writeF64(value: number): void {
    this.ensureCapacity(8);
    this.currentView().setFloat64(this.writePosition(), value, true);
    this.offset += 8;
  }

  writeString(value: string): void {
    const byteLength = utf8ByteCount(value);
    // Reserve length prefix and payload together so a realloc cannot happen
    // between taking the view and writing into it.
    this.ensureCapacity(4 + byteLength);
    this.writeU32(byteLength);
    if (byteLength > 0) {
      const target = new Uint8Array(this.currentBuffer(), this.writePosition(), byteLength);
      const { written } = UTF8_ENCODER.encodeInto(value, target);
      if (written !== byteLength) {
        // encodeInto may stop short on an exactly sized buffer for some
        // non-ASCII inputs; fall back to the allocating path.
        target.set(UTF8_ENCODER.encode(value));
      }
    }
    this.offset += byteLength;
  }

  writeBytes(value: Uint8Array): void {
    this.writeU32(value.length);
    this.ensureCapacity(value.length);
    new Uint8Array(this.currentBuffer()).set(value, this.writePosition());
    this.offset += value.length;
  }

  writeOptional<T>(value: T | null, writeValue: (v: T) => void): void {
    if (value === null) {
      this.writeU8(0);
    } else {
      this.writeU8(1);
      writeValue(value);
    }
  }

  writeArray<T>(values: ArrayLike<T> & Iterable<T>, writeElement: (v: T) => void): void {
    this.writeU32(values.length);
    for (const v of values) {
      writeElement(v);
    }
  }

  writeMap<K, V>(
    values: ReadonlyMap<K, V>,
    writeKey: (key: K) => void,
    writeValue: (value: V) => void
  ): void {
    this.writeU32(values.size);
    values.forEach((value, key) => {
      writeKey(key);
      writeValue(value);
    });
  }

  writeResult<T, E = never>(
    value: T | WireResult<T, E> | Error,
    writeOk: (v: T) => void,
    writeErr: (e: E) => void
  ): void {
    matchWireResult(
      value,
      (ok) => {
        this.writeU8(0);
        writeOk(ok);
      },
      (err) => {
        this.writeU8(1);
        writeErr(err);
      }
    );
  }

  writeDuration(value: Duration): void {
    this.writeU64(value.secs);
    this.writeU32(value.nanos);
  }

  writeTimestamp(value: Date): void {
    const ms = value.getTime();
    const wholeSeconds = Math.floor(ms / 1000);
    const secs = BigInt(wholeSeconds);
    const nanos = (ms - wholeSeconds * 1000) * 1_000_000;
    this.writeI64(secs);
    this.writeU32(nanos);
  }

  writeUuid(value: string): void {
    const hex = value.replace(/-/g, "");
    const hi = BigInt("0x" + hex.slice(0, 16));
    const lo = BigInt("0x" + hex.slice(16, 32));
    this.writeU64(hi);
    this.writeU64(lo);
  }

  writeUrl(value: string): void {
    this.writeString(value);
  }
}

export function wireStringSize(value: string): number {
  return 4 + utf8ByteCount(value);
}

/**
 * Counts UTF-8 bytes without allocating. Encoding the string just to read
 * `.length` made every string encode twice: once to size it, once to write it.
 * Unpaired surrogates count as 3 bytes, matching TextEncoder's U+FFFD output.
 */
export function utf8ByteCount(value: string): number {
  let bytes = 0;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code < 0x80) {
      bytes += 1;
    } else if (code < 0x800) {
      bytes += 2;
    } else if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else {
        bytes += 3;
      }
    } else {
      bytes += 3;
    }
  }
  return bytes;
}

export function wireOptionalSize<T>(value: T | null, size: (value: T) => number): number {
  return value === null ? 1 : 1 + size(value);
}

export function wireArraySize<T>(values: readonly T[], size: (value: T) => number): number {
  return values.reduce((bytes, value) => bytes + size(value), 4);
}

export function wireMapSize<K, V>(
  values: ReadonlyMap<K, V>,
  keySize: (key: K) => number,
  valueSize: (value: V) => number
): number {
  let bytes = 4;
  values.forEach((value, key) => {
    bytes += keySize(key) + valueSize(value);
  });
  return bytes;
}

export function wireResultSize<T, E = never>(
  value: T | WireResult<T, E> | Error,
  ok: (value: T) => number,
  err: (error: E) => number
): number {
  return 1 + matchWireResult(value, ok, err);
}

export interface WireCodec<T> {
  size(value: T): number;
  encode(writer: WireWriter, value: T): void;
  decode(reader: WireReader): T;
}

/**
 * Marks a callback error payload as a host-language failure rather than the
 * error type the callback declared.
 *
 * Must stay in step with `UnexpectedFfiCallbackError::WIRE_MARKER` in
 * `boltffi_core`, which is what reads this back.
 */
const UNEXPECTED_CALLBACK_ERROR_MARKER = "BOLTFFI_CALLBACK";

/** Envelope format understood by the Rust side. */
const UNEXPECTED_CALLBACK_ERROR_VERSION = 1;

/** Allocates the wasm-backed writer an unexpected callback error is written into. */
export interface UnexpectedCallbackErrorAllocator {
  allocWriter(size: number): WireWriter;
}

/**
 * Encodes a host error a callback threw, so Rust can tell it apart from the
 * error type that callback declared.
 *
 * An async completion reports failure through a status code, and the code for
 * "the callback threw" is indistinguishable from the one for a typed error
 * once it reaches Rust. Without this envelope the thrown message is decoded as
 * the declared error; that decode fails, and the failure is a panic, which
 * under `panic = "abort"` takes the whole module down. The envelope routes it
 * through `From<UnexpectedFfiCallbackError>` instead.
 *
 * The layout is the marker, a version byte, then the message as an ordinary
 * wire string. `boltffiEncodeUnexpectedCallbackError` in the Swift runtime
 * writes the same bytes.
 */
export function writeUnexpectedCallbackError(
  allocator: UnexpectedCallbackErrorAllocator,
  error: unknown
): WireWriter {
  const message = error instanceof Error ? error.message : String(error);
  const writer = allocator.allocWriter(
    UNEXPECTED_CALLBACK_ERROR_MARKER.length + 1 + 4 + utf8ByteCount(message)
  );
  for (let index = 0; index < UNEXPECTED_CALLBACK_ERROR_MARKER.length; index++) {
    writer.writeU8(UNEXPECTED_CALLBACK_ERROR_MARKER.charCodeAt(index));
  }
  writer.writeU8(UNEXPECTED_CALLBACK_ERROR_VERSION);
  writer.writeString(message);
  return writer;
}
