import Demo
import XCTest

final class BasicResultsTests: XCTestCase {
    func testBasicResultFns() throws {
        // case:results.basic.safe_divide.ok_err
        XCTAssertEqual(try safeDivide(a: 10, b: 2), 5)
        assertThrowsMessageContains("division by zero", try safeDivide(a: 1, b: 0))
        // case:results.basic.safe_sqrt.ok_err
        XCTAssertEqual(try safeSqrt(x: 9.0), 3.0, accuracy: 1e-9)
        assertThrowsMessageContains("negative input", try safeSqrt(x: -1.0))
        // case:results.basic.parse_point.ok_err
        XCTAssertEqual(try parsePoint(s: "1.5, 2.5"), Point(x: 1.5, y: 2.5))
        assertThrowsMessageContains("expected format", try parsePoint(s: "wat"))
        // case:results.basic.always_ok_err
        XCTAssertEqual(try alwaysOk(v: 21), 42)
        assertThrowsMessageContains("boom", try alwaysErr(msg: "boom"))
        // case:results.basic.result_to_string.ok_err
        XCTAssertEqual(resultToString(v: .success(7)), "ok: 7")
        XCTAssertEqual(resultToString(v: .failure(FfiError(message: "bad"))), "err: bad")
        XCTAssertEqual(try divide(a: 10, b: 2), 5)
        assertThrowsMessageContains("division by zero", try divide(a: 10, b: 0))
        XCTAssertEqual(try parseInt(input: "42"), 42)
        assertThrowsMessageContains("invalid integer", try parseInt(input: "nope"))
        XCTAssertEqual(try validateName(name: "Ali"), "Hello, Ali!")
        assertThrowsMessageContains("name cannot be empty", try validateName(name: ""))
    }
}
