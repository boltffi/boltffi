import Demo
import XCTest

final class ReprIntEnumsTests: DemoTestCase {
    func testPriorityFns() {
        demoCase("case:enums.repr_int.priority.should_roundtrip_value")
        XCTAssertEqual(echoPriority(p: Priority.high), Priority.high)
        demoCase("case:enums.repr_int.priority.should_render_label")
        XCTAssertEqual(priorityLabel(p: Priority.low), "low")
        demoCase("case:enums.repr_int.priority.should_identify_high_priority")
        XCTAssertEqual(isHighPriority(p: Priority.critical), true)
        XCTAssertEqual(isHighPriority(p: Priority.low), false)
    }

    func testLogLevelFns() {
        demoCase("case:enums.repr_int.log_level.should_roundtrip_value")
        XCTAssertEqual(echoLogLevel(level: LogLevel.info), LogLevel.info)
        demoCase("case:enums.repr_int.log_level.should_compare_against_minimum")
        XCTAssertEqual(shouldLog(level: LogLevel.error, minLevel: LogLevel.warn), true)

        demoCase("case:enums.repr_int.log_level.should_roundtrip_vectors")
        XCTAssertEqual(echoVecLogLevel(levels: [LogLevel.trace, LogLevel.info, LogLevel.error]), [LogLevel.trace, LogLevel.info, LogLevel.error])
    }

    func testHttpCodeFns() {
        demoCase("case:enums.repr_int.http_code.should_expose_discriminant_values")
        XCTAssertEqual(HttpCode.ok.rawValue, 200)
        XCTAssertEqual(HttpCode.notFound.rawValue, 404)
        XCTAssertEqual(HttpCode.serverError.rawValue, 500)
        demoCase("case:enums.repr_int.http_code.should_return_not_found")
        XCTAssertEqual(httpCodeNotFound(), HttpCode.notFound)
        demoCase("case:enums.repr_int.http_code.should_roundtrip_values")
        XCTAssertEqual(echoHttpCode(code: HttpCode.ok), HttpCode.ok)
        XCTAssertEqual(echoHttpCode(code: HttpCode.serverError), HttpCode.serverError)
    }

    func testSignFns() {
        demoCase("case:enums.repr_int.sign.should_expose_signed_discriminant_values")
        XCTAssertEqual(Sign.negative.rawValue, -1)
        XCTAssertEqual(Sign.zero.rawValue, 0)
        XCTAssertEqual(Sign.positive.rawValue, 1)
        demoCase("case:enums.repr_int.sign.should_return_negative")
        XCTAssertEqual(signNegative(), Sign.negative)
        demoCase("case:enums.repr_int.sign.should_roundtrip_signed_values")
        XCTAssertEqual(echoSign(s: Sign.negative), Sign.negative)
        XCTAssertEqual(echoSign(s: Sign.positive), Sign.positive)
    }
}
