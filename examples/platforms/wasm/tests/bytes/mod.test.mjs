import { assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  // case:bytes.echo.basic
  assertArrayEqual(demo.echoBytes(Uint8Array.from([1, 2, 3, 4])), [1, 2, 3, 4]);
  assertArrayEqual(demo.echoBytes(Uint8Array.from([])), []);
  // case:bytes.make.basic
  assertArrayEqual(demo.makeBytes(4), [0, 1, 2, 3]);
  // case:bytes.reverse.basic
  assertArrayEqual(demo.reverseBytes(Uint8Array.from([1, 2, 3, 4])), [4, 3, 2, 1]);
  if (demo.bytesLength(Uint8Array.from([9, 8, 7])) !== 3) {
    throw new Error("case:bytes.length.basic bytesLength returned incorrect count");
  }
  if (demo.bytesSum(Uint8Array.from([1, 2, 3, 4])) !== 10) {
    throw new Error("case:bytes.sum.basic bytesSum returned incorrect sum");
  }
}
