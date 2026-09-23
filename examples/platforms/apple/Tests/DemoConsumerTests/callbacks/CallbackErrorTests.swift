import Demo
import Foundation
import XCTest

final class CallbackErrorTests: DemoTestCase {
    final class Worker: FallibleWorker {
        func run(mode: Int32) throws {
            if mode == 1 { throw MathError.negativeInput }
            if mode == 2 { throw NSError(domain: "UnexpectedCallbackError", code: 1) }
        }

        func value(mode: Int32) throws -> Int32 {
            try run(mode: mode)
            return 42
        }
    }

    final class AsyncWorker: AsyncFallibleWorker {
        func run(mode: Int32) async throws {
            try Worker().run(mode: mode)
        }

        func value(mode: Int32) async throws -> Int32 {
            try Worker().value(mode: mode)
        }
    }

    func testCallbackErrorsReachRust() async throws {
        let worker = Worker()
        let asyncWorker = AsyncWorker()
        demoCase("case:callbacks.errors.unit.should_report_success")
        try invokeUnitWorker(worker: worker, mode: 0)
        demoCase("case:callbacks.errors.unit.should_report_declared_error")
        do {
            _ = try invokeUnitWorker(worker: worker, mode: 1)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
        demoCase("case:callbacks.errors.unit.should_report_unexpected_error")
        do {
            _ = try invokeUnitWorker(worker: worker, mode: 2)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .overflow)
        }
        demoCase("case:callbacks.errors.value.should_report_success")
        let value = try invokeValueWorker(worker: worker, mode: 0)
        XCTAssertEqual(value, 42)
        demoCase("case:callbacks.errors.value.should_report_declared_error")
        do {
            _ = try invokeValueWorker(worker: worker, mode: 1)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
        demoCase("case:callbacks.errors.value.should_report_unexpected_error")
        do {
            _ = try invokeValueWorker(worker: worker, mode: 2)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .overflow)
        }
        demoCase("case:callbacks.errors.async_unit.should_report_success")
        try await invokeAsyncUnitWorker(worker: asyncWorker, mode: 0)
        demoCase("case:callbacks.errors.async_unit.should_report_declared_error")
        do {
            _ = try await invokeAsyncUnitWorker(worker: asyncWorker, mode: 1)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
        demoCase("case:callbacks.errors.async_unit.should_report_unexpected_error")
        do {
            _ = try await invokeAsyncUnitWorker(worker: asyncWorker, mode: 2)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .overflow)
        }
        demoCase("case:callbacks.errors.async_value.should_report_success")
        let asyncValue = try await invokeAsyncValueWorker(worker: asyncWorker, mode: 0)
        XCTAssertEqual(asyncValue, 42)
        demoCase("case:callbacks.errors.async_value.should_report_declared_error")
        do {
            _ = try await invokeAsyncValueWorker(worker: asyncWorker, mode: 1)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
        demoCase("case:callbacks.errors.async_value.should_report_unexpected_error")
        do {
            _ = try await invokeAsyncValueWorker(worker: asyncWorker, mode: 2)
            XCTFail("callback failure was reported as success")
        } catch {
            XCTAssertEqual(error as? MathError, .overflow)
        }
    }
}
