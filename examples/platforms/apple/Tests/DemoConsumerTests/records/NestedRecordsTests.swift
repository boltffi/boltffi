import Demo
import XCTest

final class NestedRecordsTests: DemoTestCase {
    func testLineFns() {
        demoCase("case:records.nested.line.should_roundtrip_nested_points")
        XCTAssertEqual(echoLine(l: Line(start: Point(x: 1, y: 2), end: Point(x: 5, y: 6))), Line(start: Point(x: 1, y: 2), end: Point(x: 5, y: 6)))
        demoCase("case:records.nested.line.should_make_from_coordinates")
        XCTAssertEqual(makeLine(x1: 0, y1: 0, x2: 3, y2: 4), Line(start: Point(x: 0, y: 0), end: Point(x: 3, y: 4)))
        demoCase("case:records.nested.line.should_compute_length")
        XCTAssertEqual(lineLength(l: Line(start: Point(x: 0, y: 0), end: Point(x: 3, y: 4))), 5.0, accuracy: 1e-6)
    }

    func testRectFns() {
        demoCase("case:records.nested.rect.should_roundtrip_nested_records")
        XCTAssertEqual(echoRect(r: Rect(origin: Point(x: 1, y: 2), dimensions: Dimensions(width: 3, height: 4))), Rect(origin: Point(x: 1, y: 2), dimensions: Dimensions(width: 3, height: 4)))
        demoCase("case:records.nested.rect.should_compute_area")
        XCTAssertEqual(rectArea(r: Rect(origin: Point(x: 0, y: 0), dimensions: Dimensions(width: 3, height: 4))), 12.0, accuracy: 1e-9)
    }
}
