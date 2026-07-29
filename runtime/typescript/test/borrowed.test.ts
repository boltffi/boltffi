import { describe, expect, it } from "vitest";
import { WireReader, WireWriter } from "../src/wire.js";

/**
 * A reader in borrowed mode points at memory the caller frees once the read
 * returns, so nothing it hands back may alias that memory. These tests write a
 * payload, decode it through a borrowed reader, then overwrite the source bytes
 * — anything that changed underneath was aliasing and would have been a
 * use-after-free in the generated glue.
 */
function borrowedOver(bytes: Uint8Array): {
  reader: WireReader;
  scribble: () => void;
} {
  // Copy into a standalone buffer we can scribble over, mimicking the payload
  // being freed and the region reused.
  const backing = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(backing).set(bytes);
  return {
    reader: new WireReader(backing, 0, true, backing.byteLength),
    scribble: () => new Uint8Array(backing).fill(0xff),
  };
}

describe("borrowed reader does not alias the payload", () => {
  it("copies u8 arrays", () => {
    const writer = new WireWriter();
    writer.writeBytes(new Uint8Array([1, 2, 3, 4])); // len + payload: what readU8Array expects
    const { reader, scribble } = borrowedOver(writer.getBytes());

    const value = reader.readU8Array();
    scribble();
    expect(Array.from(value)).toEqual([1, 2, 3, 4]);
  });

  it("copies i8 arrays", () => {
    const writer = new WireWriter();
    writer.writeBytes(new Uint8Array(new Int8Array([-1, 2, -3]).buffer));
    const { reader, scribble } = borrowedOver(writer.getBytes());

    const value = reader.readI8Array();
    scribble();
    expect(Array.from(value)).toEqual([-1, 2, -3]);
  });

  it("copies typed arrays of every width", () => {
    const writer = new WireWriter();
    writer.writeU32(2);
    writer.writeI32(-5);
    writer.writeI32(6);
    const { reader, scribble } = borrowedOver(writer.getBytes());

    const value = reader.readI32Array();
    scribble();
    expect(Array.from(value)).toEqual([-5, 6]);
  });

  it("copies f64 arrays", () => {
    const writer = new WireWriter();
    writer.writeU32(2);
    writer.writeF64(1.5);
    writer.writeF64(-2.25);
    const { reader, scribble } = borrowedOver(writer.getBytes());

    const value = reader.readF64Array();
    scribble();
    expect(Array.from(value)).toEqual([1.5, -2.25]);
  });

  it("keeps strings, bytes and bool arrays owned", () => {
    const writer = new WireWriter();
    writer.writeString("olá mundo");
    writer.writeBytes(new Uint8Array([7, 8]));
    writer.writeArray([true, false], (value) => writer.writeBool(value));
    const { reader, scribble } = borrowedOver(writer.getBytes());

    const text = reader.readString();
    const bytes = reader.readBytes();
    const flags = reader.readBoolArray();
    scribble();

    expect(text).toBe("olá mundo");
    expect(Array.from(bytes)).toEqual([7, 8]);
    expect(flags).toEqual([true, false]);
  });

  it("still hands out views when not borrowed", () => {
    // The zero-copy path is the point of non-borrowed mode; assert it survives.
    const writer = new WireWriter();
    writer.writeBytes(new Uint8Array([1, 2, 3]));
    const bytes = writer.getBytes();
    const reader = new WireReader(bytes.buffer, bytes.byteOffset);

    const value = reader.readU8Array();
    expect(value.buffer).toBe(bytes.buffer);
  });
});

describe("reset", () => {
  it("repoints a reader at another buffer and mode", () => {
    const first = new WireWriter();
    first.writeU32(11);
    const second = new WireWriter();
    second.writeU32(22);

    const reader = new WireReader(first.getBytes().buffer, 0, false);
    expect(reader.readU32()).toBe(11);

    const secondBytes = second.getBytes();
    reader.reset(secondBytes.buffer, 0, secondBytes.byteLength, true);
    expect(reader.readU32()).toBe(22);
  });

  it("applies the borrowed flag it is given", () => {
    const writer = new WireWriter();
    writer.writeBytes(new Uint8Array([4, 5]));
    const bytes = writer.getBytes();

    const reader = new WireReader(new ArrayBuffer(0), 0, false);
    reader.reset(bytes.buffer, bytes.byteOffset, bytes.byteLength, true);

    const value = reader.readU8Array();
    expect(value.buffer).not.toBe(bytes.buffer);
    expect(Array.from(value)).toEqual([4, 5]);
  });
});

describe("payload bounds", () => {
  /**
   * The reader spans only the payload, so a truncated or malformed length
   * cannot walk into whatever wasm allocated next — it throws instead of
   * returning adjacent memory as if it were data.
   */
  function payloadInsideLargerBuffer(payload: Uint8Array): ArrayBuffer {
    const backing = new ArrayBuffer(payload.byteLength + 64);
    const bytes = new Uint8Array(backing);
    bytes.set(payload);
    bytes.fill(0xaa, payload.byteLength); // the neighbour that must stay unread
    return backing;
  }

  it("throws instead of reading a scalar past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(1);
    const payload = writer.getBytes();
    const reader = new WireReader(
      payloadInsideLargerBuffer(payload),
      0,
      true,
      payload.byteLength
    );

    expect(reader.readU32()).toBe(1);
    expect(() => reader.readU32()).toThrow();
  });

  it("throws instead of reading an array past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(32); // a length the payload cannot satisfy
    const payload = writer.getBytes();
    const reader = new WireReader(
      payloadInsideLargerBuffer(payload),
      0,
      true,
      payload.byteLength
    );

    expect(() => reader.readU8Array()).toThrow(RangeError);
  });

  it("throws instead of decoding a string past the payload", () => {
    const writer = new WireWriter();
    writer.writeU32(32);
    const payload = writer.getBytes();
    const reader = new WireReader(
      payloadInsideLargerBuffer(payload),
      0,
      true,
      payload.byteLength
    );

    expect(() => reader.readString()).toThrow(RangeError);
  });
});

describe("invalidate", () => {
  it("makes a reader that outlived its payload throw", () => {
    const writer = new WireWriter();
    writer.writeU32(7);
    const bytes = writer.getBytes();
    const reader = new WireReader(bytes.buffer, bytes.byteOffset, true, 4);

    expect(reader.readU32()).toBe(7);
    reader.invalidate();
    expect(() => reader.readU32()).toThrow();
  });
});
