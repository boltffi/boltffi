import { assert, assertArrayEqual, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.resultOfOption(4), 8);
  assert.equal(demo.resultOfOption(0), null);
  assertThrowsWithMessage(() => demo.resultOfOption(-1), "invalid key");
  assertArrayEqual(demo.resultOfVec(3), [0, 1, 2]);
  assertThrowsWithMessage(() => demo.resultOfVec(-1), "negative count");
  assert.equal(demo.resultOfString(7), "item_7");
  assertThrowsWithMessage(() => demo.resultOfString(-1), "invalid key");
}
