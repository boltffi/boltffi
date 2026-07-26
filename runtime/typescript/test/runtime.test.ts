import { describe, expect, it, vi } from "vitest";
import { AsyncFutureManager, BoltFFIModule, instantiateBoltFFI } from "../src/module.js";
import { CallbackRegistry } from "../src/callback.js";
import { StreamPollManager, StreamPollResult, StreamSession } from "../src/stream.js";
import {
  WireReader,
  WireWriter,
  utf8ByteCount,
  wireArraySize,
  wireMapSize,
  wireErr,
  wireOk,
  wireOptionalSize,
  wireResultSize,
} from "../src/wire.js";

type ExportFunction = (...args: number[]) => number | void;

interface RuntimeHarness {
  module: BoltFFIModule;
  freedAllocations: Array<[number, number]>;
}

function createHarness(): RuntimeHarness {
  const wasmMemory = new WebAssembly.Memory({ initial: 1 });
  const freedAllocations: Array<[number, number]> = [];
  const allocations = new Map<number, number>();
  const returnSlotAddress = 16;
  let nextPointer = 256;

  const exports: Record<string, ExportFunction | WebAssembly.Memory> = {
    memory: wasmMemory,
    boltffi_wasm_abi_version: () => 1,
    boltffi_wasm_alloc: (size: number) => {
      if (size === 0) {
        return 0;
      }
      const pointer = nextPointer;
      nextPointer += size;
      allocations.set(pointer, size);
      return pointer;
    },
    boltffi_wasm_free: (ptr: number, size: number) => {
      if (ptr === 0 || size === 0) {
        return;
      }
      freedAllocations.push([ptr, size]);
      allocations.delete(ptr);
    },
    boltffi_wasm_free_buf: (ptr: number, size: number) => {
      if (ptr === 0 || size === 0) {
        return;
      }
      freedAllocations.push([ptr, size]);
      allocations.delete(ptr);
    },
    boltffi_wasm_realloc: (ptr: number, oldSize: number, newSize: number) => {
      if (newSize === 0) {
        if (ptr !== 0 && oldSize !== 0) {
          freedAllocations.push([ptr, oldSize]);
          allocations.delete(ptr);
        }
        return 0;
      }
      if (ptr === 0) {
        const pointer = nextPointer;
        nextPointer += newSize;
        allocations.set(pointer, newSize);
        return pointer;
      }
      const pointer = nextPointer;
      nextPointer += newSize;
      const memoryBytes = new Uint8Array(wasmMemory.buffer);
      const copyLength = Math.min(oldSize, newSize);
      memoryBytes.set(memoryBytes.subarray(ptr, ptr + copyLength), pointer);
      allocations.delete(ptr);
      allocations.set(pointer, newSize);
      return pointer;
    },
    boltffi_wasm_alloc_owned_bytes: (size: number) => {
      if (size === 0) {
        return 0;
      }
      const pointer = nextPointer;
      nextPointer += size;
      allocations.set(pointer, size);
      return pointer;
    },
    boltffi_wasm_free_string_return: (ptr: number, len: number) => {
      if (ptr === 0 || len === 0) {
        return;
      }
      freedAllocations.push([ptr, len]);
      allocations.delete(ptr);
    },
    boltffi_wasm_return_slot_addr: () => returnSlotAddress,
  };

  const instance = { exports } as unknown as WebAssembly.Instance;
  const asyncManager = new AsyncFutureManager();
  const streamManager = new StreamPollManager();
  return { module: new BoltFFIModule(instance, asyncManager, streamManager), freedAllocations };
}

