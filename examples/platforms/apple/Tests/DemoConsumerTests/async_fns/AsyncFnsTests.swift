import Demo
import XCTest

final class AsyncFnsTests: DemoTestCase {
    func testAsyncFns() async throws {
        demoCase("case:async_fns.basic.add.should_return_sum")
        let sum = try await asyncAdd(a: 3, b: 7)
        XCTAssertEqual(sum, 10)
        demoCase("case:async_fns.basic.echo.should_prefix_message")
        let echoedMessage = try await asyncEcho(message: "hello async")
        XCTAssertEqual(echoedMessage, "Echo: hello async")
        demoCase("case:async_fns.basic.double_all.should_double_i32_vector")
        let doubledValues = try await asyncDoubleAll(values: [1, 2, 3])
        XCTAssertEqual(doubledValues, [2, 4, 6])
        demoCase("case:async_fns.basic.find_positive.should_return_first_positive")
        let firstPositive = try await asyncFindPositive(values: [-1, 0, 5, 3])
        XCTAssertEqual(firstPositive, 5)
        demoCase("case:async_fns.basic.find_positive.should_return_none_for_all_negative")
        let missingPositive = try await asyncFindPositive(values: [-1, -2, -3])
        XCTAssertNil(missingPositive)
        demoCase("case:async_fns.basic.concat.should_join_string_vector")
        let concatenated = try await asyncConcat(strings: ["a", "b", "c"])
        XCTAssertEqual(concatenated, "a, b, c")
        demoCase("case:async_fns.results.try_compute.should_return_doubled_value")
        let computedValue = try await tryComputeAsync(value: 4)
        XCTAssertEqual(computedValue, 8)
        demoCase("case:async_fns.results.try_compute.should_return_overflow_for_negative_value")
        do {
            _ = try await tryComputeAsync(value: -1)
            XCTFail("expected tryComputeAsync to throw")
        } catch {
            XCTAssertEqual(error as? ComputeError, .overflow(value: -1, limit: 0))
        }
        demoCase("case:async_fns.results.try_compute.should_return_invalid_input_for_zero")
        do {
            _ = try await tryComputeAsync(value: 0)
            XCTFail("expected tryComputeAsync to throw")
        } catch {
            XCTAssertEqual(error as? ComputeError, .invalidInput(-999))
        }
        demoCase("case:async_fns.results.fetch_data.should_return_scaled_positive_id")
        let fetchedValue = try await fetchData(id: 7)
        XCTAssertEqual(fetchedValue, 70)
        demoCase("case:async_fns.results.fetch_data.should_reject_non_positive_id")
        await assertAsyncThrowsMessageContains("invalid id") {
            try await fetchData(id: 0)
        }
        demoCase("case:async_fns.basic.get_numbers.should_return_counting_sequence")
        let numbers = try await asyncGetNumbers(count: 4)
        XCTAssertEqual(numbers, [0, 1, 2, 3])
        demoCase("case:async_fns.mixed_record.echo.should_roundtrip_record")
        let record = MixedRecord.sample()
        let echoedRecord = try await asyncEchoMixedRecord(record: record)
        XCTAssertEqual(echoedRecord, record)
        demoCase("case:async_fns.mixed_record.make.should_construct_record")
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
