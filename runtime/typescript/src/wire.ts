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
    return Array.from(values, (value) => value !== 0);
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
    const result: T[] = [];
    for (let i = 0; i < len; i++) {
      result.push(readElement());
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
  if (typeof value === "object" && value !== null) {
    throw new Error(
      "Ambiguous Result object. Pass wireOk(value) or wireErr(error) for object payloads."
    );
  }
  return ok(value as T);
}

export class WireWriter {
  private localBuffer: ArrayBuffer;
  private localView: DataView;
  private wasmAllocator: WasmWireWriterAllocator | null;
  private wasmPtr: number;
  private allocationSize: number;
  private offset: number;
  private cachedWasmView: DataView | null;
  private cachedWasmBuffer: ArrayBuffer | null;

  constructor(initialSize = 256) {
    const normalizedSize = Math.max(initialSize, 1);
    this.localBuffer = new ArrayBuffer(normalizedSize);
    this.localView = new DataView(this.localBuffer);
    this.wasmAllocator = null;
    this.wasmPtr = 0;
    this.allocationSize = normalizedSize;
    this.offset = 0;
    this.cachedWasmView = null;
    this.cachedWasmBuffer = null;
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

  static withWasmRegion(pointer: number, size: number, buffer: () => ArrayBuffer): WireWriter {
    const writer = new WireWriter(1);
    writer.wasmAllocator = {
      alloc: () => pointer,
      realloc: () => {
        throw new Error("Fixed WASM region exceeded its capacity");
      },
      free: () => {},
      buffer,
    };
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

  private inWasmMemory(): boolean {
    return this.wasmAllocator !== null;
  }

  private currentBuffer(): ArrayBuffer {
    return this.inWasmMemory() ? this.wasmAllocator!.buffer() : this.localBuffer;
  }

  private currentView(): DataView {
    if (!this.inWasmMemory()) {
      return this.localView;
    }
    const buffer = this.wasmAllocator!.buffer();
    if (this.cachedWasmBuffer !== buffer) {
      this.cachedWasmBuffer = buffer;
      this.cachedWasmView = new DataView(buffer);
    }
    return this.cachedWasmView!;
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
    new Uint8Array(newBuffer).set(new Uint8Array(this.localBuffer));
    this.localBuffer = newBuffer;
    this.localView = new DataView(this.localBuffer);
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
