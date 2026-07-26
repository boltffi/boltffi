import Demo
import XCTest

final class CollectionsTests: DemoTestCase {
    func testHashMapFunctions() {
        XCTAssertEqual(makeHashMap(), ["first": 10, "second": 20], "case:collections.hash_map.should_return_values")

        let emptyValues: [String: [Int32]] = [:]
        XCTAssertEqual(echoHashMap(values: emptyValues), emptyValues, "case:collections.hash_map.should_roundtrip_empty")

        let nestedValues: [String: [Int32]] = ["first": [1, 2, 3], "empty": []]
        XCTAssertEqual(echoHashMap(values: nestedValues), nestedValues, "case:collections.hash_map.should_roundtrip_nested_values")
    }
}
