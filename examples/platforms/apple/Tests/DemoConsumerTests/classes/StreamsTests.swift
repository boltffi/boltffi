import Demo
import XCTest

private final class CallbackCollector: @unchecked Sendable {
    private let queue = DispatchQueue(label: "callback-collector")
    private var items: [Int32] = []

    var count: Int { queue.sync { items.count } }
    var snapshot: [Int32] { queue.sync { items } }

    func append(_ value: Int32) {
        queue.sync { items.append(value) }
    }
}

final class StreamsTests: DemoTestCase {
    func testEventBusStreamsDeliverValuesAndPoints() async throws {
        let bus = EventBus()
        async let values: [Int32] = collectPrefix(bus.subscribeValues(), count: 4)
        async let points: [Point] = collectPrefix(bus.subscribePoints(), count: 2)

        try await _Concurrency.Task.sleep(nanoseconds: 100_000_000)
        bus.emitValue(value: 1)
        XCTAssertEqual(bus.emitBatch(values: [2, 3, 4]), 3)
        bus.emitPoint(point: Point(x: 1.0, y: 2.0))
        bus.emitPoint(point: Point(x: 3.0, y: 4.0))

        let emittedValues = await values
        XCTAssertEqual(emittedValues, [1, 2, 3, 4])
        let emittedPoints = await points
        XCTAssertEqual(emittedPoints, [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0)])
    }

    func testEventBusSubscribeValuesBatch() throws {
        let bus = EventBus()
        let sub = bus.subscribeValuesBatch()
        bus.emitValue(value: 100)
        bus.emitValue(value: 200)
        bus.emitValue(value: 300)
        Thread.sleep(forTimeInterval: 0.1)
        let batch = sub.popBatch(maxCount: 16)
        XCTAssertTrue(batch.contains(100))
        XCTAssertTrue(batch.contains(200))
        XCTAssertTrue(batch.contains(300))
        sub.unsubscribe()
    }

    func testEventBusSubscribeValuesCallback() async throws {
        let bus = EventBus()
        let expectation = XCTestExpectation(description: "callback receives values")
        let collected = CallbackCollector()

        let cancellable = bus.subscribeValuesCallback(callback: { value in
            collected.append(value)
            if collected.count >= 3 { expectation.fulfill() }
        })

        try await _Concurrency.Task.sleep(nanoseconds: 100_000_000)
        bus.emitValue(value: 10)
        bus.emitValue(value: 20)
        bus.emitValue(value: 30)

        await fulfillment(of: [expectation], timeout: 5.0)
        let values = collected.snapshot
        XCTAssertTrue(values.contains(10))
        XCTAssertTrue(values.contains(20))
        XCTAssertTrue(values.contains(30))
        cancellable.cancel()
    }
}

