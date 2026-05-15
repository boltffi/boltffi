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
        XCTAssertEqual(echoVecString(v: ["hello", "world"]), ["hello", "world"], "case:primitives.vecs.string.should_roundtrip_values")
        XCTAssertEqual(vecStringLengths(v: ["hi", "café"]), [2, 5], "case:primitives.vecs.string.should_report_utf8_byte_lengths")
        XCTAssertEqual(sumVecI32(v: [10, 20, 30]), 60, "case:primitives.vecs.i32.should_sum_values")
        XCTAssertEqual(makeRange(start: 0, end: 5), [0, 1, 2, 3, 4], "case:primitives.vecs.i32.should_make_range")
        XCTAssertEqual(reverseVecI32(v: [1, 2, 3]), [3, 2, 1], "case:primitives.vecs.i32.should_reverse_values")
        XCTAssertEqual(generateI32Vec(count: 4), [0, 1, 2, 3], "case:primitives.vecs.i32.should_generate_sequence")
        XCTAssertEqual(sumI32Vec(values: [1, 2, 3]), 6, "case:primitives.vecs.i32.should_sum_benchmark_values")
        XCTAssertEqual(generateF64Vec(count: 3).count, 3, "case:primitives.vecs.f64.should_generate_sequence")
        XCTAssertEqual(sumF64Vec(values: [0.5, 1.5, 2.0]), 4.0, accuracy: 1e-9, "case:primitives.vecs.f64.should_sum_values")
        var incrementedValues: [UInt64] = [1, 2]
        incU64(values: &incrementedValues)
        XCTAssertEqual(incrementedValues, [2, 2], "case:primitives.vecs.u64.should_increment_first_value_in_place")
        XCTAssertEqual(incU64Value(value: 9), 10, "case:primitives.vecs.u64.should_increment_value")
    }

    func testNestedVecFns() {
        XCTAssertEqual(echoVecVecI32(v: [[1, 2, 3], [], [4, 5]]), [[1, 2, 3], [], [4, 5]], "case:primitives.vecs.nested_i32.should_roundtrip_values")
        XCTAssertEqual(echoVecVecI32(v: []), [], "case:primitives.vecs.nested_i32.should_roundtrip_empty_outer")
        XCTAssertEqual(echoVecVecBool(v: [[true, false, true], [], [false]]), [[true, false, true], [], [false]], "case:primitives.vecs.nested_bool.should_roundtrip_values")
        XCTAssertEqual(echoVecVecIsize(v: [[-2, 0, 5], [], [9]]), [[-2, 0, 5], [], [9]], "case:primitives.vecs.nested_isize.should_roundtrip_values")
        XCTAssertEqual(echoVecVecUsize(v: [[0, 2, 4], [], [8]]), [[0, 2, 4], [], [8]], "case:primitives.vecs.nested_usize.should_roundtrip_values")

        let strings = [["hello", "world"], [], ["café", "🌍"]]
        XCTAssertEqual(echoVecVecString(v: strings), strings, "case:primitives.vecs.nested_string.should_roundtrip_utf8_values")

        XCTAssertEqual(flattenVecVecI32(v: [[1, 2], [3], [], [4, 5]]), [1, 2, 3, 4, 5], "case:primitives.vecs.nested_i32.should_flatten_values")
        XCTAssertEqual(flattenVecVecI32(v: []), [], "case:primitives.vecs.nested_i32.should_flatten_empty")
    }
}