describe("WireReader and WireWriter", () => {
  it("round-trips primitive, string, and bytes values", () => {
    const writer = new WireWriter();
    writer.writeBool(true);
    writer.writeI8(-5);
    writer.writeU8(250);
    writer.writeI16(-1234);
    writer.writeU16(54321);
    writer.writeI32(-123_456_789);
    writer.writeU32(3_456_789_012);
    writer.writeI64(-9_000_000_000n);
    writer.writeU64(18_000_000_000n);
    writer.writeISize(-17);
    writer.writeUSize(23);
    writer.writeF32(Math.PI);
    writer.writeF64(Math.E);
    writer.writeString("boltffi");
    writer.writeBytes(Uint8Array.from([1, 2, 3, 4, 5]));

    const reader = new WireReader(writer.getBytes().buffer);
    expect(reader.readBool()).toBe(true);
    expect(reader.readI8()).toBe(-5);
    expect(reader.readU8()).toBe(250);
    expect(reader.readI16()).toBe(-1234);
    expect(reader.readU16()).toBe(54321);
    expect(reader.readI32()).toBe(-123_456_789);
    expect(reader.readU32()).toBe(3_456_789_012);
    expect(reader.readI64()).toBe(-9_000_000_000n);
    expect(reader.readU64()).toBe(18_000_000_000n);
    expect(reader.readISize()).toBe(-17);
    expect(reader.readUSize()).toBe(23);
    expect(reader.readF32()).toBeCloseTo(Math.PI, 5);
    expect(reader.readF64()).toBeCloseTo(Math.E, 12);
    expect(reader.readString()).toBe("boltffi");
    expect(Array.from(reader.readBytes())).toEqual([1, 2, 3, 4, 5]);
  });

  it("encodes and decodes optional and array values", () => {
    const writer = new WireWriter();
    writer.writeOptional<number>(null, (value) => writer.writeI32(value));
    writer.writeOptional<number>(33, (value) => writer.writeI32(value));
    writer.writeArray<number>([4, 5, 6], (value) => writer.writeU32(value));
    writer.writeU32(2);
    writer.writeString("first");
    writer.writeI32(10);
    writer.writeString("second");
    writer.writeI32(20);

    const reader = new WireReader(writer.getBytes().buffer);
    expect(reader.readOptional(() => reader.readI32())).toBeNull();
    expect(reader.readOptional(() => reader.readI32())).toBe(33);
    expect(reader.readArray(() => reader.readU32())).toEqual([4, 5, 6]);
    expect(reader.readMap(() => reader.readString(), () => reader.readI32())).toEqual(
      new Map([
        ["first", 10],
        ["second", 20],
      ])
    );
    expect(wireOptionalSize(null, () => 8)).toBe(1);
    expect(wireOptionalSize(33n, () => 8)).toBe(9);
  });

  it("encodes and decodes maps without intermediate entry arrays", () => {
    const values = new Map([
      ["first", [1, 2]],
      ["second", [3]],
    ]);
    const writer = new WireWriter();
    writer.writeMap(
      values,
      (key) => writer.writeString(key),
      (value) => writer.writeArray(value, (item) => writer.writeI32(item))
    );

    expect(writer.len).toBe(
      wireMapSize(values, (key) => 4 + utf8ByteCount(key), (value) => 4 + value.length * 4)
    );

    const reader = new WireReader(writer.getBytes().buffer);
    expect(
      reader.readMap(
        () => reader.readString(),
        () => reader.readArray(() => reader.readI32())
      )
    ).toEqual(values);
  });

  it("decodes primitive sequences through bulk array readers", () => {
    const writer = new WireWriter();
    writer.writeArray([true, false, true], (value) => writer.writeBool(value));
    writer.writeArray([-7, 12], (value) => writer.writeISize(value));
    writer.writeArray([9, 14], (value) => writer.writeUSize(value));

    const reader = new WireReader(writer.getBytes().buffer);
    expect(reader.readBoolArray()).toEqual([true, false, true]);
    expect(Array.from(reader.readISizeArray())).toEqual([-7, 12]);
    expect(Array.from(reader.readUSizeArray())).toEqual([9, 14]);
    expect(wireArraySize(["a", "boltffi"], utf8ByteCount)).toBe(12);
  });

  it("decodes readResult success and error branches", () => {
    const okWriter = new WireWriter();
    okWriter.writeU8(0);
    okWriter.writeI32(44);
    const okReader = new WireReader(okWriter.getBytes().buffer);
    expect(okReader.readResult(() => okReader.readI32(), () => new Error("err"))).toBe(44);

    const errWriter = new WireWriter();
    errWriter.writeU8(1);
    errWriter.writeString("boom");
    const errReader = new WireReader(errWriter.getBytes().buffer);
    expect(() =>
      errReader.readResult(
        () => 0,
        () => new Error(errReader.readString())
      )
    ).toThrow("boom");
  });

  it("returns a detached copy for readBytes", () => {
    const writer = new WireWriter();
    writer.writeBytes(Uint8Array.from([7, 8, 9]));
    const payloadBuffer = writer.getBytes().buffer;

    const firstReader = new WireReader(payloadBuffer);
    const firstRead = firstReader.readBytes();
    firstRead[0] = 99;

    const secondReader = new WireReader(payloadBuffer);
    expect(Array.from(secondReader.readBytes())).toEqual([7, 8, 9]);
  });

  it("grows local writer capacity for large payloads", () => {
    const writer = new WireWriter(1);
    const largePayload = Uint8Array.from(Array.from({ length: 300 }, (_, index) => index % 256));
    writer.writeBytes(largePayload);

    expect(writer.len).toBe(304);
    const reader = new WireReader(writer.getBytes().buffer);
    expect(Array.from(reader.readBytes())).toEqual(Array.from(largePayload));
  });
});

