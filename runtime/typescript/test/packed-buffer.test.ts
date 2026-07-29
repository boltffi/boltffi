import { describe, expect, it } from "vitest";
import type { WireReader } from "../src/wire.js";
import { createFakeModule } from "./support/fake-module.js";

/**
 * `readPackedBuffer` lends a reader memory that is freed the moment the
 * callback returns, so a reader that outlives the callback must not be able to
 * read. The pooled reader is detached on the way out; the reader handed to a
 * nested call has to be detached too, or it keeps pointing at freed memory.
 */
describe("readPackedBuffer detaches the reader it lends", () => {
  const payload = (): Uint8Array => {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setUint32(0, 0x1234, true);
    new DataView(bytes.buffer).setUint32(4, 0x5678, true);
    return bytes;
  };

  it("detaches the pooled reader", () => {
    const fake = createFakeModule();
    let escaped: WireReader | undefined;

    const value = fake.module.readPackedBuffer(fake.pack(payload()), (reader) => {
      escaped = reader;
      return reader.readU32();
    });

    expect(value).toBe(0x1234);
    expect(() => escaped!.readU32()).toThrow(RangeError);
  });

  it("detaches the reader handed to a nested call", () => {
    const fake = createFakeModule();
    let escapedInner: WireReader | undefined;

    const value = fake.module.readPackedBuffer(fake.pack(payload()), (outer) => {
      const inner = fake.module.readPackedBuffer(fake.pack(payload()), (reader) => {
        escapedInner = reader;
        return reader.readU32();
      });
      // The outer reader must survive the nested call untouched.
      return inner + outer.readU32();
    });

    expect(value).toBe(0x1234 * 2);
    expect(escapedInner).toBeDefined();
    expect(escapedInner).not.toBe(undefined);
    expect(() => escapedInner!.readU32()).toThrow(RangeError);
  });

  it("frees both payloads when a call nests", () => {
    const fake = createFakeModule();

    fake.module.readPackedBuffer(fake.pack(payload()), (outer) => {
      fake.module.readPackedBuffer(fake.pack(payload()), (inner) => inner.readU32());
      return outer.readU32();
    });

    expect(fake.freed).toHaveLength(2);
  });

  it("gives a nested call its own reader rather than resetting the outer one", () => {
    const fake = createFakeModule();

    const value = fake.module.readPackedBuffer(fake.pack(payload()), (outer) => {
      const inner = fake.module.readPackedBuffer(fake.pack(payload()), (r) => r.readU32());
      // Would read the nested payload's second word if the reader were shared.
      return `${inner}:${outer.readU32()}:${outer.readU32()}`;
    });

    expect(value).toBe(`${0x1234}:${0x1234}:${0x5678}`);
  });
});
