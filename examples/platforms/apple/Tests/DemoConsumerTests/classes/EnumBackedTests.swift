import Demo
import XCTest

final class EnumBackedTests: DemoTestCase {
    func testTrafficLightMethodsSeeTheVariantBehindTheHandle() {
        let red = TrafficLight()
        demoCase("case:classes.enum_backed.traffic_light.seconds_left.should_match_on_the_variant_behind_the_handle")
        XCTAssertEqual(red.secondsLeft(), 0)
        XCTAssertEqual(red.turnGreen(seconds: 30).secondsLeft(), 30)
    }
}