describe("WireWriter result encoding", () => {
  it("encodes explicit ok and err tags deterministically", () => {
    const okWriter = new WireWriter();
    okWriter.writeResult(
      wireOk(42),
      (value) => {
        okWriter.writeI32(value);
      },
      () => {
        throw new Error("unexpected err branch");
      }
    );
    const okReader = new WireReader(okWriter.getBytes().buffer);
    expect(okReader.readU8()).toBe(0);
    expect(okReader.readI32()).toBe(42);

    const errWriter = new WireWriter();
    errWriter.writeResult(
      wireErr({ code: 7 }),
      () => {
        throw new Error("unexpected ok branch");
      },
      (error) => {
        errWriter.writeU32(error.code);
      }
    );
    const errReader = new WireReader(errWriter.getBytes().buffer);
    expect(errReader.readU8()).toBe(1);
    expect(errReader.readU32()).toBe(7);
    expect(wireResultSize(wireOk(42), () => 4, () => 8)).toBe(5);
    expect(wireResultSize(wireErr("failure"), () => 4, (error) => error.length)).toBe(8);
  });

  it("uses Error objects as err branch fallback", () => {
    const writer = new WireWriter();
    writer.writeResult<number, Error>(
      new Error("fallback"),
      () => {
        throw new Error("unexpected ok branch");
      },
      (error) => {
        writer.writeString(error.message);
      }
    );

    const reader = new WireReader(writer.getBytes().buffer);
    expect(reader.readU8()).toBe(1);
    expect(reader.readString()).toBe("fallback");
  });

  it("rejects ambiguous object payloads without explicit result tagging", () => {
    const writer = new WireWriter();
    expect(() =>
      writer.writeResult(
        { code: 7 } as unknown as number,
        () => {
          throw new Error("unexpected ok branch");
        },
        () => {
          throw new Error("unexpected err branch");
        }
      )
    ).toThrow("Ambiguous Result object");
  });
});

