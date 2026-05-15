import Demo
import XCTest

final class StringsTests: DemoTestCase {
    func testStringFns() {
        XCTAssertEqual(echoString(v: ""), "", "case:primitives.strings.string.should_roundtrip_empty")
        XCTAssertEqual(echoString(v: "hello 🌍"), "hello 🌍", "case:primitives.strings.string.should_roundtrip_emoji")
        XCTAssertEqual(concatStrings(a: "foo", b: "bar"), "foobar", "case:primitives.strings.string.should_concatenate_values")
        XCTAssertEqual(stringLength(v: "café"), 5, "case:primitives.strings.string.should_report_utf8_byte_length")
        XCTAssertEqual(stringIsEmpty(v: ""), true, "case:primitives.strings.string.should_detect_empty")
        XCTAssertEqual(repeatString(v: "ab", count: 3), "ababab", "case:primitives.strings.string.should_repeat_value")
        XCTAssertEqual(generateString(size: 4), "xxxx")
    }
}
