import Demo
import XCTest

final class BorrowedTests: DemoTestCase {
    func testDescribeCounterWithBorrowedRef() {
        let counter = Counter(initial: 42)
        XCTAssertEqual(describeCounter(counter: counter), "Counter(value=42)")
    }
}
