import Demo
import XCTest

final class ThreadSafeTests: DemoTestCase {
    func testSharedCounterSyncAndAsyncMethods() async throws {
        let sharedCounter = SharedCounter(initial: 5)
        XCTAssertEqual(sharedCounter.get(), 5)
        sharedCounter.set(value: 6)
        XCTAssertEqual(sharedCounter.get(), 6)
        XCTAssertEqual(sharedCounter.increment(), 7)
        XCTAssertEqual(sharedCounter.add(amount: 3), 10)
        let asyncValue = try await sharedCounter.asyncGet()
        XCTAssertEqual(asyncValue, 10)
        let asyncAddedValue = try await sharedCounter.asyncAdd(amount: 5)
        XCTAssertEqual(asyncAddedValue, 15)
        XCTAssertEqual(sharedCounter.get(), 15)

        let emptyStore = DataStore()
        XCTAssertTrue(emptyStore.isEmpty())
        XCTAssertEqual(emptyStore.len(), 0)
        emptyStore.addParts(x: 1, y: 2, timestamp: 3)
        emptyStore.add(point: DataPoint(x: 4, y: 5, timestamp: 6))
        XCTAssertFalse(emptyStore.isEmpty())
        XCTAssertEqual(emptyStore.len(), 2)
        XCTAssertEqual(emptyStore.sum(), 12)
        var visitedPoints: [DataPoint] = []
        emptyStore.foreach(callback: { point in
            visitedPoints.append(point)
        })
        XCTAssertEqual(visitedPoints, [
            DataPoint(x: 1, y: 2, timestamp: 3),
            DataPoint(x: 4, y: 5, timestamp: 6),
        ])
        let asyncSum = try await emptyStore.asyncSum()
        XCTAssertEqual(asyncSum, 12)
        let asyncLength = try await emptyStore.asyncLen()
        XCTAssertEqual(asyncLength, 2)

        let sampledStore = DataStore.withSampleData()
        XCTAssertEqual(sampledStore.len(), 3)

        let capacityStore = DataStore(withCapacity: 8)
        XCTAssertEqual(capacityStore.len(), 0)

        let seededStore = DataStore(withInitialPoint: 7, y: 8, timestamp: 9)
        XCTAssertEqual(seededStore.sum(), 15)

        let accumulator = Accumulator()
        accumulator.add(amount: 4)
        accumulator.add(amount: 6)
        XCTAssertEqual(accumulator.get(), 10)
        accumulator.reset()
        XCTAssertEqual(accumulator.get(), 0)
    }
}
