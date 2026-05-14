import {
  assert,
  assertRejectsWithCode,
  assertRejectsWithMessage,
  demo,
} from "../support/index.mjs";

export async function run() {
  // case:results.async_results.math_fetch_find
  assert.equal(await demo.asyncSafeDivide(10, 2), 5);
  await assertRejectsWithCode(
    () => demo.asyncSafeDivide(1, 0),
    demo.MathErrorException,
    demo.MathError.DivisionByZero,
  );
  assert.equal(await demo.asyncFallibleFetch(7), "value_7");
  await assertRejectsWithMessage(() => demo.asyncFallibleFetch(-1), "invalid key");
  assert.equal(await demo.asyncFindValue(4), 40);
  assert.equal(await demo.asyncFindValue(0), null);
  await assertRejectsWithMessage(() => demo.asyncFindValue(-1), "invalid key");
}
