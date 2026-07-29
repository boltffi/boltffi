import { describe, expect, it } from "vitest";
import { WireReader, WireWriter } from "../src/wire.js";
import { createFakeModule } from "./support/fake-module.js";

/**
 * A borrowed reader is a window into the whole of wasm memory, so both ends of
 * the payload have to be enforced explicitly. Reads that build a typed array
 * bypass the view's own check and go through the same gate.
 */
function payloadInsideLargerBuffer(payload: Uint8Array): ArrayBuffer {
  const backing = new ArrayBuffer(payload.byteLength + 128);
  const bytes = new Uint8Array(backing);
  bytes.set(payload, 64);
  bytes.fill(0xaa, 0, 64);
  bytes.fill(0xaa, 64 + payload.byteLength); // neighbours that must stay unread
  return backing;
}

describe("payload bounds", () => {
  const reader = (payload: Uint8Array): WireReader =>
    new WireReader(payloadInsideLargerBuffer(payload), 64, true, payload.byteLength);

  it("throws instead of reading a scalar past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(1);
    const r = reader(writer.getBytes());

    expect(r.readU32()).toBe(1);
    expect(() => r.readU32()).toThrow(RangeError);
  });

  it("throws instead of reading an array past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(32); // a length the payload cannot satisfy
    expect(() => reader(writer.getBytes()).readU8Array()).toThrow(RangeError);
  });

  it("throws instead of decoding a string past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(32);
    expect(() => reader(writer.getBytes()).readString()).toThrow(RangeError);
  });

  it("throws instead of reading a typed array past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(64);
    expect(() => reader(writer.getBytes()).readF64Array()).toThrow(RangeError);
  });

  it("refuses to skip backwards out of the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(7);
    const r = reader(writer.getBytes());

    expect(r.readU32()).toBe(7);
    expect(() => r.skip(-8)).toThrow(RangeError);
  });

  it("refuses to skip past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(7);
    expect(() => reader(writer.getBytes()).skip(8)).toThrow(RangeError);
  });
});

describe("borrowed reads do not alias the payload", () => {
  it("copies what would otherwise be a view over wasm memory", () => {
    const fake = createFakeModule();
    const writer = new WireWriter();
    writer.writeU32(4);
    writer.writeU8(1);
    writer.writeU8(2);
    writer.writeU8(3);
    writer.writeU8(4);
    const packed = fake.pack(writer.getBytes());
    const pointer = Number(packed & 0xffff_ffffn);
    const length = Number(packed >> 32n);

    const value = fake.module.readPackedBuffer(packed, (r) => r.readU8Array());
    fake.scribble(pointer, length);

    expect(Array.from(value)).toEqual([1, 2, 3, 4]);
  });

  it("still hands out a view when the reader owns its buffer", () => {
    const writer = new WireWriter();
    writer.writeU32(3);
    writer.writeU8(1);
    writer.writeU8(2);
    writer.writeU8(3);
    const bytes = writer.getBytes();

    const value = new WireReader(bytes.buffer, bytes.byteOffset).readU8Array();
    expect(value.buffer).toBe(bytes.buffer);
  });
});

describe("memory growth", () => {
  it("keeps reading the right bytes after wasm memory grows", () => {
    const fake = createFakeModule();
    const write = (n: number): bigint => {
      const writer = new WireWriter();
      writer.writeU32(n);
      return fake.pack(writer.getBytes());
    };

    expect(fake.module.readPackedBuffer(write(11), (r) => r.readU32())).toBe(11);

    const before = fake.memory.buffer;
    fake.memory.grow(2);
    expect(fake.memory.buffer).not.toBe(before);

    expect(fake.module.readPackedBuffer(write(22), (r) => r.readU32())).toBe(22);
  });

  it("detaches a reader that outlives its payload", () => {
    const fake = createFakeModule();
    const writer = new WireWriter();
    writer.writeU32(5);
    let escaped: WireReader | undefined;

    fake.module.readPackedBuffer(fake.pack(writer.getBytes()), (r) => {
      escaped = r;
      return r.readU32();
    });

    expect(() => escaped!.readU32()).toThrow(RangeError);
  });
});
