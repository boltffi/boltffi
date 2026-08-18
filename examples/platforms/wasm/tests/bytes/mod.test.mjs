import { assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:bytes.bytes.should_roundtrip_values");
  assertArrayEqual(demo.echoBytes(Uint8Array.from([1, 2, 3, 4])), [1, 2, 3, 4]);
  assertArrayEqual(demo.echoBytes(Uint8Array.from([])), []);
  globalThis.demoCase("case:bytes.bytes.should_make_sequential_values");
  assertArrayEqual(demo.makeBytes(4), [0, 1, 2, 3]);
  globalThis.demoCase("case:bytes.bytes.should_reverse_values");
  assertArrayEqual(demo.reverseBytes(Uint8Array.from([1, 2, 3, 4])), [4, 3, 2, 1]);
  globalThis.demoCase("case:bytes.bytes.should_report_length");
  if (demo.bytesLength(Uint8Array.from([9, 8, 7])) !== 3) {
    throw new Error("bytesLength returned incorrect count");
  }
  globalThis.demoCase("case:bytes.bytes.should_sum_values");
  if (demo.bytesSum(Uint8Array.from([1, 2, 3, 4])) !== 10) {
    throw new Error("bytesSum returned incorrect sum");
  }

  // A returned buffer whose capacity exceeds its length takes a different path
  // out of the module: the capacity has to go before the pointer can cross,
  // and shrinking it in place must not disturb the payload.
  const overallocated = demo.generateBytesOverallocated(1000);
  if (overallocated.length !== 1000) {
    throw new Error(
      `generateBytesOverallocated returned ${overallocated.length} bytes, expected 1000`,
    );
  }
  if (!overallocated.every((byte) => byte === 42)) {
    throw new Error("generateBytesOverallocated returned a corrupted payload");
  }
  assertArrayEqual(demo.generateBytesOverallocated(0), []);
}
