import Demo
import XCTest

final class CStyleEnumsTests: DemoTestCase {
    func testStatusFns() {
        demoCase("case:enums.c_style.status.should_roundtrip_values")
        XCTAssertEqual(echoStatus(s: .active), .active)
        demoCase("case:enums.c_style.status.should_render_labels")
        XCTAssertEqual(statusToString(s: .active), "active")
        demoCase("case:enums.c_style.status.should_identify_active_values")
        XCTAssertEqual(isActive(s: .pending), false)

        demoCase("case:enums.c_style.status.should_roundtrip_vectors")
        XCTAssertEqual(echoVecStatus(values: [.active, .pending]), [.active, .pending])
    }

    func testDirectionFns() {
        demoCase("case:enums.c_style.direction.should_construct_from_raw_value")
        XCTAssertEqual(Direction(raw: 3), .west)
        demoCase("case:enums.c_style.direction.should_return_cardinal_value")
        XCTAssertEqual(Direction.cardinal(), .north)
        demoCase("case:enums.c_style.direction.should_construct_from_degrees")
        XCTAssertEqual(Direction(fromDegrees: 90.0), .east)
        demoCase("case:enums.c_style.direction.should_report_variant_count")
        XCTAssertEqual(Direction.count(), 4)
        demoCase("case:enums.c_style.direction.should_return_opposite_from_method")
        XCTAssertEqual(Direction.north.opposite(), .south)
        demoCase("case:enums.c_style.direction.should_identify_horizontal_values")
        XCTAssertEqual(Direction.east.isHorizontal(), true)
        demoCase("case:enums.c_style.direction.should_render_compass_label")
        XCTAssertEqual(Direction.west.label(), "W")
        demoCase("case:enums.c_style.direction.should_roundtrip_value")
        XCTAssertEqual(echoDirection(d: .east), .east)
        demoCase("case:enums.c_style.direction.should_return_opposite_from_free_function")
        XCTAssertEqual(oppositeDirection(d: .east), .west)
        demoCase("case:enums.c_style.direction.should_return_degrees")
        XCTAssertEqual(directionToDegrees(direction: .west), 270)
        demoCase("case:enums.c_style.direction.should_generate_sequence")
        XCTAssertEqual(generateDirections(count: 5), [.north, .east, .south, .west, .north])
        demoCase("case:enums.c_style.direction.should_count_north_values")
        XCTAssertEqual(countNorth(directions: [.north, .east, .north]), 2)
        demoCase("case:enums.c_style.direction.find_direction.should_return_some_for_known_id")
        XCTAssertEqual(findDirection(id: 2), .south)
        demoCase("case:enums.c_style.direction.find_direction.should_return_none_for_unknown_id")
        XCTAssertNil(findDirection(id: 9))
        demoCase("case:enums.c_style.direction.find_directions.should_return_sequence_for_positive_count")
        XCTAssertEqual(findDirections(count: 3), [.north, .east, .south])
        demoCase("case:enums.c_style.direction.find_directions.should_return_none_for_non_positive_count")
        XCTAssertNil(findDirections(count: 0))
    }
}
