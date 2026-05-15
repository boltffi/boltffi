import Demo
import XCTest

final class AsyncResultsTests: DemoTestCase {
    func testAsyncSafeDivide() async throws {
        demoCase("case:results.async_results.safe_divide.should_return_quotient")
        let quotient = try await asyncSafeDivide(a: 10, b: 2)
        XCTAssertEqual(quotient, 5)

        demoCase("case:results.async_results.safe_divide.should_reject_division_by_zero")
        do {
            _ = try await asyncSafeDivide(a: 1, b: 0)
            XCTFail("expected asyncSafeDivide to throw")
        } catch {
            XCTAssertEqual(error as? MathError, .divisionByZero)
        }
    }

    func testAsyncFallibleFetch() async throws {
        demoCase("case:results.async_results.fallible_fetch.should_return_value_for_non_negative_key")
        let fetchedValue = try await asyncFallibleFetch(key: 7)
        XCTAssertEqual(fetchedValue, "value_7")
        demoCase("case:results.async_results.fallible_fetch.should_reject_negative_key")
        await assertAsyncThrowsMessageContains("invalid key") {
            try await asyncFallibleFetch(key: -1)
        }
    }

    func testAsyncFindValue() async throws {
        demoCase("case:results.async_results.find_value.should_return_some_for_positive_key")
        let presentValue = try await asyncFindValue(key: 4)
        XCTAssertEqual(presentValue, 40)
        demoCase("case:results.async_results.find_value.should_return_none_for_zero_key")
        let missingValue = try await asyncFindValue(key: 0)
        XCTAssertNil(missingValue)
        demoCase("case:results.async_results.find_value.should_reject_negative_key")
        await assertAsyncThrowsMessageContains("invalid key") {
            try await asyncFindValue(key: -1)
        }
    }
}
