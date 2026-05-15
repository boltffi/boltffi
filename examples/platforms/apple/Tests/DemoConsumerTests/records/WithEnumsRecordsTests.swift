import Demo
import XCTest

final class WithEnumsRecordsTests: DemoTestCase {
    func testTaskFns() {
        demoCase("case:records.with_enums.task.should_make_incomplete_task")
        let task = makeTask(title: "ship", priority: .critical)
        XCTAssertEqual(task.completed, false)
        demoCase("case:records.with_enums.task.should_detect_urgent_priority")
        XCTAssertEqual(isUrgent(task: task), true)

        demoCase("case:records.with_enums.task.should_roundtrip_priority_field")
        XCTAssertEqual(echoTask(task: task), task)
    }

    func testNotificationFns() {
        demoCase("case:records.with_enums.notification.should_roundtrip_priority_field")
        XCTAssertEqual(echoNotification(notification: Notification(message: "hello", priority: .low, read: false)), Notification(message: "hello", priority: .low, read: false))
    }

    func testHolderFns() {
        demoCase("case:records.with_enums.holder.should_make_triangle_variant")
        let triangle = makeTriangleHolder()
        guard case let .triangle(a, b, c) = triangle.shape else {
            return XCTFail("expected Triangle variant")
        }
        XCTAssertEqual(a, Point(x: 0.0, y: 0.0))
        XCTAssertEqual(b, Point(x: 4.0, y: 0.0))
        XCTAssertEqual(c, Point(x: 0.0, y: 3.0))
        demoCase("case:records.with_enums.holder.should_roundtrip_data_enum_field")
        XCTAssertEqual(echoHolder(h: triangle), triangle)
    }

    func testTaskHeaderFns() {
        demoCase("case:records.with_enums.task_header.should_make_critical_header")
        let header = makeCriticalTaskHeader(id: 42)
        XCTAssertEqual(header.id, 42)
        XCTAssertEqual(header.priority, Priority.critical)
        XCTAssertFalse(header.completed)
        demoCase("case:records.with_enums.task_header.should_roundtrip_repr_enum_field")
        XCTAssertEqual(echoTaskHeader(header: header), header)
    }

    func testLogEntryFns() {
        demoCase("case:records.with_enums.log_entry.should_make_error_entry")
        let entry = makeErrorLogEntry(timestamp: 1234567890, code: 42)
        XCTAssertEqual(entry.timestamp, 1234567890)
        XCTAssertEqual(entry.level, LogLevel.error)
        XCTAssertEqual(entry.code, 42)
        demoCase("case:records.with_enums.log_entry.should_roundtrip_u8_enum_field")
        XCTAssertEqual(echoLogEntry(entry: entry), entry)
    }
}
