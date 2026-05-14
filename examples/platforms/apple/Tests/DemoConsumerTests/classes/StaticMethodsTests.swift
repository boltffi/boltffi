import Demo
import XCTest

final class StaticMethodsTests: DemoTestCase {
    func testMathUtilsInstanceAndStaticMethods() throws {
        let mathUtils = MathUtils(precision: 2)
        XCTAssertEqual(mathUtils.round(value: 3.14159), 3.14, accuracy: 1e-9)
        XCTAssertEqual(MathUtils.add(a: 4, b: 5), 9)
        XCTAssertEqual(MathUtils.clamp(value: 12.0, min: 0.0, max: 10.0), 10.0, accuracy: 1e-9)
        XCTAssertEqual(MathUtils.distanceBetween(a: Point(x: 0.0, y: 0.0), b: Point(x: 3.0, y: 4.0)), 5.0, accuracy: 1e-9)
        XCTAssertEqual(MathUtils.midpoint(a: Point(x: 1.0, y: 2.0), b: Point(x: 3.0, y: 4.0)), Point(x: 2.0, y: 3.0))
        XCTAssertEqual(try MathUtils.parseInt(input: "42"), 42)
        assertThrowsMessageContains("invalid digit found in string", try MathUtils.parseInt(input: "nope"))
        XCTAssertEqual(MathUtils.safeSqrt(value: 9.0), Optional(3.0))
        XCTAssertNil(MathUtils.safeSqrt(value: -1.0))
    }
}

