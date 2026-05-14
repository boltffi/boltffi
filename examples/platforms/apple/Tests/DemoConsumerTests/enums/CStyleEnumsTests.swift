import Demo
import XCTest

final class CStyleEnumsTests: DemoTestCase {
    func testStatusFns() {
        demoCase("case:enums.c_style.status.basic")
        XCTAssertEqual(echoStatus(s: .active), .active)
        XCTAssertEqual(statusToString(s: .active), "active")
        XCTAssertEqual(isActive(s: .pending), false)

        demoCase("case:enums.c_style.status.vec")
        XCTAssertEqual(echoVecStatus(values: [.active, .pending]), [.active, .pending])
    }

    func testDirectionFns() {
        demoCase("case:enums.c_style.direction.basic")
        XCTAssertEqual(Direction(raw: 3), .west)
        XCTAssertEqual(Direction.cardinal(), .north)
        XCTAssertEqual(Direction(fromDegrees: 90.0), .east)
        XCTAssertEqual(Direction.count(), 4)
        XCTAssertEqual(Direction.north.opposite(), .south)
        XCTAssertEqual(Direction.east.isHorizontal(), true)
        XCTAssertEqual(Direction.west.label(), "W")
        XCTAssertEqual(echoDirection(d: .east), .east)
        XCTAssertEqual(oppositeDirection(d: .east), .west)
        XCTAssertEqual(directionToDegrees(direction: .west), 270)
        XCTAssertEqual(generateDirections(count: 5), [.north, .east, .south, .west, .north])
        XCTAssertEqual(countNorth(directions: [.north, .east, .north]), 2)
        XCTAssertEqual(findDirection(id: 2), .south)
        XCTAssertNil(findDirection(id: 9))
        XCTAssertEqual(findDirections(count: 3), [.north, .east, .south])
        XCTAssertNil(findDirections(count: 0))
    }
}
