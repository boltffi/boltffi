import Demo
import XCTest

final class StringsTests: XCTestCase {
    func testStringFns() {
        XCTAssertEqual(echoString(v: ""), "", "case:primitives.strings.echo.empty")
        XCTAssertEqual(echoString(v: "hello 🌍"), "hello 🌍", "case:primitives.strings.echo.emoji")
        XCTAssertEqual(concatStrings(a: "foo", b: "bar"), "foobar", "case:primitives.strings.concat.basic")
        XCTAssertEqual(stringLength(v: "café"), 5, "case:primitives.strings.length.utf8_bytes")
        XCTAssertEqual(stringIsEmpty(v: ""), true, "case:primitives.strings.is_empty.empty")
        XCTAssertEqual(repeatString(v: "ab", count: 3), "ababab", "case:primitives.strings.repeat.basic")
        XCTAssertEqual(generateString(size: 4), "xxxx")
    }
}
