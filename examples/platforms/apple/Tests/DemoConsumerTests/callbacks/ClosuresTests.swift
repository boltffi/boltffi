import Demo
import XCTest

final class ClosuresTests: DemoTestCase {
    func testClosureFns() {
        var observedValue: Int32?

        XCTAssertEqual(applyClosure(f: { $0 * 2 }, value: 5), 10)
        applyVoidClosure(f: { observedValue = $0 }, value: 42)
        XCTAssertEqual(observedValue, 42)
        XCTAssertEqual(applyNullaryClosure(f: { 99 }), 99)
        XCTAssertEqual(applyStringClosure(f: { $0.uppercased() }, s: "hello"), "HELLO")
        XCTAssertEqual(applyBoolClosure(f: { !$0 }, v: true), false)
        XCTAssertEqual(applyF64Closure(f: { $0 * $0 }, v: 3.0), 9.0, accuracy: 1e-9)
        XCTAssertEqual(mapVecWithClosure(f: { $0 * 2 }, values: [1, 2, 3]), [2, 4, 6])
        XCTAssertEqual(filterVecWithClosure(f: { $0 % 2 == 0 }, values: [1, 2, 3, 4]), [2, 4])
        XCTAssertEqual(applyBinaryClosure(f: +, a: 3, b: 4), 7)
        XCTAssertEqual(applyOffsetClosure(f: { value, delta in value + Int(delta) }, value: -5, delta: 8), 3)
        XCTAssertEqual(applyStatusClosure(f: { $0 == .active ? .pending : .active }, status: .active), .pending)
        XCTAssertEqual(applyPointClosure(f: { Point(x: $0.x + 1.0, y: $0.y + 1.0) }, p: Point(x: 1.0, y: 2.0)), Point(x: 2.0, y: 3.0))
        XCTAssertEqual(
            applyOptionalPointClosure(
                f: { point in point.map { Point(x: $0.x + 2.0, y: $0.y + 3.0) } },
                point: Point(x: 1.0, y: 2.0)
            ),
            Point(x: 3.0, y: 5.0)
        )
        XCTAssertNil(applyOptionalPointClosure(f: { $0 }, point: nil))
        XCTAssertEqual(try applyResultClosure(f: { .success($0 * 4) }, value: 6), 24)
        XCTAssertThrowsError(try applyResultClosure(f: { _ in .failure(.negativeInput) }, value: -1)) { error in
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
    }
}
