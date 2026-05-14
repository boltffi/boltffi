import Demo
import XCTest

final class ReprIntEnumsTests: DemoTestCase {
    func testPriorityFns() {
        demoCase("case:enums.repr_int.priority.basic")
        XCTAssertEqual(echoPriority(p: Priority.high), Priority.high)
        XCTAssertEqual(priorityLabel(p: Priority.low), "low")
        XCTAssertEqual(isHighPriority(p: Priority.critical), true)
        XCTAssertEqual(isHighPriority(p: Priority.low), false)
    }

    func testLogLevelFns() {
        demoCase("case:enums.repr_int.log_level.basic")
        XCTAssertEqual(echoLogLevel(level: LogLevel.info), LogLevel.info)
        XCTAssertEqual(shouldLog(level: LogLevel.error, minLevel: LogLevel.warn), true)

        demoCase("case:enums.repr_int.log_level.vec")
        XCTAssertEqual(echoVecLogLevel(levels: [LogLevel.trace, LogLevel.info, LogLevel.error]), [LogLevel.trace, LogLevel.info, LogLevel.error])
    }

    func testHttpCodeFns() {
        demoCase("case:enums.repr_int.http_code.discriminants")
        XCTAssertEqual(HttpCode.ok.rawValue, 200)
        XCTAssertEqual(HttpCode.notFound.rawValue, 404)
        XCTAssertEqual(HttpCode.serverError.rawValue, 500)
        XCTAssertEqual(httpCodeNotFound(), HttpCode.notFound)
        XCTAssertEqual(echoHttpCode(code: HttpCode.ok), HttpCode.ok)
        XCTAssertEqual(echoHttpCode(code: HttpCode.serverError), HttpCode.serverError)
    }

    func testSignFns() {
        demoCase("case:enums.repr_int.sign.discriminants")
        XCTAssertEqual(Sign.negative.rawValue, -1)
        XCTAssertEqual(Sign.zero.rawValue, 0)
        XCTAssertEqual(Sign.positive.rawValue, 1)
        XCTAssertEqual(signNegative(), Sign.negative)
        XCTAssertEqual(echoSign(s: Sign.negative), Sign.negative)
        XCTAssertEqual(echoSign(s: Sign.positive), Sign.positive)
    }
}
