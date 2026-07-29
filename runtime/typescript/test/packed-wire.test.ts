import { describe, expect, it } from "vitest";
import { createFakeModule } from "./support/fake-module.js";

/**
 * Wire bytes and strings arrive as a 4-byte length followed by the payload.
 * The value handed back must own its bytes, because the payload is freed
 * before the caller ever sees it, and a length that does not describe the
 * region must be rejected rather than silently truncated.
 */
function wireBytes(payload: Uint8Array): Uint8Array {
  const framed = new Uint8Array(4 + payload.length);
  new DataView(framed.buffer).setUint32(0, payload.length, true);
  framed.set(payload, 4);
  return framed;
}

describe("takePackedWireBytes", () => {
  it("returns bytes that do not alias wasm memory", () => {
    const fake = createFakeModule();
    const packed = fake.pack(wireBytes(new Uint8Array([1, 2, 3, 4])));
    const { pointer, length } = { pointer: Number(packed & 0xffff_ffffn), length: Number(packed >> 32n) };

    const value = fake.module.takePackedWireBytes(packed);
    fake.scribble(pointer, length);

    expect(Array.from(value)).toEqual([1, 2, 3, 4]);
  });

  it("frees the payload", () => {
    const fake = createFakeModule();
    fake.module.takePackedWireBytes(fake.pack(wireBytes(new Uint8Array([9]))));
    expect(fake.freed).toHaveLength(1);
  });

  it("returns an empty array for an empty payload", () => {
    const fake = createFakeModule();
    const value = fake.module.takePackedWireBytes(fake.pack(wireBytes(new Uint8Array(0))));
    expect(value).toHaveLength(0);
  });

  it("rejects a length that disagrees with the frame", () => {
    const fake = createFakeModule();
    const framed = wireBytes(new Uint8Array([1, 2, 3]));
    const packed = fake.pack(framed);
    const pointer = Number(packed & 0xffff_ffffn);

    // Frame says 3 bytes of payload, packed length claims one more.
    expect(() => fake.module.takePackedWireBytes(fake.packRaw(pointer, framed.length + 1)))
      .toThrow(/Invalid packed wire bytes/);
  });

  it("rejects a payload that runs past the end of memory", () => {
    const fake = createFakeModule();
    const size = fake.memory.buffer.byteLength;
    const pointer = size - 16;
    const view = new DataView(fake.memory.buffer);
    view.setUint32(pointer, 1_000_000, true);

    // `slice` would clamp here and hand back a short array; it must throw.
    expect(() => fake.module.takePackedWireBytes(fake.packRaw(pointer, 1_000_004)))
      .toThrow(/Invalid packed wire bytes/);
  });
});

describe("takePackedWireString", () => {
  const encode = (text: string): Uint8Array => wireBytes(new TextEncoder().encode(text));

  it("decodes and frees", () => {
    const fake = createFakeModule();
    const value = fake.module.takePackedWireString(fake.pack(encode("olá mundo")));
    expect(value).toBe("olá mundo");
    expect(fake.freed).toHaveLength(1);
  });

  it("rejects a length that disagrees with the frame", () => {
    const fake = createFakeModule();
    const framed = encode("abc");
    const pointer = Number(fake.pack(framed) & 0xffff_ffffn);

    expect(() => fake.module.takePackedWireString(fake.packRaw(pointer, framed.length + 1)))
      .toThrow(/Invalid packed wire string/);
  });

  it("rejects a payload that runs past the end of memory", () => {
    const fake = createFakeModule();
    const pointer = fake.memory.buffer.byteLength - 16;
    new DataView(fake.memory.buffer).setUint32(pointer, 1_000_000, true);

    expect(() => fake.module.takePackedWireString(fake.packRaw(pointer, 1_000_004)))
      .toThrow(/Invalid packed wire string/);
  });
});
