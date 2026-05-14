import Demo
import XCTest

final class AsyncFnsTests: XCTestCase {
    func testAsyncFns() async throws {
        // case:async_fns.basic.scalar_string_vec
        let sum = try await asyncAdd(a: 3, b: 7)
        XCTAssertEqual(sum, 10)
        let echoedMessage = try await asyncEcho(message: "hello async")
        XCTAssertEqual(echoedMessage, "Echo: hello async")
        let doubledValues = try await asyncDoubleAll(values: [1, 2, 3])
        XCTAssertEqual(doubledValues, [2, 4, 6])
        let firstPositive = try await asyncFindPositive(values: [-1, 0, 5, 3])
        XCTAssertEqual(firstPositive, 5)
        let missingPositive = try await asyncFindPositive(values: [-1, -2, -3])
        XCTAssertNil(missingPositive)
        let concatenated = try await asyncConcat(strings: ["a", "b", "c"])
        XCTAssertEqual(concatenated, "a, b, c")
        // case:async_fns.results.compute_fetch
        let computedValue = try await tryComputeAsync(value: 4)
        XCTAssertEqual(computedValue, 8)
        do {
            _ = try await tryComputeAsync(value: -1)
            XCTFail("expected tryComputeAsync to throw")
        } catch {
            XCTAssertEqual(error as? ComputeError, .overflow(value: -1, limit: 0))
        }
        let fetchedValue = try await fetchData(id: 7)
        XCTAssertEqual(fetchedValue, 70)
        await assertAsyncThrowsMessageContains("invalid id") {
            try await fetchData(id: 0)
        }
        // case:async_fns.basic.get_numbers
        let numbers = try await asyncGetNumbers(count: 4)
        XCTAssertEqual(numbers, [0, 1, 2, 3])
        // case:async_fns.mixed_record.roundtrip
        let record = MixedRecord.sample()
        let echoedRecord = try await asyncEchoMixedRecord(record: record)
        XCTAssertEqual(echoedRecord, record)
        let createdRecord = try await asyncMakeMixedRecord(
            name: record.name,
            anchor: record.anchor,
            priority: record.priority,
            shape: record.shape,
            parameters: record.parameters
        )
        XCTAssertEqual(createdRecord, record)
    }
}
