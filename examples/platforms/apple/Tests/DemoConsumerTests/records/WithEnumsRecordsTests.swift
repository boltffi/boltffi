import Demo
import XCTest

final class WithEnumsRecordsTests: XCTestCase {
    func testTaskFns() {
        // case:records.with_enums.task.echo
        // case:records.with_enums.task.make_urgent
        XCTAssertEqual(echoTask(task: Task(title: "ship", priority: .high, completed: false)), Task(title: "ship", priority: .high, completed: false))
        XCTAssertEqual(makeTask(title: "ship", priority: .critical).completed, false)
        XCTAssertEqual(isUrgent(task: Task(title: "ship", priority: .critical, completed: false)), true)
    }

    func testNotificationFns() {
        // case:records.with_enums.notification.echo
        XCTAssertEqual(echoNotification(notification: Notification(message: "hello", priority: .low, read: false)), Notification(message: "hello", priority: .low, read: false))
    }

    func testHolderFns() {
        // case:records.with_enums.holder.triangle
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
        // case:records.with_enums.task_header.roundtrip
        let header = makeCriticalTaskHeader(id: 42)
        XCTAssertEqual(header.id, 42)
        XCTAssertEqual(header.priority, Priority.critical)
        XCTAssertFalse(header.completed)
        XCTAssertEqual(echoTaskHeader(header: header), header)
    }

    func testLogEntryFns() {
        // case:records.with_enums.log_entry.roundtrip
        let entry = makeErrorLogEntry(timestamp: 1234567890, code: 42)
        XCTAssertEqual(entry.timestamp, 1234567890)
        XCTAssertEqual(entry.level, LogLevel.error)
        XCTAssertEqual(entry.code, 42)
        XCTAssertEqual(echoLogEntry(entry: entry), entry)
    }
}
