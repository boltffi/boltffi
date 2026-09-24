import Demo
import XCTest

final class CloseGuardTests: DemoTestCase {
    func testGuardedCounterMethods() {
        let counter = GuardedCounter(initial: 1)
        XCTAssertEqual(counter.increment(), 2)
        XCTAssertEqual(counter.incrementThroughGate(gate: { $0 + 5 }), 9)
    }
}
