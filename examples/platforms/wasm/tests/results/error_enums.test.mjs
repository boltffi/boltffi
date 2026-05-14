import {
  assert,
  assertThrowsWithCode,
  demo,
} from "../support/index.mjs";
import { wireErr, wireOk } from "@boltffi/runtime";

export async function run() {
  // case:results.error_enums.math.checked
  assert.equal(demo.checkedDivide(10, 2), 5);
  assertThrowsWithCode(
    () => demo.checkedDivide(1, 0),
    demo.MathErrorException,
    demo.MathError.DivisionByZero,
  );
  assert.equal(demo.checkedSqrt(9), 3);
  assertThrowsWithCode(
    () => demo.checkedSqrt(-1),
    demo.MathErrorException,
    demo.MathError.NegativeInput,
  );
  assertThrowsWithCode(
    () => demo.checkedAdd(2_147_483_647, 1),
    demo.MathErrorException,
    demo.MathError.Overflow,
  );
  // case:results.error_enums.validation.username
  assert.equal(demo.validateUsername("valid_name"), "valid_name");
  assertThrowsWithCode(
    () => demo.validateUsername("ab"),
    demo.ValidationErrorException,
    demo.ValidationError.TooShort,
  );
  assertThrowsWithCode(
    () => demo.validateUsername("a".repeat(21)),
    demo.ValidationErrorException,
    demo.ValidationError.TooLong,
  );
  assertThrowsWithCode(
    () => demo.validateUsername("has space"),
    demo.ValidationErrorException,
    demo.ValidationError.InvalidFormat,
  );

  assert.equal(demo.mayFail(true), "Success!");
  assert.equal(demo.divideApp(10, 2), 5);

  // case:results.error_enums.api_result.basic
  assert.deepEqual(demo.processValue(3), { tag: "Success" });
  assert.deepEqual(demo.processValue(0), { tag: "ErrorCode", value0: -1 });
  assert.equal(demo.apiResultIsSuccess({ tag: "Success" }), true);
  assert.equal(demo.apiResultIsSuccess({ tag: "ErrorCode", value0: -1 }), false);
  assert.equal(demo.tryCompute(3), 6);
  try {
    demo.tryCompute(-1);
    assert.fail("expected tryCompute(-1) to throw");
  } catch (error) {
    assert.ok(error instanceof demo.ComputeErrorException);
    assert.deepEqual(error.value, { tag: "Overflow", value: -1, limit: 0 });
  }

  // case:results.error_enums.benchmark_response.basic
  const okResponse = demo.createSuccessResponse(7n, { x: 1, y: 2, timestamp: 3n });
  assert.deepEqual(okResponse, {
    requestId: 7n,
    result: { x: 1, y: 2, timestamp: 3n },
  });

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

  assert.equal(demo.isResponseSuccess(successEnvelope), true);
  assert.equal(demo.isResponseSuccess(errorEnvelope), false);
  assert.deepEqual(demo.getResponseValue(successEnvelope), { x: 4, y: 5, timestamp: 6n });
  assert.equal(demo.getResponseValue(errorEnvelope), null);
}
