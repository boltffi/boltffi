import { wireErr, wireOk } from "@boltffi/runtime";
import { assert, assertRejectsWithCode, assertThrowsWithCode, demo } from "../support/index.mjs";

export async function run() {
  const worker = {
    run(mode) {
      if (mode === 1) return wireErr(demo.MathError.NegativeInput);
      if (mode === 2) throw new Error("unchecked callback failure 東京🦀");
      return wireOk(undefined);
    },
    value(mode) {
      if (mode === 1) return wireErr(demo.MathError.NegativeInput);
      if (mode === 2) throw new Error("unchecked callback failure 東京🦀");
      return wireOk(42);
    },
  };
  const asyncWorker = {
    run: async (mode) => worker.run(mode),
    value: async (mode) => worker.value(mode),
  };
  globalThis.demoCase("case:callbacks.errors.unit.should_report_success");
  assert.equal(demo.invokeUnitWorker(worker, 0), undefined);
  globalThis.demoCase("case:callbacks.errors.unit.should_report_declared_error");
  assertThrowsWithCode(() => demo.invokeUnitWorker(worker, 1), demo.MathErrorException, demo.MathError.NegativeInput);
  globalThis.demoCase("case:callbacks.errors.unit.should_report_unexpected_error");
  assertThrowsWithCode(() => demo.invokeUnitWorker(worker, 2), demo.MathErrorException, demo.MathError.Overflow);
  globalThis.demoCase("case:callbacks.errors.value.should_report_success");
  assert.equal(demo.invokeValueWorker(worker, 0), 42);
  globalThis.demoCase("case:callbacks.errors.value.should_report_declared_error");
  assertThrowsWithCode(() => demo.invokeValueWorker(worker, 1), demo.MathErrorException, demo.MathError.NegativeInput);
  globalThis.demoCase("case:callbacks.errors.value.should_report_unexpected_error");
  assertThrowsWithCode(() => demo.invokeValueWorker(worker, 2), demo.MathErrorException, demo.MathError.Overflow);
  globalThis.demoCase("case:callbacks.errors.async_unit.should_report_success");
  assert.equal(await demo.invokeAsyncUnitWorker(asyncWorker, 0), undefined);
  globalThis.demoCase("case:callbacks.errors.async_unit.should_report_declared_error");
  await assertRejectsWithCode(() => demo.invokeAsyncUnitWorker(asyncWorker, 1), demo.MathErrorException, demo.MathError.NegativeInput);
  globalThis.demoCase("case:callbacks.errors.async_unit.should_report_unexpected_error");
  await assertRejectsWithCode(() => demo.invokeAsyncUnitWorker(asyncWorker, 2), demo.MathErrorException, demo.MathError.Overflow);
  globalThis.demoCase("case:callbacks.errors.async_value.should_report_success");
  assert.equal(await demo.invokeAsyncValueWorker(asyncWorker, 0), 42);
  globalThis.demoCase("case:callbacks.errors.async_value.should_report_declared_error");
  await assertRejectsWithCode(() => demo.invokeAsyncValueWorker(asyncWorker, 1), demo.MathErrorException, demo.MathError.NegativeInput);
  globalThis.demoCase("case:callbacks.errors.async_value.should_report_unexpected_error");
  await assertRejectsWithCode(() => demo.invokeAsyncValueWorker(asyncWorker, 2), demo.MathErrorException, demo.MathError.Overflow);
  assertThrowsWithCode(
    () => demo.applyResultClosure(() => { throw new Error("unexpected closure error"); }, 0),
    demo.MathErrorException,
    demo.MathError.Overflow,
  );
}
