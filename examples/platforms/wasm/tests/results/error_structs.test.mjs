import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:results.error_enums.may_fail.should_return_success_when_valid");
  assert.equal(demo.mayFail(true), "Success!");

  globalThis.demoCase("case:results.error_enums.may_fail.should_return_app_error_when_invalid");
  try {
    demo.mayFail(false);
    assert.fail("expected mayFail(false) to throw");
  } catch (error) {
    assert.ok(error instanceof demo.AppErrorException);
    assert.deepEqual(error.value, { code: 400, message: "Invalid input" });
  }

  globalThis.demoCase("case:results.error_enums.divide_app.should_return_quotient");
  assert.equal(demo.divideApp(10, 2), 5);

  globalThis.demoCase("case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero");
  try {
    demo.divideApp(10, 0);
    assert.fail("expected divideApp(10, 0) to throw");
  } catch (error) {
    assert.ok(error instanceof demo.AppErrorException);
    assert.deepEqual(error.value, { code: 500, message: "Division by zero" });
  }
}
