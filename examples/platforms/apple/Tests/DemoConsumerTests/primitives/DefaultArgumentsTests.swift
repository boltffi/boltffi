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

    func testFloatAndEnumArguments() {
        demoCase("case:primitives.default_arguments.should_apply_float_and_enum_defaults")
        XCTAssertEqual(scaleDefault(), 0.75)
        XCTAssertEqual(scaleDefault(ratio: 2, weight: 4, mode: .loud), 16)
        XCTAssertEqual(scaleDefault(weight: nil), 0.5)
        XCTAssertTrue(DefaultMode.quiet.matches())
        XCTAssertFalse(DefaultMode.loud.matches())
        XCTAssertTrue(DefaultMode.loud.matches(mode: .loud))
    }

    func testRecordArguments() {
        demoCase("case:primitives.default_arguments.should_apply_record_defaults")
        let amount = DefaultAmount()
        XCTAssertEqual(amount.value, 3)
        XCTAssertEqual(amount.offset(), 5)
        XCTAssertEqual(amount.offset(step: 4), 7)
        XCTAssertEqual(DefaultAmount(value: 9).value, 9)
        XCTAssertEqual(NamedAmount().value, 5)
        XCTAssertEqual(NamedAmount(withValue: 9).value, 9)
    }

    func testCustomTypeArguments() {
        demoCase("case:primitives.default_arguments.should_apply_custom_type_defaults")
        XCTAssertEqual(defaultTimeoutSeconds(), 1.5)
        XCTAssertEqual(defaultTimeoutSeconds(timeout: TimeoutFFI(seconds: 2.5)), 2.5)
        XCTAssertNil(defaultLimit())
        XCTAssertEqual(defaultLimit(limit: 7), 7)
    }

    func testAsyncArguments() async throws {
        demoCase("case:primitives.default_arguments.should_apply_async_defaults")
        let defaultValue = try await asyncDefault()
        let suppliedValue = try await asyncDefault(value: 4)
        XCTAssertEqual(defaultValue, 9)
        XCTAssertEqual(suppliedValue, 4)

        let defaultAmount = try await NamedAmount.load()
        let suppliedAmount = try await NamedAmount.load(value: 8)
        XCTAssertEqual(defaultAmount.value, 6)
        XCTAssertEqual(suppliedAmount.value, 8)
        let defaultMode = try await DefaultMode.load()
        let suppliedMode = try await DefaultMode.load(mode: .loud)
        XCTAssertEqual(defaultMode, .quiet)
        XCTAssertEqual(suppliedMode, .loud)

        let counter = DefaultedCounter()
        let defaultOffset = try await counter.asyncOffset()
        let suppliedOffset = try await counter.asyncOffset(step: 6)
        XCTAssertEqual(defaultOffset, 13)
        XCTAssertEqual(suppliedOffset, 16)

        let started = try await DefaultedCounter.start()
        let doubled = try await DefaultedCounter.start(second: Doubler())
        XCTAssertEqual(started.offset(), 31)
        XCTAssertEqual(doubled.offset(), 61)
    }
}
