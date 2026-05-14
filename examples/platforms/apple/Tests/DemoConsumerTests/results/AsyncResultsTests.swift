import Demo
import XCTest

final class AsyncResultsTests: XCTestCase {
    func testAsyncSafeDivide() async throws {
        // case:results.async_results.math_fetch_find
        let quotient = try await asyncSafeDivide(a: 10, b: 2)
        XCTAssertEqual(quotient, 5)

        do {
            _ = try await asyncSafeDivide(a: 1, b: 0)
            XCTFail("expected asyncSafeDivide to throw")
        } catch {
            XCTAssertEqual(error as? MathError, .divisionByZero)
        }
    }

    func testAsyncFallibleFetch() async throws {
        let fetchedValue = try await asyncFallibleFetch(key: 7)
        XCTAssertEqual(fetchedValue, "value_7")
        await assertAsyncThrowsMessageContains("invalid key") {
            try await asyncFallibleFetch(key: -1)
        }
    }

    func testAsyncFindValue() async throws {
        let presentValue = try await asyncFindValue(key: 4)
        XCTAssertEqual(presentValue, 40)
        let missingValue = try await asyncFindValue(key: 0)
        XCTAssertNil(missingValue)
        await assertAsyncThrowsMessageContains("invalid key") {
            try await asyncFindValue(key: -1)
        }
    }
}
