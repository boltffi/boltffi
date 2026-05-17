import Demo
import Foundation
import XCTest

final class CustomTypesTests: DemoTestCase {
    func testCustomTypesRoundTrip() {
        let email = "café@example.com"
        XCTAssertEqual(echoEmail(email: email), email, "case:custom_types.email.should_roundtrip_value")
        XCTAssertEqual(emailDomain(email: email), "example.com", "case:custom_types.email.should_extract_domain")

        let datetime: UtcDateTime = 1_701_234_567_890
        XCTAssertEqual(echoDatetime(dt: datetime), datetime, "case:custom_types.datetime.should_roundtrip_millis")
        XCTAssertEqual(datetimeToMillis(dt: datetime), 1_701_234_567_890, "case:custom_types.datetime.should_convert_to_millis")

        XCTAssertTrue(formatTimestamp(timestamp: datetime).contains("2023"), "case:custom_types.datetime.should_format_rfc3339_timestamp")

        let event = Event(name: "launch", timestamp: datetime)
        demoCase("case:custom_types.event.should_expose_datetime_field")
        XCTAssertEqual(event.name, "launch")
        XCTAssertEqual(event.timestamp, datetime)
        demoCase("case:custom_types.event.should_roundtrip_datetime_field")
        let echoedEvent = echoEvent(event: event)
        XCTAssertEqual(echoedEvent, event)
        demoCase("case:custom_types.event.should_extract_timestamp_millis")
        XCTAssertEqual(eventTimestamp(event: event), datetime)

        let emails = ["café@example.com", "user@example.org"]
        demoCase("case:custom_types.vectors.emails.should_roundtrip_values")
        XCTAssertEqual(echoEmails(emails: emails), emails)

        let dts: [UtcDateTime] = [1_710_000_000_000, 1_710_000_001_000, 1_710_000_002_000]
        demoCase("case:custom_types.vectors.datetimes.should_roundtrip_millis_values")
        XCTAssertEqual(echoDatetimes(dts: dts), dts)
    }
}
