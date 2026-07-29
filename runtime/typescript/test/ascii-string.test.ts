import { describe, expect, it } from "vitest";
import { WireReader, WireWriter } from "../src/wire.js";

/**
 * Short ASCII strings take a fast path that builds the string from its char
 * codes instead of decoding it. It must be indistinguishable from
 * `TextDecoder` for every input, so these tests are equivalence tests: the
 * threshold and the bail-out are where a divergence would hide.
 */
const roundtrip = (value: string): string => {
  const writer = new WireWriter();
  writer.writeString(value);
  const bytes = writer.getBytes();
  return new WireReader(bytes.buffer, bytes.byteOffset, false).readString();
};

describe("readString", () => {
  it("matches across and around the threshold", () => {
    for (const len of [0, 1, 2, 8, 15, 16, 17, 24, 64, 200, 1000]) {
      const value = "a".repeat(len);
      expect(roundtrip(value), `${len} bytes`).toBe(value);
    }
  });

  it("falls back when any byte is above ascii", () => {
    for (const value of ["ç", "ça", "aç", "aça", "ção", "áéíóúàèìòù", "日本語", "🎉"]) {
      expect(roundtrip(value), value).toBe(value);
    }
  });

  it("bails out wherever the multi-byte sequence sits", () => {
    // Start, middle and end of a string short enough to enter the fast path.
    expect(roundtrip("çaaaaaaa")).toBe("çaaaaaaa");
    expect(roundtrip("aaaaçaaa")).toBe("aaaaçaaa");
    expect(roundtrip("aaaaaaaç")).toBe("aaaaaaaç");
  });

  it("handles a multi-byte sequence straddling the threshold", () => {
    // 14 ascii + 2 bytes lands exactly on the limit and must bail on the last
    // character; 15 + 2 exceeds it and never enters the fast path at all.
    expect(roundtrip("a".repeat(14) + "ç")).toBe("a".repeat(14) + "ç");
    expect(roundtrip("a".repeat(15) + "ç")).toBe("a".repeat(15) + "ç");
  });

  it("preserves every ascii code point", () => {
    let value = "";
    for (let code = 0; code < 128; code++) value += String.fromCharCode(code);
    // Longer than the fast path, so also read it in short slices.
    expect(roundtrip(value)).toBe(value);
    for (let start = 0; start < 128; start += 8) {
      const chunk = value.slice(start, start + 8);
      expect(roundtrip(chunk), `codes ${start}..${start + 8}`).toBe(chunk);
    }
  });

  it("does not carry the scratch buffer between reads", () => {
    const writer = new WireWriter();
    writer.writeString("abcdefgh");
    writer.writeString("xy");
    writer.writeString("");
    writer.writeString("zzz");
    const bytes = writer.getBytes();
    const reader = new WireReader(bytes.buffer, bytes.byteOffset, false);

    expect(reader.readString()).toBe("abcdefgh");
    expect(reader.readString()).toBe("xy");
    expect(reader.readString()).toBe("");
    expect(reader.readString()).toBe("zzz");
  });

  it("re-reads after being pointed at another buffer", () => {
    const first = new WireWriter();
    first.writeString("first");
    const second = new WireWriter();
    second.writeString("second");
    const firstBytes = first.getBytes();
    const secondBytes = second.getBytes();

    const reader = new WireReader(firstBytes.buffer, firstBytes.byteOffset, false);
    expect(reader.readString()).toBe("first");

    reader.reset(secondBytes.buffer, secondBytes.byteOffset, secondBytes.byteLength, false);
    expect(reader.readString()).toBe("second");
  });
});
