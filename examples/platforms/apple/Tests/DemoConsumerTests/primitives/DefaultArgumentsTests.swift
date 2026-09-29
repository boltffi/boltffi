import Demo
import XCTest

/// Swift renders no parameter defaults yet (the demo cases exclude it), so
/// every argument is passed here; the values match the Rust defaults.
final class DefaultArgumentsTests: DemoTestCase {
    final class Doubler: ValueCallback {
        func onValue(value: Int32) -> Int32 { value * 2 }
    }

    func testDefaultArgumentFns() {
        XCTAssertEqual(repeatGreeting(name: "ada", greeting: "hello", times: 2, shout: false), "hello ada, hello ada")
        XCTAssertEqual(describeLimit(label: nil, limit: 7), "none:7")
        XCTAssertEqual(applyOptionalCallback(value: 21, callback: nil), 21)
        XCTAssertEqual(applyOptionalCallback(value: 21, callback: Doubler()), 42)
        let counter = DefaultedCounter(start: 10)
        XCTAssertEqual(counter.offset(step: 1), 11)
    }
}
