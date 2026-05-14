import Demo
import XCTest

final class WithStringsRecordsTests: XCTestCase {
    func testPersonFns() {
        // case:records.with_strings.person.basic
        XCTAssertEqual(echoPerson(p: Person(name: "Bob", age: 25)), Person(name: "Bob", age: 25))
        XCTAssertEqual(makePerson(name: "Alice", age: 30), Person(name: "Alice", age: 30))
        XCTAssertEqual(greetPerson(p: Person(name: "Charlie", age: 40)), "Hello, Charlie! You are 40 years old.")
    }

    func testAddressFns() {
        // case:records.with_strings.address.basic
        XCTAssertEqual(echoAddress(a: Address(street: "Main", city: "AMS", zip: "1000")), Address(street: "Main", city: "AMS", zip: "1000"))
        XCTAssertEqual(formatAddress(a: Address(street: "Main", city: "AMS", zip: "1000")), "Main, AMS, 1000")
    }
}