describe("BoltFFIModule memory operations", () => {
  it("allocString writes UTF-8 bytes and freeAlloc releases them", () => {
    const { module, freedAllocations } = createHarness();
    const allocation = module.allocString("hello");
    expect(allocation.ptr).toBeGreaterThan(0);
    expect(allocation.len).toBe(5);
    expect(Array.from(module.readFromMemory(allocation.ptr, allocation.len))).toEqual([
      104, 101, 108, 108, 111,
    ]);

    module.freeAlloc(allocation);
    expect(freedAllocations).toContainEqual([allocation.ptr, allocation.len]);
  });

  it("handles empty string allocations without invalid free", () => {
    const { module, freedAllocations } = createHarness();
    const allocation = module.allocString("");
    expect(allocation.ptr).toBe(0);
    expect(allocation.len).toBe(0);
    module.freeAlloc(allocation);
    expect(freedAllocations).toEqual([]);
  });

  it("reads and frees result buffers through descriptor pointer", () => {
    const { module, freedAllocations } = createHarness();
    const payloadPointer = 1024;
    const payloadCapacity = 32;
    const encodedPayloadWriter = new WireWriter();
    encodedPayloadWriter.writeBytes(Uint8Array.from([10, 11, 12, 13]));
    const encodedPayload = encodedPayloadWriter.getBytes();
    module.writeToMemory(payloadPointer, encodedPayload);

    const descriptorPointer = 2048;
    const descriptorView = new DataView(new ArrayBuffer(16));
    descriptorView.setUint32(0, payloadPointer, true);
    descriptorView.setUint32(4, encodedPayload.length, true);
    descriptorView.setUint32(8, payloadCapacity, true);
    descriptorView.setUint32(12, 1, true);
    module.writeToMemory(descriptorPointer, new Uint8Array(descriptorView.buffer));

    const reader = module.readerFromBuf(descriptorPointer);
    expect(Array.from(reader.readBytes())).toEqual([10, 11, 12, 13]);

    module.freeBuf(descriptorPointer);
    expect(freedAllocations).toContainEqual([payloadPointer, payloadCapacity]);
    expect(freedAllocations).toContainEqual([descriptorPointer, 16]);
  });

  it("freeBufDescriptor releases descriptor allocation only", () => {
    const { module, freedAllocations } = createHarness();
    const descriptorPointer = 4096;
    module.freeBufDescriptor(descriptorPointer);
    expect(freedAllocations).toContainEqual([descriptorPointer, 16]);
  });

  it("allocWriter reallocates when payload outgrows initial capacity", () => {
    const { module, freedAllocations } = createHarness();
    const writer = module.allocWriter(1);
    const initialPointer = writer.ptr;
    const payload = Uint8Array.from(Array.from({ length: 400 }, (_, index) => index % 256));

    writer.writeBytes(payload);

    expect(writer.ptr).toBeGreaterThan(0);
    expect(writer.ptr).not.toBe(initialPointer);
    expect(writer.capacity).toBeGreaterThan(64);
    expect(Array.from(module.readFromMemory(writer.ptr, writer.len))).toEqual([
      144, 1, 0, 0,
      ...Array.from(payload),
    ]);

    const pointer = writer.ptr;
    const capacity = writer.capacity;
    module.freeWriter(writer);
    expect(freedAllocations).not.toContainEqual([pointer, capacity]);
  });

  it("allocPrimitiveBuffer writes i32 elements and frees by byte size", () => {
    const { module, freedAllocations } = createHarness();
    const allocation = module.allocPrimitiveBuffer([1, -2, 3, -4], "i32");

    expect(allocation.len).toBe(4);
    expect(allocation.allocationSize).toBe(16);

    const raw = module.readFromMemory(allocation.ptr, allocation.allocationSize);
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    expect(view.getInt32(0, true)).toBe(1);
    expect(view.getInt32(4, true)).toBe(-2);
    expect(view.getInt32(8, true)).toBe(3);
    expect(view.getInt32(12, true)).toBe(-4);

    module.freePrimitiveBuffer(allocation);
    expect(freedAllocations).toContainEqual([allocation.ptr, allocation.allocationSize]);
  });

  it("copies mutable primitive buffers back with typed-array bulk operations", () => {
    const { module } = createHarness();
    const allocation = module.allocU64Array(BigUint64Array.from([1n, 2n]));
    const updated = BigUint64Array.from([9n, 10n]);
    module.writeToMemory(
      allocation.ptr,
      new Uint8Array(updated.buffer, updated.byteOffset, updated.byteLength)
    );
    const target = new BigUint64Array(2);

    module.copyPrimitiveBufferInto(allocation, target, "u64");

    expect(Array.from(target)).toEqual([9n, 10n]);
  });

  it("borrows callback vectors directly from Wasm memory", () => {
    const { module } = createHarness();
    const allocation = module.allocI32Array([3, 5, 8]);
    const borrowed = module.borrowI32Array(allocation.ptr, allocation.len);

    borrowed[1] = 13;

    expect(Array.from(module.borrowI32Array(allocation.ptr, allocation.len))).toEqual([3, 13, 8]);
  });

  it("publishes callback vector ownership through the Wasm return slot", () => {
    const { module } = createHarness();
    const allocation = module.allocI32Array([2, 4, 6]);

    module.writeReturnSlot(allocation, 4);

    expect(module.readReturnSlot()).toEqual({
      ptr: allocation.ptr,
      len: 12,
      cap: 12,
      align: 4,
    });
  });

  it("writes the three-word callback buffer descriptor without touching adjacent memory", () => {
    const { module } = createHarness();
    const descriptorPointer = 64;
    module.writeToMemory(descriptorPointer + 12, Uint8Array.from([0xaa, 0xbb, 0xcc, 0xdd]));

    module.writeCallbackBuffer(descriptorPointer, 1024, 24, 32);

    const bytes = module.readFromMemory(descriptorPointer, 16);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    expect(view.getUint32(0, true)).toBe(1024);
    expect(view.getUint32(4, true)).toBe(24);
    expect(view.getUint32(8, true)).toBe(32);
    expect(Array.from(bytes.subarray(12))).toEqual([0xaa, 0xbb, 0xcc, 0xdd]);
  });

  it("writes packed callback success values to unaligned Wasm stack slots", () => {
    const { module } = createHarness();
    const pointer = 20;
    const value = 0x0102_0304_0506_0708n;

    module.writeU64(pointer, value);

    const bytes = module.readFromMemory(pointer, 8);
    expect(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0, true)).toBe(value);
  });

  it("reuses pooled writers when capacity matches", () => {
    const { module } = createHarness();
    const writer = module.allocWriter(8);
    const pointer = writer.ptr;
    writer.writeU32(42);
    module.freeWriter(writer);

    const reusedWriter = module.allocWriter(8);
    expect(reusedWriter.ptr).toBe(pointer);
    expect(reusedWriter.len).toBe(0);
  });

  it("round-trips specialized wire strings and bytes", () => {
    const { module, freedAllocations } = createHarness();
    const stringAllocation = module.allocWireString("boltffi");
    const bytesAllocation = module.allocWireBytes(Uint8Array.from([1, 2, 3]));
    const packedString =
      BigInt(stringAllocation.ptr) | (BigInt(stringAllocation.len) << 32n);
    const packedBytes =
      BigInt(bytesAllocation.ptr) | (BigInt(bytesAllocation.len) << 32n);

    expect(module.takePackedWireString(packedString)).toBe("boltffi");
    expect(Array.from(module.takePackedWireBytes(packedBytes))).toEqual([1, 2, 3]);
    expect(freedAllocations).toContainEqual([
      stringAllocation.ptr,
      stringAllocation.len,
    ]);
    expect(freedAllocations).toContainEqual([
      bytesAllocation.ptr,
      bytesAllocation.len,
    ]);

    const unicodeAllocation = module.allocWireString("BoltFFI ⚡");
    const packedUnicode =
      BigInt(unicodeAllocation.ptr) | (BigInt(unicodeAllocation.len) << 32n);
    expect(module.takePackedWireString(packedUnicode)).toBe("BoltFFI ⚡");
  });

  it("transfers owned UTF-8 strings without wire framing", () => {
    const { module, freedAllocations } = createHarness();
    const ascii = module.allocOwnedString("boltffi");
    const packedAscii = BigInt(ascii.ptr) | (BigInt(ascii.len) << 32n);
    expect(module.takePackedUtf8String(packedAscii)).toBe("boltffi");

    const unicode = module.allocOwnedString("BoltFFI ⚡");
    const packedUnicode = BigInt(unicode.ptr) | (BigInt(unicode.len) << 32n);
    expect(module.takePackedUtf8String(packedUnicode)).toBe("BoltFFI ⚡");
    expect(freedAllocations).toContainEqual([ascii.ptr, ascii.len]);
    expect(freedAllocations).toContainEqual([unicode.ptr, unicode.len]);
  });

  it("preserves every optional f64 state", () => {
    const { module } = createHarness();
    expect(module.unpackOptionF64(4.5)).toBe(4.5);

    module.writeToMemory(16, Uint8Array.from([0, 0, 0, 0]));
    expect(module.unpackOptionF64(Number.NaN)).toBeNull();

    module.writeToMemory(16, Uint8Array.from([1, 0, 0, 0]));
    expect(Number.isNaN(module.unpackOptionF64(Number.NaN))).toBe(true);

    const negativeZero = module.unpackOptionF64Bits(module.packOptionF64Bits(-0));
    expect(Object.is(negativeZero, -0)).toBe(true);
    expect(module.unpackOptionF64Bits(module.packOptionF64Bits(null))).toBeNull();
    expect(Number.isNaN(module.unpackOptionF64Bits(module.packOptionF64Bits(Number.NaN)))).toBe(true);
  });

  it("packs scalar callback options", () => {
    const { module } = createHarness();
    expect(Number.isNaN(module.packOptionScalar(null))).toBe(true);
    expect(module.packOptionScalar(true)).toBe(1);
    expect(module.packOptionScalar(false)).toBe(0);
    expect(module.packOptionScalar(-17)).toBe(-17);
  });
});

