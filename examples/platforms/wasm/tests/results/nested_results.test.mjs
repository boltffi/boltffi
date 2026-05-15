import { assert, assertArrayEqual, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:results.nested_results.option.should_return_some_for_positive_key");
  assert.equal(demo.resultOfOption(4), 8);
  globalThis.demoCase("case:results.nested_results.option.should_return_none_for_zero_key");
  assert.equal(demo.resultOfOption(0), null);
  globalThis.demoCase("case:results.nested_results.option.should_reject_negative_key");
  assertThrowsWithMessage(() => demo.resultOfOption(-1), "invalid key");
  globalThis.demoCase("case:results.nested_results.vec.should_return_values_for_non_negative_count");
  assertArrayEqual(demo.resultOfVec(3), [0, 1, 2]);
  globalThis.demoCase("case:results.nested_results.vec.should_reject_negative_count");
  assertThrowsWithMessage(() => demo.resultOfVec(-1), "negative count");
  globalThis.demoCase("case:results.nested_results.string.should_return_value_for_non_negative_key");
  assert.equal(demo.resultOfString(7), "item_7");
  globalThis.demoCase("case:results.nested_results.string.should_reject_negative_key");
  assertThrowsWithMessage(() => demo.resultOfString(-1), "invalid key");
}
