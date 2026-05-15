import Demo
import Foundation
import XCTest

final class BytesTests: DemoTestCase {
    func testBytesFns() {
        XCTAssertEqual(echoBytes(data: Data([1, 2, 3, 4])), Data([1, 2, 3, 4]), "case:bytes.bytes.should_roundtrip_values")
        XCTAssertEqual(bytesLength(data: Data([9, 8, 7])), 3, "case:bytes.bytes.should_report_length")
        XCTAssertEqual(bytesSum(data: Data([1, 2, 3, 4])), 10, "case:bytes.bytes.should_sum_values")
        XCTAssertEqual(makeBytes(len: 4), Data([0, 1, 2, 3]), "case:bytes.bytes.should_make_sequential_values")
        XCTAssertEqual(reverseBytes(data: Data([1, 2, 3, 4])), Data([4, 3, 2, 1]), "case:bytes.bytes.should_reverse_values")
        XCTAssertEqual(generateBytes(size: 4), Data([42, 42, 42, 42]))
    }
}