describe("BoltFFIModule async completion", () => {
  it("throws invalid argument from completion status", () => {
    const { module, freedAllocations } = createHarness();

    expect(() =>
      module.completeAsync((statusPtr) => {
        const status = new DataView(new ArrayBuffer(4));
        status.setInt32(0, 3, true);
        module.writeToMemory(statusPtr, new Uint8Array(status.buffer));
        return 0;
      })
    ).toThrow("invalid argument");

    expect(freedAllocations).toContainEqual([256, 4]);
  });
});

describe("StreamSession", () => {
  it("drains batches and releases a subscription exactly once", () => {
    const unsubscribe = vi.fn();
    const free = vi.fn();
    const session = new StreamSession(
      7,
      (_handle, maxCount) => [1, 2, 3].slice(0, maxCount),
      () => {},
      new StreamPollManager(),
      unsubscribe,
      free
    );

    expect(session.popBatch(2)).toEqual([1, 2]);
    session.dispose();
    session.dispose();

    expect(unsubscribe).toHaveBeenCalledOnce();
    expect(unsubscribe).toHaveBeenCalledWith(7);
    expect(free).toHaveBeenCalledOnce();
    expect(free).toHaveBeenCalledWith(7);
    expect(session.popBatch()).toEqual([]);
  });

  it("delivers asynchronously produced items through AsyncIterator", async () => {
    const batches: number[][] = [[]];
    const polls = new StreamPollManager();
    const poll = vi.fn();
    const unsubscribe = vi.fn((handle: number) => polls.wake(handle, StreamPollResult.Closed));
    const free = vi.fn();
    const session = new StreamSession(
      3,
      () => batches.shift() ?? [],
      poll,
      polls,
      unsubscribe,
      free
    );
    const iterator = session[Symbol.asyncIterator]();
    const next = iterator.next();

    await Promise.resolve();
    expect(poll).toHaveBeenCalledOnce();
    expect(poll).toHaveBeenCalledWith(3);
    batches.push([9]);
    polls.wake(3, StreamPollResult.Ready);

    expect(await next).toEqual({ value: 9, done: false });
    await iterator.return?.();
    expect(unsubscribe).toHaveBeenCalledOnce();
    expect(free).toHaveBeenCalledOnce();
  });

  it("finishes an invalid subscription without polling or lifecycle calls", async () => {
    const poll = vi.fn();
    const unsubscribe = vi.fn();
    const free = vi.fn();
    const session = new StreamSession(
      0,
      () => [],
      poll,
      new StreamPollManager(),
      unsubscribe,
      free
    );

    expect(await session[Symbol.asyncIterator]().next()).toEqual({ value: undefined, done: true });
    expect(poll).not.toHaveBeenCalled();
    expect(unsubscribe).not.toHaveBeenCalled();
    expect(free).not.toHaveBeenCalled();
  });

  it("wakes and frees a parked iterator when unsubscribed", async () => {
    const polls = new StreamPollManager();
    const unsubscribe = vi.fn((handle: number) => polls.wake(handle, StreamPollResult.Closed));
    const free = vi.fn();
    const session = new StreamSession(11, () => [], () => {}, polls, unsubscribe, free);
    const next = session[Symbol.asyncIterator]().next();

    await Promise.resolve();
    session.unsubscribe();

    expect(await next).toEqual({ value: undefined, done: true });
    expect(unsubscribe).toHaveBeenCalledOnce();
    expect(free).toHaveBeenCalledOnce();
  });

  it("releases a callback subscription when the consumer throws", async () => {
    const unsubscribe = vi.fn();
    const free = vi.fn();
    const session = new StreamSession(
      13,
      () => [1],
      () => {},
      new StreamPollManager(),
      unsubscribe,
      free
    );
    const cancellable = session.consume(() => {
      throw new Error("consumer failed");
    });

    await expect(cancellable.done).rejects.toThrow("consumer failed");
    expect(unsubscribe).toHaveBeenCalledOnce();
    expect(unsubscribe).toHaveBeenCalledWith(13);
    expect(free).toHaveBeenCalledOnce();
    expect(free).toHaveBeenCalledWith(13);
  });
});

