import Demo
import XCTest

final class NestedResultsTests: DemoTestCase {
    func testNestedResultFns() throws {
        demoCase("case:results.nested_results.option.should_return_some_for_positive_key")
        XCTAssertEqual(try resultOfOption(key: 4), 8)
        demoCase("case:results.nested_results.option.should_return_none_for_zero_key")
        XCTAssertNil(try resultOfOption(key: 0))
        demoCase("case:results.nested_results.option.should_reject_negative_key")
        assertThrowsMessageContains("invalid key", try resultOfOption(key: -1))
        demoCase("case:results.nested_results.vec.should_return_values_for_non_negative_count")
        XCTAssertEqual(try resultOfVec(count: 3), [0, 1, 2])
        demoCase("case:results.nested_results.vec.should_reject_negative_count")
        assertThrowsMessageContains("negative count", try resultOfVec(count: -1))
        demoCase("case:results.nested_results.string.should_return_value_for_non_negative_key")
        XCTAssertEqual(try resultOfString(key: 7), "item_7")
        demoCase("case:results.nested_results.string.should_reject_negative_key")
        assertThrowsMessageContains("invalid key", try resultOfString(key: -1))
    }
}
