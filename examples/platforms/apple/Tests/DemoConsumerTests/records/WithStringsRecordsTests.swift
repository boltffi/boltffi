import Demo
import XCTest

final class WithStringsRecordsTests: DemoTestCase {
    func testPersonFns() {
        demoCase("case:records.with_strings.person.should_roundtrip_value")
        XCTAssertEqual(echoPerson(p: Person(name: "Bob", age: 25)), Person(name: "Bob", age: 25))
        demoCase("case:records.with_strings.person.should_make_from_fields")
        XCTAssertEqual(makePerson(name: "Alice", age: 30), Person(name: "Alice", age: 30))
        demoCase("case:records.with_strings.person.should_format_greeting")
        XCTAssertEqual(greetPerson(p: Person(name: "Charlie", age: 40)), "Hello, Charlie! You are 40 years old.")
    }

    func testAddressFns() {
        demoCase("case:records.with_strings.address.should_roundtrip_value")
        XCTAssertEqual(echoAddress(a: Address(street: "Main", city: "AMS", zip: "1000")), Address(street: "Main", city: "AMS", zip: "1000"))
        demoCase("case:records.with_strings.address.should_format_value")
        XCTAssertEqual(formatAddress(a: Address(street: "Main", city: "AMS", zip: "1000")), "Main, AMS, 1000")
    }
}
