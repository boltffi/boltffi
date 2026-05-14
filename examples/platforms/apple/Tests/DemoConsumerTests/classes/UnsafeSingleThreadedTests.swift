import Demo
import XCTest

final class UnsafeSingleThreadedTests: DemoTestCase {
    func testStateHolderSyncAndAsyncMethods() async throws {
        let stateHolder = StateHolder(label: "local")
        let incrementer = makeIncrementingCallback(delta: 3)
        XCTAssertEqual(stateHolder.getLabel(), "local")
        XCTAssertEqual(stateHolder.getValue(), 0)
        stateHolder.setValue(value: 5)
        XCTAssertEqual(stateHolder.getValue(), 5)
        XCTAssertEqual(stateHolder.increment(), 6)
        stateHolder.addItem(item: "a")
        stateHolder.addItem(item: "b")
        XCTAssertEqual(stateHolder.itemCount(), 2)
        XCTAssertEqual(stateHolder.getItems(), ["a", "b"])
        XCTAssertEqual(stateHolder.removeLast(), "b")
        XCTAssertEqual(stateHolder.transformValue(f: { $0 / 2 }), 3)
        XCTAssertEqual(stateHolder.applyValueCallback(callback: incrementer), 6)
        let asyncValue = try await stateHolder.asyncGetValue()
        XCTAssertEqual(asyncValue, 6)
        try await stateHolder.asyncSetValue(value: 9)
        XCTAssertEqual(stateHolder.getValue(), 9)
        let asyncItemCount = try await stateHolder.asyncAddItem(item: "z")
        XCTAssertEqual(asyncItemCount, 2)
        XCTAssertEqual(stateHolder.getItems(), ["a", "z"])
        stateHolder.clear()
        XCTAssertEqual(stateHolder.getValue(), 0)
        XCTAssertEqual(stateHolder.getItems(), [])

        let counter = CounterSingleThreaded()
        counter.set(value: 5)
        XCTAssertEqual(counter.get(), 5)
        counter.increment()
        XCTAssertEqual(counter.get(), 6)

        let accumulator = AccumulatorSingleThreaded()
        accumulator.add(amount: 4)
        accumulator.add(amount: 6)
        XCTAssertEqual(accumulator.get(), 10)
        accumulator.reset()
        XCTAssertEqual(accumulator.get(), 0)
    }
}
