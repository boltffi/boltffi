import {
  assert,
  assertRejectsWithCode,
  assertRejectsWithMessage,
  demo,
} from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:results.async_results.safe_divide.should_return_quotient");
  assert.equal(await demo.asyncSafeDivide(10, 2), 5);
  globalThis.demoCase("case:results.async_results.safe_divide.should_reject_division_by_zero");
  await assertRejectsWithCode(
    () => demo.asyncSafeDivide(1, 0),
    demo.MathErrorException,
    demo.MathError.DivisionByZero,
  );
  globalThis.demoCase("case:results.async_results.fallible_fetch.should_return_value_for_non_negative_key");
  assert.equal(await demo.asyncFallibleFetch(7), "value_7");
  globalThis.demoCase("case:results.async_results.fallible_fetch.should_reject_negative_key");
  await assertRejectsWithMessage(() => demo.asyncFallibleFetch(-1), "invalid key");
  globalThis.demoCase("case:results.async_results.find_value.should_return_some_for_positive_key");
  assert.equal(await demo.asyncFindValue(4), 40);
  globalThis.demoCase("case:results.async_results.find_value.should_return_none_for_zero_key");
  assert.equal(await demo.asyncFindValue(0), null);
  globalThis.demoCase("case:results.async_results.find_value.should_reject_negative_key");
  await assertRejectsWithMessage(() => demo.asyncFindValue(-1), "invalid key");
}
