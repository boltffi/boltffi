import Demo
import XCTest

final class PrimitivesOptionsTests: XCTestCase {
    func testPrimitiveOptionFns() {
        // case:options.primitives.i32.some_none_and_helpers
        XCTAssertEqual(echoOptionalI32(v: 7), 7)
        XCTAssertNil(echoOptionalI32(v: nil))
        XCTAssertEqual(echoOptionalF64(v: 4.5), Optional(4.5))
        XCTAssertNil(echoOptionalF64(v: nil))
        XCTAssertEqual(echoOptionalBool(v: true), true)
        XCTAssertNil(echoOptionalBool(v: nil))
        XCTAssertEqual(unwrapOrDefaultI32(v: 9, fallback: 4), 9)
        XCTAssertEqual(unwrapOrDefaultI32(v: nil, fallback: 4), 4)
        XCTAssertEqual(makeSomeI32(v: 12), 12)
        XCTAssertNil(makeNoneI32())
        XCTAssertEqual(doubleIfSome(v: 8), 16)
        XCTAssertNil(doubleIfSome(v: nil))
        XCTAssertEqual(findEven(value: 8), 8)
        XCTAssertNil(findEven(value: 7))
        XCTAssertEqual(findPositiveI64(value: 9), 9)
        XCTAssertNil(findPositiveI64(value: -1))
        XCTAssertEqual(findPositiveF64(value: 3.5), 3.5)
        XCTAssertNil(findPositiveF64(value: -0.1))
    }
}
