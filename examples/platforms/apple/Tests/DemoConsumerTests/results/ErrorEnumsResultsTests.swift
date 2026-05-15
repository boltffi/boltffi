import Demo
import XCTest

final class ErrorEnumsResultsTests: DemoTestCase {
    func testTypedErrorResultFns() throws {
        demoCase("case:results.error_enums.checked_divide.should_return_quotient")
        XCTAssertEqual(try checkedDivide(a: 10, b: 2), 5)
        demoCase("case:results.error_enums.checked_sqrt.should_return_square_root")
        XCTAssertEqual(try checkedSqrt(x: 9.0), 3.0, accuracy: 1e-9)
        demoCase("case:results.error_enums.checked_add.should_return_sum")
        XCTAssertEqual(try checkedAdd(a: 2, b: 3), 5)
        demoCase("case:results.error_enums.checked_divide.should_reject_division_by_zero")
        XCTAssertThrowsError(try checkedDivide(a: 1, b: 0)) { error in
            XCTAssertEqual(error as? MathError, MathError.divisionByZero)
        }
        demoCase("case:results.error_enums.checked_sqrt.should_reject_negative_input")
        XCTAssertThrowsError(try checkedSqrt(x: -1.0)) { error in
            XCTAssertEqual(error as? MathError, MathError.negativeInput)
        }
        demoCase("case:results.error_enums.checked_add.should_reject_overflow")
        XCTAssertThrowsError(try checkedAdd(a: .max, b: 1)) { error in
            XCTAssertEqual(error as? MathError, MathError.overflow)
        }

        demoCase("case:results.error_enums.validate_username.should_accept_valid_name")
        XCTAssertEqual(try validateUsername(name: "valid_name"), "valid_name")
        demoCase("case:results.error_enums.validate_username.should_reject_too_short_name")
        XCTAssertThrowsError(try validateUsername(name: "ab")) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.tooShort)
        }
        demoCase("case:results.error_enums.validate_username.should_reject_too_long_name")
        XCTAssertThrowsError(try validateUsername(name: String(repeating: "a", count: 21))) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.tooLong)
        }
        demoCase("case:results.error_enums.validate_username.should_reject_invalid_format")
        XCTAssertThrowsError(try validateUsername(name: "has space")) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.invalidFormat)
        }

        demoCase("case:results.error_enums.may_fail.should_return_success_when_valid")
        XCTAssertEqual(try mayFail(valid: true), "Success!")
        demoCase("case:results.error_enums.may_fail.should_return_app_error_when_invalid")
        XCTAssertThrowsError(try mayFail(valid: false)) { error in
            XCTAssertEqual(
                error as? AppError,
                AppError(code: 400, message: "Invalid input")
            )
        }

        demoCase("case:results.error_enums.divide_app.should_return_quotient")
        XCTAssertEqual(try divideApp(a: 10, b: 2), 5)
        demoCase("case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero")
        XCTAssertThrowsError(try divideApp(a: 10, b: 0)) { error in
            XCTAssertEqual(
                error as? AppError,
                AppError(code: 500, message: "Division by zero")
            )
        }

        demoCase("case:results.error_enums.process_value.should_return_success_variant")
        XCTAssertEqual(processValue(value: 3), .success)
        demoCase("case:results.error_enums.process_value.should_return_error_code_variant")
        XCTAssertEqual(processValue(value: 0), .errorCode(-1))
        demoCase("case:results.error_enums.process_value.should_return_error_with_data_variant")
        XCTAssertEqual(processValue(value: -2), .errorWithData(code: -2, detail: -4))
        demoCase("case:results.error_enums.api_result_is_success.should_report_success_variant")
        XCTAssertTrue(apiResultIsSuccess(result: .success))
        demoCase("case:results.error_enums.api_result_is_success.should_report_error_variant")
        XCTAssertFalse(apiResultIsSuccess(result: .errorCode(-1)))

        demoCase("case:results.error_enums.try_compute.should_return_doubled_value")
        XCTAssertEqual(try tryCompute(value: 3), 6)
        demoCase("case:results.error_enums.try_compute.should_return_overflow_error")
        XCTAssertThrowsError(try tryCompute(value: -1)) { error in
            XCTAssertEqual(error as? ComputeError, .overflow(value: -1, limit: 0))
        }

        let point = DataPoint(x: 1, y: 2, timestamp: 3)
        demoCase("case:results.error_enums.benchmark_response.should_make_success_response")
        let okResponse = createSuccessResponse(requestId: 7, point: point)
        XCTAssertEqual(
            okResponse,
            BenchmarkResponse(requestId: 7, result: .success(point))
        )

        demoCase("case:results.error_enums.benchmark_response.should_make_error_response")
        let errorResponse = createErrorResponse(requestId: 8, error: .invalidInput(-9))
        XCTAssertEqual(
            errorResponse,
            BenchmarkResponse(requestId: 8, result: .failure(.invalidInput(-9)))
        )

        demoCase("case:results.error_enums.benchmark_response.should_report_success_response")
        XCTAssertTrue(isResponseSuccess(response: okResponse))
        demoCase("case:results.error_enums.benchmark_response.should_report_error_response")
        XCTAssertFalse(isResponseSuccess(response: errorResponse))
        demoCase("case:results.error_enums.benchmark_response.should_return_value_for_success_response")
        XCTAssertEqual(getResponseValue(response: okResponse), point)
        demoCase("case:results.error_enums.benchmark_response.should_return_none_for_error_response")
        XCTAssertNil(getResponseValue(response: errorResponse))
    }
}
