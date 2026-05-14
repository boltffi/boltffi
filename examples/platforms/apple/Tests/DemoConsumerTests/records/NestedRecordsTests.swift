import Demo
import XCTest

final class NestedRecordsTests: XCTestCase {
    func testLineFns() {
        // case:records.nested.line.basic
        XCTAssertEqual(echoLine(l: Line(start: Point(x: 1, y: 2), end: Point(x: 5, y: 6))), Line(start: Point(x: 1, y: 2), end: Point(x: 5, y: 6)))
        XCTAssertEqual(makeLine(x1: 0, y1: 0, x2: 3, y2: 4), Line(start: Point(x: 0, y: 0), end: Point(x: 3, y: 4)))
        XCTAssertEqual(lineLength(l: Line(start: Point(x: 0, y: 0), end: Point(x: 3, y: 4))), 5.0, accuracy: 1e-6)
    }

    func testRectFns() {
        // case:records.nested.rect.basic
        XCTAssertEqual(echoRect(r: Rect(origin: Point(x: 1, y: 2), dimensions: Dimensions(width: 3, height: 4))), Rect(origin: Point(x: 1, y: 2), dimensions: Dimensions(width: 3, height: 4)))
        XCTAssertEqual(rectArea(r: Rect(origin: Point(x: 0, y: 0), dimensions: Dimensions(width: 3, height: 4))), 12.0, accuracy: 1e-9)
    }
}
