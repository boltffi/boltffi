import Demo
import XCTest

final class AsyncMethodsTests: DemoTestCase {
    func testAsyncWorkerMethodsAndErrorPaths() async throws {
        let worker = AsyncWorker(prefix: "test")
        XCTAssertEqual(worker.getPrefix(), "test")
        let processedInput = try await worker.process(input: "data")
        XCTAssertEqual(processedInput, "test: data")
        let tryProcessedInput = try await worker.tryProcess(input: "data")
        XCTAssertEqual(tryProcessedInput, "test: data")
        await assertAsyncThrowsMessageContains("input must not be empty") { try await worker.tryProcess(input: "") }
        let foundItem = try await worker.findItem(id: 42)
        XCTAssertEqual(foundItem, "test_42")
        let missingItem = try await worker.findItem(id: -1)
        XCTAssertNil(missingItem)
        let processedBatch = try await worker.processBatch(inputs: ["x", "y"])
        XCTAssertEqual(processedBatch, ["test: x", "test: y"])
    }
}

