import Demo
import XCTest

final class DefaultArgumentsTests: DemoTestCase {
    final class Doubler: ValueCallback {
        func onValue(value: Int32) -> Int32 { value * 2 }
    }

    func testOmittedScalarAndStringArguments() {
        demoCase("case:primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults")
        XCTAssertEqual(repeatGreeting(name: "ada"), "hello ada, hello ada")
        XCTAssertEqual(repeatGreeting(name: "ada", shout: true), "HELLO ADA, HELLO ADA")
        XCTAssertEqual(repeatGreeting(name: "ada", greeting: "hi", times: 1), "hi ada")
    }

    func testOptionalArguments() {
        demoCase("case:primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals")
        XCTAssertEqual(describeLimit(), "none:7")
        XCTAssertEqual(describeLimit(label: "tag"), "tag:7")
        XCTAssertEqual(describeLimit(label: "tag", limit: nil), "tag:unlimited")
    }

    func testOptionalCallback() {
        demoCase("case:primitives.default_arguments.should_default_an_optional_callback_to_none")
        XCTAssertEqual(applyOptionalCallback(value: 21), 21)
        XCTAssertEqual(applyOptionalCallback(value: 21, callback: Doubler()), 42)
    }

    func testConstructorAndMethodArguments() {
        demoCase("case:primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults")
        let counter = DefaultedCounter()
        XCTAssertEqual(counter.offset(), 11)
        XCTAssertEqual(DefaultedCounter(start: 5).offset(step: 3), 8)
        XCTAssertEqual(DefaultedCounter(offset: 3).offset(), 24)
        XCTAssertEqual(DefaultedCounter(withOffset: 5, offset: 3).offset(), 9)

        let limits = DefaultedCounter.integerLimits()
        XCTAssertEqual(limits.0, Int64.min)
        XCTAssertEqual(limits.1, UInt64.max)
        let supplied = DefaultedCounter.integerLimits(lower: 0, upper: 1)
        XCTAssertEqual(supplied.0, 0)
        XCTAssertEqual(supplied.1, 1)
    }
}
