import Demo
import XCTest

final class ErrorEnumsResultsTests: XCTestCase {
    func testTypedErrorResultFns() throws {
        // case:results.error_enums.math.checked
        // case:results.error_enums.validation.username
        XCTAssertEqual(try checkedDivide(a: 10, b: 2), 5)
        XCTAssertEqual(try checkedSqrt(x: 9.0), 3.0, accuracy: 1e-9)
        XCTAssertEqual(try checkedAdd(a: 2, b: 3), 5)
        XCTAssertEqual(try validateUsername(name: "valid_name"), "valid_name")

        XCTAssertThrowsError(try checkedDivide(a: 1, b: 0)) { error in
            XCTAssertEqual(error as? MathError, MathError.divisionByZero)
        }
        XCTAssertThrowsError(try checkedSqrt(x: -1.0)) { error in
            XCTAssertEqual(error as? MathError, MathError.negativeInput)
        }
        XCTAssertThrowsError(try checkedAdd(a: .max, b: 1)) { error in
            XCTAssertEqual(error as? MathError, MathError.overflow)
        }
        XCTAssertThrowsError(try validateUsername(name: "ab")) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.tooShort)
        }
        XCTAssertThrowsError(try validateUsername(name: String(repeating: "a", count: 21))) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.tooLong)
        }
        XCTAssertThrowsError(try validateUsername(name: "has space")) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.invalidFormat)
        }

        // case:results.error_enums.app_error.basic
        XCTAssertEqual(try mayFail(valid: true), "Success!")
        XCTAssertThrowsError(try mayFail(valid: false)) { error in
            XCTAssertEqual(
                error as? AppError,
                AppError(code: 400, message: "Invalid input")
            )
        }

        XCTAssertEqual(try divideApp(a: 10, b: 2), 5)
        XCTAssertThrowsError(try divideApp(a: 10, b: 0)) { error in
            XCTAssertEqual(
                error as? AppError,
                AppError(code: 500, message: "Division by zero")
            )
        }

        // case:results.error_enums.api_result.basic
        XCTAssertEqual(processValue(value: 3), .success)
        XCTAssertEqual(processValue(value: 0), .errorCode(-1))
        XCTAssertEqual(processValue(value: -2), .errorWithData(code: -2, detail: -4))
        XCTAssertTrue(apiResultIsSuccess(result: .success))
        XCTAssertFalse(apiResultIsSuccess(result: .errorCode(-1)))

        XCTAssertEqual(try tryCompute(value: 3), 6)
        XCTAssertThrowsError(try tryCompute(value: -1)) { error in
            XCTAssertEqual(error as? ComputeError, .overflow(value: -1, limit: 0))
        }

        // case:results.error_enums.benchmark_response.basic
        let point = DataPoint(x: 1, y: 2, timestamp: 3)
        let okResponse = createSuccessResponse(requestId: 7, point: point)
        XCTAssertEqual(
            okResponse,
            BenchmarkResponse(requestId: 7, result: .success(point))
        )

        let errorResponse = createErrorResponse(requestId: 8, error: .invalidInput(-9))
        XCTAssertEqual(
            errorResponse,
            BenchmarkResponse(requestId: 8, result: .failure(.invalidInput(-9)))
        )

        XCTAssertTrue(isResponseSuccess(response: okResponse))
        XCTAssertFalse(isResponseSuccess(response: errorResponse))
        XCTAssertEqual(getResponseValue(response: okResponse), point)
        XCTAssertNil(getResponseValue(response: errorResponse))
    }
}
