import Demo
import Foundation
import XCTest

final class VecsTests: DemoTestCase {
    func testVecFns() {
        XCTAssertEqual(echoVecI32(v: [1, 2, 3]), [1, 2, 3], "case:primitives.vecs.i32.should_roundtrip_non_empty")
        XCTAssertEqual(echoVecI32(v: []), [], "case:primitives.vecs.i32.should_roundtrip_empty")
        XCTAssertEqual(echoVecI8(v: [-1, 0, 7]), [-1, 0, 7], "case:primitives.vecs.i8.should_roundtrip_values")
        XCTAssertEqual(echoVecU8(v: Data([0, 1, 2, 3])), Data([0, 1, 2, 3]), "case:primitives.vecs.u8.should_roundtrip_values")
        XCTAssertEqual(echoVecI16(v: [-3, 0, 9]), [-3, 0, 9], "case:primitives.vecs.i16.should_roundtrip_values")
        XCTAssertEqual(echoVecU16(v: [0, 10, 20]), [0, 10, 20], "case:primitives.vecs.u16.should_roundtrip_values")
        XCTAssertEqual(echoVecU32(v: [0, 10, 20]), [0, 10, 20], "case:primitives.vecs.u32.should_roundtrip_values")
        XCTAssertEqual(echoVecI64(v: [-5, 0, 8]), [-5, 0, 8], "case:primitives.vecs.i64.should_roundtrip_values")
        XCTAssertEqual(echoVecU64(v: [0, 1, 2]), [0, 1, 2], "case:primitives.vecs.u64.should_roundtrip_values")
        XCTAssertEqual(echoVecIsize(v: [-2, 0, 5]), [-2, 0, 5], "case:primitives.vecs.isize.should_roundtrip_values")
        XCTAssertEqual(echoVecUsize(v: [0, 2, 4]), [0, 2, 4], "case:primitives.vecs.usize.should_roundtrip_values")
        XCTAssertEqual(echoVecF32(v: [1.25, -2.5]), [1.25, -2.5], "case:primitives.vecs.f32.should_roundtrip_values_with_tolerance")
        XCTAssertEqual(echoVecF64(v: [1.5, 2.5]), [1.5, 2.5], "case:primitives.vecs.f64.should_roundtrip_values")
        XCTAssertEqual(echoVecBool(v: [true, false, true]), [true, false, true], "case:primitives.vecs.bool.should_roundtrip_values")
        XCTAssertEqual(echoVecString(v: ["hello", "world"]), ["hello", "world"], "case:primitives.vecs.echo_string.basic")
        XCTAssertEqual(vecStringLengths(v: ["hi", "café"]), [2, 5], "case:primitives.vecs.string_lengths.utf8")
        XCTAssertEqual(sumVecI32(v: [10, 20, 30]), 60, "case:primitives.vecs.i32.should_sum_values")
        XCTAssertEqual(makeRange(start: 0, end: 5), [0, 1, 2, 3, 4], "case:primitives.vecs.i32.should_make_range")
        XCTAssertEqual(reverseVecI32(v: [1, 2, 3]), [3, 2, 1], "case:primitives.vecs.i32.should_reverse_values")
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
