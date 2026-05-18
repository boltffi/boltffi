import {
  assert,
  assertThrowsWithCode,
  demo,
} from "../support/index.mjs";
import { wireErr, wireOk } from "@boltffi/runtime";

export async function run() {
  globalThis.demoCase("case:results.error_enums.checked_divide.should_return_quotient");
  assert.equal(demo.checkedDivide(10, 2), 5);
  globalThis.demoCase("case:results.error_enums.checked_divide.should_reject_division_by_zero");
  assertThrowsWithCode(
    () => demo.checkedDivide(1, 0),
    demo.MathErrorException,
    demo.MathError.DivisionByZero,
  );
  globalThis.demoCase("case:results.error_enums.checked_sqrt.should_return_square_root");
  assert.equal(demo.checkedSqrt(9), 3);
  globalThis.demoCase("case:results.error_enums.checked_sqrt.should_reject_negative_input");
  assertThrowsWithCode(
    () => demo.checkedSqrt(-1),
    demo.MathErrorException,
    demo.MathError.NegativeInput,
  );
  globalThis.demoCase("case:results.error_enums.checked_add.should_return_sum");
  assert.equal(demo.checkedAdd(2, 3), 5);
  globalThis.demoCase("case:results.error_enums.checked_add.should_reject_overflow");
  assertThrowsWithCode(
    () => demo.checkedAdd(2_147_483_647, 1),
    demo.MathErrorException,
    demo.MathError.Overflow,
  );
  globalThis.demoCase("case:results.error_enums.validate_username.should_accept_valid_name");
  assert.equal(demo.validateUsername("valid_name"), "valid_name");
  globalThis.demoCase("case:results.error_enums.validate_username.should_reject_too_short_name");
  assertThrowsWithCode(
    () => demo.validateUsername("ab"),
    demo.ValidationErrorException,
    demo.ValidationError.TooShort,
  );
  globalThis.demoCase("case:results.error_enums.validate_username.should_reject_too_long_name");
  assertThrowsWithCode(
    () => demo.validateUsername("a".repeat(21)),
    demo.ValidationErrorException,
    demo.ValidationError.TooLong,
  );
  globalThis.demoCase("case:results.error_enums.validate_username.should_reject_invalid_format");
  assertThrowsWithCode(
    () => demo.validateUsername("has space"),
    demo.ValidationErrorException,
    demo.ValidationError.InvalidFormat,
  );

  globalThis.demoCase("case:results.error_enums.may_fail.should_return_success_when_valid");
  assert.equal(demo.mayFail(true), "Success!");
  globalThis.demoCase("case:results.error_enums.divide_app.should_return_quotient");
  assert.equal(demo.divideApp(10, 2), 5);

  globalThis.demoCase("case:results.error_enums.process_value.should_return_success_variant");
  assert.deepEqual(demo.processValue(3), { tag: "Success" });
  globalThis.demoCase("case:results.error_enums.process_value.should_return_error_code_variant");
  assert.deepEqual(demo.processValue(0), { tag: "ErrorCode", value0: -1 });
  globalThis.demoCase("case:results.error_enums.process_value.should_return_error_with_data_variant");
  assert.deepEqual(demo.processValue(-3), { tag: "ErrorWithData", code: -3, detail: -6 });
  globalThis.demoCase("case:results.error_enums.api_result_is_success.should_report_success_variant");
  assert.equal(demo.apiResultIsSuccess({ tag: "Success" }), true);
  globalThis.demoCase("case:results.error_enums.api_result_is_success.should_report_error_variant");
  assert.equal(demo.apiResultIsSuccess({ tag: "ErrorCode", value0: -1 }), false);
  globalThis.demoCase("case:results.error_enums.try_compute.should_return_doubled_value");
  assert.equal(demo.tryCompute(3), 6);
  globalThis.demoCase("case:results.error_enums.try_compute.should_return_overflow_error");
  try {
    demo.tryCompute(-1);
    assert.fail("expected tryCompute(-1) to throw");
  } catch (error) {
    assert.ok(error instanceof demo.ComputeErrorException);
    assert.deepEqual(error.value, { tag: "Overflow", value: -1, limit: 0 });
  }

  globalThis.demoCase("case:results.error_enums.benchmark_response.should_make_success_response");
  const okResponse = demo.createSuccessResponse(7n, { x: 1, y: 2, timestamp: 3n });
  assert.deepEqual(okResponse, {
    requestId: 7n,
    result: { x: 1, y: 2, timestamp: 3n },
  });

  globalThis.demoCase("case:results.error_enums.benchmark_response.should_make_error_response");
  try {
    demo.createErrorResponse(8n, { tag: "InvalidInput", value0: -9 });
    assert.fail("expected createErrorResponse to throw");
  } catch (error) {
    assert.ok(error instanceof demo.ComputeErrorException);
    assert.deepEqual(error.value, { tag: "InvalidInput", value0: -9 });
  }

  const successEnvelope = {
    requestId: 11n,
    result: wireOk({ x: 4, y: 5, timestamp: 6n }),
  };
  const errorEnvelope = {
    requestId: 12n,
    result: wireErr({ tag: "InvalidInput", value0: -2 }),
  };

  globalThis.demoCase("case:results.error_enums.benchmark_response.should_report_success_response");
  assert.equal(demo.isResponseSuccess(successEnvelope), true);
  globalThis.demoCase("case:results.error_enums.benchmark_response.should_report_error_response");
  assert.equal(demo.isResponseSuccess(errorEnvelope), false);
  globalThis.demoCase("case:results.error_enums.benchmark_response.should_return_value_for_success_response");
  assert.deepEqual(demo.getResponseValue(successEnvelope), { x: 4, y: 5, timestamp: 6n });
  globalThis.demoCase("case:results.error_enums.benchmark_response.should_return_none_for_error_response");
  assert.equal(demo.getResponseValue(errorEnvelope), null);
}
