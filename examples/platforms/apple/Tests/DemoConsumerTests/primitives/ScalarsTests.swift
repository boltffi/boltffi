import Demo
import XCTest

final class ScalarsTests: DemoTestCase {
    func testScalarFns() {
        XCTAssertEqual(echoBool(v: true), true, "case:primitives.scalars.bool.should_roundtrip_true")
        XCTAssertEqual(negateBool(v: false), true, "case:primitives.scalars.bool.should_negate_false_to_true")
        XCTAssertEqual(echoI8(v: -7), -7, "case:primitives.scalars.i8.should_roundtrip_negative_value")
        XCTAssertEqual(echoU8(v: 255), 255, "case:primitives.scalars.u8.should_roundtrip_max_value")
        XCTAssertEqual(echoI16(v: -1234), -1234, "case:primitives.scalars.i16.should_roundtrip_negative_value")
        XCTAssertEqual(echoU16(v: 55_000), 55_000, "case:primitives.scalars.u16.should_roundtrip_large_value")
        XCTAssertEqual(echoI32(v: -42), -42, "case:primitives.scalars.i32.should_roundtrip_negative_value")
        XCTAssertEqual(addI32(a: 10, b: 20), 30, "case:primitives.scalars.i32.should_add_two_values")
        XCTAssertEqual(echoU32(v: 4_000_000_000), 4_000_000_000, "case:primitives.scalars.u32.should_roundtrip_large_value")
        XCTAssertEqual(echoI64(v: -9_999_999_999), -9_999_999_999, "case:primitives.scalars.i64.should_roundtrip_large_negative_value")
        XCTAssertEqual(echoU64(v: 9_999_999_999), 9_999_999_999, "case:primitives.scalars.u64.should_roundtrip_large_value")
        XCTAssertEqual(echoF32(v: 3.5), 3.5, accuracy: 1e-6, "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance")
        XCTAssertEqual(addF32(a: 1.5, b: 2.5), 4.0, accuracy: 1e-6, "case:primitives.scalars.f32.should_add_two_values_with_tolerance")
        XCTAssertEqual(echoF64(v: 3.14159265359), 3.14159265359, accuracy: 1e-9, "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance")
        XCTAssertEqual(addF64(a: 1.5, b: 2.5), 4.0, accuracy: 1e-9, "case:primitives.scalars.f64.should_add_two_values_with_tolerance")
        XCTAssertEqual(echoUsize(v: 123), 123, "case:primitives.scalars.usize.should_roundtrip_value")
        XCTAssertEqual(echoIsize(v: -123), -123, "case:primitives.scalars.isize.should_roundtrip_negative_value")
        demoCase("case:primitives.scalars.noop.should_cross_without_values")
        noop()
        XCTAssertEqual(Demo.add(a: 10, b: 20), 30, "case:primitives.scalars.i32.should_add_with_benchmark_alias")
        XCTAssertEqual(multiply(a: 1.5, b: 2.0), 3.0, accuracy: 1e-9, "case:primitives.scalars.f64.should_multiply_two_values")
    }
}
