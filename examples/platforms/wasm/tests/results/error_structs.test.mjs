import { assert, demo } from "../support/index.mjs";

export async function run() {
  // case:results.error_enums.app_error.basic
  assert.equal(demo.mayFail(true), "Success!");

  try {
    demo.mayFail(false);
    assert.fail("expected mayFail(false) to throw");
  } catch (error) {
    assert.ok(error instanceof demo.AppErrorException);
    assert.deepEqual(error.value, { code: 400, message: "Invalid input" });
  }

  assert.equal(demo.divideApp(10, 2), 5);

  try {
    demo.divideApp(10, 0);
    assert.fail("expected divideApp(10, 0) to throw");
  } catch (error) {
    assert.ok(error instanceof demo.AppErrorException);
    assert.deepEqual(error.value, { code: 500, message: "Division by zero" });
  }
}
