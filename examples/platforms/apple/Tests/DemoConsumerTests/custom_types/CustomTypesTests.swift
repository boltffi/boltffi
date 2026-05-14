import Demo
import Foundation
import XCTest

final class CustomTypesTests: XCTestCase {
    func testCustomTypesRoundTrip() {
        // case:custom_types.email.basic
        let email = "café@example.com"
        XCTAssertEqual(echoEmail(email: email), email)
        XCTAssertEqual(emailDomain(email: email), "example.com")

        // case:custom_types.datetime.roundtrip
        // case:custom_types.datetime.format
        let datetime: UtcDateTime = 1_701_234_567_890
        XCTAssertEqual(echoDatetime(dt: datetime), datetime)
        XCTAssertEqual(datetimeToMillis(dt: datetime), 1_701_234_567_890)
        XCTAssertTrue(formatTimestamp(timestamp: datetime).contains("2023"))

        // case:custom_types.event.basic
        let event = Event(name: "launch", timestamp: datetime)
        let echoedEvent = echoEvent(event: event)
        XCTAssertEqual(echoedEvent, event)
        XCTAssertEqual(eventTimestamp(event: event), datetime)

        // case:custom_types.vectors.basic
        let emails = ["café@example.com", "user@example.org"]
        XCTAssertEqual(echoEmails(emails: emails), emails)

        let dts: [UtcDateTime] = [1_710_000_000_000, 1_710_000_001_000, 1_710_000_002_000]
        XCTAssertEqual(echoDatetimes(dts: dts), dts)
    }
}
