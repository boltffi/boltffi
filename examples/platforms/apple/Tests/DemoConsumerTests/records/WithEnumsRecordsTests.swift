import Demo
import XCTest

final class WithEnumsRecordsTests: DemoTestCase {
    func testTaskFns() {
        demoCase("case:records.with_enums.task.make_urgent")
        let task = makeTask(title: "ship", priority: .critical)
        XCTAssertEqual(task.completed, false)
        XCTAssertEqual(isUrgent(task: task), true)

        demoCase("case:records.with_enums.task.echo")
        XCTAssertEqual(echoTask(task: task), task)
    }

    func testNotificationFns() {
        demoCase("case:records.with_enums.notification.echo")
        XCTAssertEqual(echoNotification(notification: Notification(message: "hello", priority: .low, read: false)), Notification(message: "hello", priority: .low, read: false))
    }

    func testHolderFns() {
        demoCase("case:records.with_enums.holder.triangle")
        let triangle = makeTriangleHolder()
        guard case let .triangle(a, b, c) = triangle.shape else {
            return XCTFail("expected Triangle variant")
        }
        XCTAssertEqual(a, Point(x: 0.0, y: 0.0))
        XCTAssertEqual(b, Point(x: 4.0, y: 0.0))
        XCTAssertEqual(c, Point(x: 0.0, y: 3.0))
        XCTAssertEqual(echoHolder(h: triangle), triangle)
    }

    func testTaskHeaderFns() {
        demoCase("case:records.with_enums.task_header.roundtrip")
        let header = makeCriticalTaskHeader(id: 42)
        XCTAssertEqual(header.id, 42)
        XCTAssertEqual(header.priority, Priority.critical)
        XCTAssertFalse(header.completed)
        XCTAssertEqual(echoTaskHeader(header: header), header)
    }

    func testLogEntryFns() {
        demoCase("case:records.with_enums.log_entry.roundtrip")
        let entry = makeErrorLogEntry(timestamp: 1234567890, code: 42)
        XCTAssertEqual(entry.timestamp, 1234567890)
        XCTAssertEqual(entry.level, LogLevel.error)
        XCTAssertEqual(entry.code, 42)
        XCTAssertEqual(echoLogEntry(entry: entry), entry)
    }
}
