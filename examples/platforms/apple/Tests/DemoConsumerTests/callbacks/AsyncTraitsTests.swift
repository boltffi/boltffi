import Demo
import XCTest

final class AsyncTraitsTests: DemoTestCase {
    final class SwiftAsyncFetcher: AsyncFetcher {
        func fetchValue(key: Int32) async -> Int32 { key * 100 }
        func fetchString(input: String) async -> String { input.uppercased() }
        func fetchJoinedMessage(scope: String, message: String) async -> String { "\(scope)::\(message.uppercased())" }
    }

    final class SwiftAsyncPointTransformer: AsyncPointTransformer {
        func transformPoint(point: Point) async -> Point { Point(x: point.x + 50.0, y: point.y + 60.0) }
    }

    final class SwiftAsyncOptionFetcher: AsyncOptionFetcher {
        func find(key: Int32) async -> Int64? { key > 0 ? Int64(key) * 1000 : nil }
    }

    final class SwiftAsyncOptionalMessageFetcher: AsyncOptionalMessageFetcher {
        func findMessage(key: Int32) async -> String? { key > 0 ? "async-message:\(key)" : nil }
    }

    final class SwiftAsyncResultFormatter: AsyncResultFormatter {
        func renderMessage(scope: String, message: String) async throws -> String {
            if scope.isEmpty {
                throw MathError.negativeInput
            }
            return "\(scope)::\(message.uppercased())"
        }

        func transformPoint(point: Point, status: Status) async throws -> Point {
            if status == .inactive {
                throw MathError.negativeInput
            }
            return Point(x: point.x + 500.0, y: point.y + 600.0)
        }
    }

    func testAsyncFetcherTraitFns() async throws {
        let asyncFetcher = SwiftAsyncFetcher()

        let fetchedValue = try await fetchWithAsyncCallback(fetcher: asyncFetcher, key: 5)
        XCTAssertEqual(fetchedValue, 500)
        let fetchedString = try await fetchStringWithAsyncCallback(fetcher: asyncFetcher, input: "boltffi")
        XCTAssertEqual(fetchedString, "BOLTFFI")
        let joinedMessage = try await fetchJoinedMessageWithAsyncCallback(
            fetcher: asyncFetcher,
            scope: "async",
            message: "borrowed strings"
        )
        XCTAssertEqual(joinedMessage, "async::BORROWED STRINGS")
    }

    func testAsyncRecordTraitFns() async throws {
        let asyncPointTransformer = SwiftAsyncPointTransformer()

        let transformedPoint = try await transformPointWithAsyncCallback(
            transformer: asyncPointTransformer,
            point: Point(x: 1.0, y: 2.0)
        )
        XCTAssertEqual(transformedPoint, Point(x: 51.0, y: 62.0))
    }

    func testAsyncOptionalTraitFns() async throws {
        let asyncOptionFetcher = SwiftAsyncOptionFetcher()
        let asyncOptionalMessageFetcher = SwiftAsyncOptionalMessageFetcher()

        let foundValue = try await invokeAsyncOptionFetcher(fetcher: asyncOptionFetcher, key: 7)
        XCTAssertEqual(foundValue, 7_000)
        let missingValue = try await invokeAsyncOptionFetcher(fetcher: asyncOptionFetcher, key: 0)
        XCTAssertNil(missingValue)
        let foundMessage = try await invokeAsyncOptionalMessageFetcher(fetcher: asyncOptionalMessageFetcher, key: 9)
        XCTAssertEqual(foundMessage, "async-message:9")
        let missingMessage = try await invokeAsyncOptionalMessageFetcher(fetcher: asyncOptionalMessageFetcher, key: 0)
        XCTAssertNil(missingMessage)
    }

    func testAsyncResultTraitFns() async throws {
        let asyncResultFormatter = SwiftAsyncResultFormatter()

        let renderedMessage = try await renderMessageWithAsyncResultCallback(
            formatter: asyncResultFormatter,
            scope: "async",
            message: "result"
        )
        XCTAssertEqual(renderedMessage, "async::RESULT")
        let resultPoint = try await transformPointWithAsyncResultCallback(
            formatter: asyncResultFormatter,
            point: Point(x: 3.0, y: 4.0),
            status: .active
        )
        XCTAssertEqual(resultPoint, Point(x: 503.0, y: 604.0))
        do {
            _ = try await renderMessageWithAsyncResultCallback(
                formatter: asyncResultFormatter,
                scope: "",
                message: "result"
            )
            XCTFail("expected async string result error")
        } catch let error as MathError {
            XCTAssertEqual(error, .negativeInput)
        }
        do {
            _ = try await transformPointWithAsyncResultCallback(
                formatter: asyncResultFormatter,
                point: Point(x: 3.0, y: 4.0),
                status: .inactive
            )
            XCTFail("expected async record result error")
        } catch let error as MathError {
            XCTAssertEqual(error, .negativeInput)
        }
    }
}
