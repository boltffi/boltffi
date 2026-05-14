import Demo
import XCTest

final class MethodsTests: DemoTestCase {
    func testCounterValueAndErrorMethods() throws {
        let counter = Counter(initial: 2)
        XCTAssertEqual(counter.get(), 2)
        counter.increment()
        XCTAssertEqual(counter.get(), 3)
        counter.add(amount: 7)
        XCTAssertEqual(counter.get(), 10)
        XCTAssertEqual(try counter.tryGetPositive(), 10)
        XCTAssertEqual(counter.maybeDouble(), 20)
        XCTAssertEqual(counter.asPoint(), Point(x: 10.0, y: 0.0))
        counter.reset()
        XCTAssertEqual(counter.get(), 0)
        XCTAssertNil(counter.maybeDouble())
        assertThrowsMessageContains("count is not positive", try counter.tryGetPositive())
    }

    func testMixedRecordServiceSyncAndAsyncMethods() async throws {
        let service = MixedRecordService(label: "records")
        let record = MixedRecord.sample()

        XCTAssertEqual(service.getLabel(), "records")
        XCTAssertEqual(service.storedCount(), 0)
        XCTAssertEqual(service.echoRecord(record: record), record)
        XCTAssertEqual(
            service.storeRecordParts(
                name: record.name,
                anchor: record.anchor,
                priority: record.priority,
                shape: record.shape,
                parameters: record.parameters
            ),
            record
        )
        XCTAssertEqual(service.storedCount(), 1)
        let echoedRecord = try await service.asyncEchoRecord(record: record)
        XCTAssertEqual(echoedRecord, record)
        let storedRecord = try await service.asyncStoreRecordParts(
            name: record.name,
            anchor: record.anchor,
            priority: record.priority,
            shape: record.shape,
            parameters: record.parameters
        )
        XCTAssertEqual(storedRecord, record)
        XCTAssertEqual(service.storedCount(), 2)
    }
}
