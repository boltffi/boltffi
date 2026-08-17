import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('error enum results', () async {
    expect(
      checkedDivide(10, 2),
      5,
      reason: 'case:results.error_enums.checked_divide.should_return_quotient',
    );
    expect(
      () => checkedDivide(10, 0),
      throwsA(MathError.divisionByZero),
      reason:
          'case:results.error_enums.checked_divide.should_reject_division_by_zero',
    );

    expect(
      checkedSqrt(9.0),
      3.0,
      reason: 'case:results.error_enums.checked_sqrt.should_return_square_root',
    );
    expect(
      () => checkedSqrt(-1.0),
      throwsA(MathError.negativeInput),
      reason:
          'case:results.error_enums.checked_sqrt.should_reject_negative_input',
    );

    expect(
      checkedAdd(1, 2),
      3,
      reason: 'case:results.error_enums.checked_add.should_return_sum',
    );
    expect(
      () => checkedAdd(2147483647, 1),
      throwsA(MathError.overflow),
      reason: 'case:results.error_enums.checked_add.should_reject_overflow',
    );

    expect(
      mayFail(true),
      'Success!',
      reason:
          'case:results.error_enums.may_fail.should_return_success_when_valid',
    );
    expect(
      () => mayFail(false),
      throwsA(AppError(code: 400, message: "Invalid input")),
      reason:
          'case:results.error_enums.may_fail.should_return_app_error_when_invalid',
    );

    expect(
      divideApp(10, 2),
      5,
      reason: 'case:results.error_enums.divide_app.should_return_quotient',
    );
    expect(
      () => divideApp(10, 0),
      throwsA(AppError(code: 500, message: "Division by zero")),
      reason:
          'case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero',
    );

    expect(
      validateUsername('alice'),
      'alice',
      reason:
          'case:results.error_enums.validate_username.should_accept_valid_name',
    );
    expect(
      () => validateUsername('ab'),
      throwsA(ValidationError.tooShort),
      reason:
          'case:results.error_enums.validate_username.should_reject_too_short_name',
    );
    expect(
      () => validateUsername('a]bcdefghijklmnopqrstu'),
      throwsA(ValidationError.tooLong),
      reason:
          'case:results.error_enums.validate_username.should_reject_too_long_name',
    );
    expect(
      () => validateUsername('has space'),
      throwsA(ValidationError.invalidFormat),
      reason:
          'case:results.error_enums.validate_username.should_reject_invalid_format',
    );

    expect(
      processValue(5),
      isA<ApiResult$Success>(),
      reason:
          'case:results.error_enums.process_value.should_return_success_variant',
    );
    final zeroResult = processValue(0);
    expect(
      zeroResult,
      isA<ApiResult$ErrorCode>(),
      reason:
          'case:results.error_enums.process_value.should_return_error_code_variant',
    );
    expect((zeroResult as ApiResult$ErrorCode).value0, -1, reason: 'case:');

    final negResult = processValue(-3);
    expect(
      negResult,
      isA<ApiResult$ErrorWithData>(),
      reason:
          'case:results.error_enums.process_value.should_return_error_with_data_variant',
    );

    expect(
      apiResultIsSuccess(ApiResult.success()),
      isTrue,
      reason:
          'case:results.error_enums.api_result_is_success.should_report_success_variant',
    );
    expect(
      apiResultIsSuccess(ApiResult.errorCode(value0: -1)),
      isFalse,
      reason:
          'case:results.error_enums.api_result_is_success.should_report_error_variant',
    );

    expect(
      tryCompute(21),
      42,
      reason:
          'case:results.error_enums.try_compute.should_return_doubled_value',
    );
    expect(
      () => tryCompute(-3),
      throwsA(ComputeError.overflow(value: -3, limit: 0)),
      reason:
          'case:results.error_enums.try_compute.should_return_overflow_error',
    );

    final benchmarkError = ComputeError.overflow(value: -5, limit: 0);

    final errorResponse = createErrorResponse(43, benchmarkError);
    expect(
      errorResponse.result.errValue(),
      benchmarkError,
      reason:
          'case:results.error_enums.benchmark_response.should_make_error_response',
    );

    final benchmarkPoint = DataPoint(x: 1.0, y: 2.0, timestamp: 123);
    final successResponse = createSuccessResponse(42, benchmarkPoint);
    expect(successResponse.requestId, 42);
    expect(
      successResponse.result is $$BoltResult$Ok,
      isTrue,
      reason:
          'case:results.error_enums.benchmark_response.should_make_success_response',
    );
    expect(successResponse.result.okValue(), benchmarkPoint);

    expect(
      isResponseSuccess(successResponse),
      isTrue,
      reason:
          'case:results.error_enums.benchmark_response.should_report_success_response',
    );
    expect(
      isResponseSuccess(errorResponse),
      isFalse,
      reason:
          'case:results.error_enums.benchmark_response.should_report_error_response',
    );

    expect(
      getResponseValue(successResponse),
      benchmarkPoint,
      reason:
          'case:results.error_enums.benchmark_response.should_return_value_for_success_response',
    );
    expect(
      getResponseValue(errorResponse),
      isNull,
      reason:
          'case:results.error_enums.benchmark_response.should_return_none_for_error_response',
    );
  });
}