describe("CallbackRegistry", () => {
  it("retains callback values until the final release", () => {
    const registry = new CallbackRegistry<(value: number) => number>("Transform");
    const callback = (value: number): number => value * 2;
    const handle = registry.register(callback);

    expect(handle).toBe(0x80000000);
    expect(registry.get(handle)(3)).toBe(6);
    expect(registry.retain(handle)).toBe(handle);
    registry.release(handle);
    expect(registry.get(handle)).toBe(callback);
    registry.release(handle);
    expect(() => registry.get(handle)).toThrow("Transform callback handle");
  });
});

// Minimal module: 1-page memory, boltffi_wasm_abi_version() -> 7, boltffi_wasm_return_slot_addr() -> 0.
const MINIMAL_BOLTFFI_WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 0x03,
  0x03, 0x02, 0x00, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x45, 0x03, 0x06, 0x6d, 0x65, 0x6d,
  0x6f, 0x72, 0x79, 0x02, 0x00, 0x18, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x77, 0x61,
  0x73, 0x6d, 0x5f, 0x61, 0x62, 0x69, 0x5f, 0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, 0x00, 0x00,
  0x1d, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x77, 0x61, 0x73, 0x6d, 0x5f, 0x72, 0x65,
  0x74, 0x75, 0x72, 0x6e, 0x5f, 0x73, 0x6c, 0x6f, 0x74, 0x5f, 0x61, 0x64, 0x64, 0x72, 0x00, 0x01,
  0x0a, 0x0b, 0x02, 0x04, 0x00, 0x41, 0x07, 0x0b, 0x04, 0x00, 0x41, 0x00, 0x0b,
]);

describe("instantiateBoltFFI", () => {
  it("accepts an ArrayBuffer source", async () => {
    const mod = await instantiateBoltFFI(MINIMAL_BOLTFFI_WASM.buffer, 7);
    expect(mod.exports.boltffi_wasm_abi_version()).toBe(7);
  });

  it("accepts a Response source", async () => {
    const response = new Response(MINIMAL_BOLTFFI_WASM);
    const mod = await instantiateBoltFFI(response, 7);
    expect(mod.exports.boltffi_wasm_abi_version()).toBe(7);
  });

  it("accepts an already-compiled WebAssembly.Module source", async () => {
    const precompiled = await WebAssembly.compile(MINIMAL_BOLTFFI_WASM);
    const mod = await instantiateBoltFFI(precompiled, 7);
    expect(mod.exports.boltffi_wasm_abi_version()).toBe(7);
  });

  it("throws a clear ABI mismatch error rather than silently continuing", async () => {
    await expect(instantiateBoltFFI(MINIMAL_BOLTFFI_WASM.buffer, 99)).rejects.toThrow(
      "BoltFFI ABI version mismatch: expected 99, got 7"
    );
  });
});
