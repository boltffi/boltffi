import Demo
import Foundation
import XCTest

final class BuiltinsTests: DemoTestCase {
    func testBuiltinsRoundTrip() {
        XCTAssertEqual(echoDuration(d: 2.5), 2.5, "case:builtins.duration.should_roundtrip_value")
        XCTAssertEqual(makeDuration(secs: 3, nanos: 25), 3.000000025, "case:builtins.duration.should_construct_from_parts")
        XCTAssertEqual(durationAsMillis(d: 2.5), 2_500, "case:builtins.duration.should_report_milliseconds")

        let instant = Date(timeIntervalSince1970: 1_701_234_567.89)
        XCTAssertEqual(echoSystemTime(t: instant), instant, "case:builtins.system_time.should_roundtrip_value")
        XCTAssertEqual(systemTimeToMillis(t: instant), 1_701_234_567_890, "case:builtins.system_time.should_convert_to_epoch_milliseconds")
        XCTAssertEqual(millisToSystemTime(millis: 1_701_234_567_890), instant, "case:builtins.system_time.should_construct_from_epoch_milliseconds")

        let uuid = UUID(uuidString: "123e4567-e89b-12d3-a456-426614174000")!
        XCTAssertEqual(echoUuid(id: uuid), uuid, "case:builtins.uuid.should_roundtrip_value")
        XCTAssertEqual(uuidToString(id: uuid), uuid.uuidString.lowercased(), "case:builtins.uuid.should_format_canonical_string")

        let url = URL(string: "https://example.com/demo?q=boltffi")!
        XCTAssertEqual(echoUrl(url: url), url, "case:builtins.url.should_roundtrip_value")
        XCTAssertEqual(urlToString(url: url), url.absoluteString, "case:builtins.url.should_format_string")
    }
}
