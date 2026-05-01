import { assert, assertArrayEqual, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.resultOfOption(4), 8);
  assert.equal(demo.resultOfOption(0), null);
  assertThrowsWithMessage(() => demo.resultOfOption(-1), "invalid key");
  assertArrayEqual(demo.resultOfVec(3), [0, 1, 2]);
  assertThrowsWithMessage(() => demo.resultOfVec(-1), "negative count");
  assert.equal(demo.resultOfString(7), "item_7");
  assertThrowsWithMessage(() => demo.resultOfString(-1), "invalid key");

  // Result<i32, i32>: the Err side isn't String or a #[error] type,
  // so the TypeScript wrapper falls back to `new Error(String(value))`.
  assert.equal(demo.resultWithIntError(5), 5);
  assertThrowsWithMessage(() => demo.resultWithIntError(-7), "-7");
}
