import Demo
import XCTest

final class NestedResultsTests: XCTestCase {
    func testNestedResultFns() throws {
        // case:results.nested_results.option_vec_string
        XCTAssertEqual(try resultOfOption(key: 4), 8)
        XCTAssertNil(try resultOfOption(key: 0))
        assertThrowsMessageContains("invalid key", try resultOfOption(key: -1))
        XCTAssertEqual(try resultOfVec(count: 3), [0, 1, 2])
        assertThrowsMessageContains("negative count", try resultOfVec(count: -1))
        XCTAssertEqual(try resultOfString(key: 7), "item_7")
        assertThrowsMessageContains("invalid key", try resultOfString(key: -1))
    }
}
