import Demo
import XCTest

final class ConstantsTests: DemoTestCase {
    func testAssociatedConstantsStayOnTheirOwnerTypes() {
        demoCase("case:constants.associated.should_expose_values_on_exported_types")
        XCTAssertEqual(DemoMode.preferred, .safe)
        XCTAssertEqual(DemoMode.fallback, .safe)
        XCTAssertEqual(DemoMode.variantCount, 2)
        XCTAssertEqual(DemoState.initial, .idle)
        XCTAssertEqual(Point.zero, Point(x: 0.0, y: 0.0))
        XCTAssertEqual(MathUtils.defaultPrecision, 2)
    }
}
