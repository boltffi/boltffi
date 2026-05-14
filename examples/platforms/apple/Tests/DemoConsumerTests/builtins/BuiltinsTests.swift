import Demo
import Foundation
import XCTest

final class BuiltinsTests: XCTestCase {
    func testBuiltinsRoundTrip() {
        // case:builtins.duration.basic
        XCTAssertEqual(echoDuration(d: 2.5), 2.5)
        XCTAssertEqual(makeDuration(secs: 3, nanos: 25), 3.000000025)
        XCTAssertEqual(durationAsMillis(d: 2.5), 2_500)

        // case:builtins.system_time.basic
        let instant = Date(timeIntervalSince1970: 1_701_234_567.89)
        XCTAssertEqual(echoSystemTime(t: instant), instant)
        XCTAssertEqual(systemTimeToMillis(t: instant), 1_701_234_567_890)
        XCTAssertEqual(millisToSystemTime(millis: 1_701_234_567_890), instant)

        // case:builtins.uuid.basic
        let uuid = UUID(uuidString: "123e4567-e89b-12d3-a456-426614174000")!
        XCTAssertEqual(echoUuid(id: uuid), uuid)
        XCTAssertEqual(uuidToString(id: uuid), uuid.uuidString.lowercased())

        // case:builtins.url.basic
        let url = URL(string: "https://example.com/demo?q=boltffi")!
        XCTAssertEqual(echoUrl(url: url), url)
        XCTAssertEqual(urlToString(url: url), url.absoluteString)
    }
}
