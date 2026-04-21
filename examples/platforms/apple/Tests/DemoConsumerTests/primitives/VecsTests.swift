import Demo
import Foundation
import XCTest

final class VecsTests: XCTestCase {
    func testVecFns() {
        XCTAssertEqual(echoVecI32(v: [1, 2, 3]), [1, 2, 3])
        XCTAssertEqual(echoVecI8(v: [-1, 0, 7]), [-1, 0, 7])
        XCTAssertEqual(echoVecU8(v: Data([0, 1, 2, 3])), Data([0, 1, 2, 3]))
        XCTAssertEqual(echoVecI16(v: [-3, 0, 9]), [-3, 0, 9])
        XCTAssertEqual(echoVecU16(v: [0, 10, 20]), [0, 10, 20])
        XCTAssertEqual(echoVecU32(v: [0, 10, 20]), [0, 10, 20])
        XCTAssertEqual(echoVecI64(v: [-5, 0, 8]), [-5, 0, 8])
        XCTAssertEqual(echoVecU64(v: [0, 1, 2]), [0, 1, 2])
        XCTAssertEqual(echoVecIsize(v: [-2, 0, 5]), [-2, 0, 5])
        XCTAssertEqual(echoVecUsize(v: [0, 2, 4]), [0, 2, 4])
        XCTAssertEqual(echoVecF32(v: [1.25, -2.5]), [1.25, -2.5])
        XCTAssertEqual(echoVecF64(v: [1.5, 2.5]), [1.5, 2.5])
        XCTAssertEqual(echoVecBool(v: [true, false, true]), [true, false, true])
        XCTAssertEqual(echoVecString(v: ["hello", "world"]), ["hello", "world"])
        XCTAssertEqual(vecStringLengths(v: ["hi", "café"]), [2, 5])
        XCTAssertEqual(sumVecI32(v: [10, 20, 30]), 60)
        XCTAssertEqual(makeRange(start: 0, end: 5), [0, 1, 2, 3, 4])
        XCTAssertEqual(reverseVecI32(v: [1, 2, 3]), [3, 2, 1])
        XCTAssertEqual(generateI32Vec(count: 4), [0, 1, 2, 3])
        XCTAssertEqual(sumI32Vec(values: [1, 2, 3]), 6)
        XCTAssertEqual(generateF64Vec(count: 3).count, 3)
        XCTAssertEqual(sumF64Vec(values: [0.5, 1.5, 2.0]), 4.0, accuracy: 1e-9)
        var incrementedValues: [UInt64] = [1, 2]
        incU64(values: &incrementedValues)
        XCTAssertEqual(incrementedValues, [2, 2])
        XCTAssertEqual(incU64Value(value: 9), 10)
    }

    func testNestedVecFns() {
        XCTAssertEqual(echoVecVecI32(v: [[1, 2, 3], [], [4, 5]]), [[1, 2, 3], [], [4, 5]])
        XCTAssertEqual(echoVecVecI32(v: []), [])
        XCTAssertEqual(echoVecVecBool(v: [[true, false, true], [], [false]]), [[true, false, true], [], [false]])
        XCTAssertEqual(echoVecVecIsize(v: [[-2, 0, 5], [], [9]]), [[-2, 0, 5], [], [9]])
        XCTAssertEqual(echoVecVecUsize(v: [[0, 2, 4], [], [8]]), [[0, 2, 4], [], [8]])

        let strings = [["hello", "world"], [], ["café", "🌍"]]
        XCTAssertEqual(echoVecVecString(v: strings), strings)

        XCTAssertEqual(flattenVecVecI32(v: [[1, 2], [3], [], [4, 5]]), [1, 2, 3, 4, 5])
        XCTAssertEqual(flattenVecVecI32(v: []), [])
    }
}
