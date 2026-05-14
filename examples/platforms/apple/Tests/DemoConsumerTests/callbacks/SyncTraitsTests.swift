import Demo
import XCTest

final class SyncTraitsTests: DemoTestCase {
    final class Doubler: ValueCallback {
        func onValue(value: Int32) -> Int32 { value * 2 }
    }

    final class Tripler: ValueCallback {
        func onValue(value: Int32) -> Int32 { value * 3 }
    }

    final class SwiftPointTransformer: PointTransformer {
        func transform(point: Point) -> Point { Point(x: point.x + 10.0, y: point.y + 20.0) }
    }

    final class SwiftStatusMapper: StatusMapper {
        func mapStatus(status: Status) -> Status { status == .pending ? .active : .inactive }
    }

    final class SwiftVecProcessor: VecProcessor {
        func process(values: [Int32]) -> [Int32] { values.map { $0 * $0 } }
    }

    final class SwiftMessageFormatter: MessageFormatter {
        func formatMessage(scope: String, message: String) -> String { "\(scope)::\(message.uppercased())" }
    }

    final class SwiftOptionalMessageCallback: OptionalMessageCallback {
        func findMessage(key: Int32) -> String? { key > 0 ? "message:\(key)" : nil }
    }

    final class SwiftResultMessageCallback: ResultMessageCallback {
        func renderMessage(key: Int32) throws -> String {
            if key < 0 {
                throw MathError.negativeInput
            }
            return "message:\(key)"
        }
    }

    final class SwiftMultiMethodCallback: MultiMethodCallback {
        func methodA(x: Int32) -> Int32 { x + 1 }
        func methodB(x: Int32, y: Int32) -> Int32 { x * y }
        func methodC() -> Int32 { 5 }
    }

    final class SwiftOptionCallback: OptionCallback {
        func findValue(key: Int32) -> Int32? { key > 0 ? key * 10 : nil }
    }

    final class SwiftResultCallback: ResultCallback {
        func compute(value: Int32) throws -> Int32 {
            if value < 0 {
                throw MathError.negativeInput
            }
            return value * 10
        }
    }

    final class SwiftOffsetCallback: OffsetCallback {
        func offset(value: Int, delta: UInt) -> Int { value + Int(delta) }
    }

    final class SwiftFalliblePointTransformer: FalliblePointTransformer {
        func transformPoint(point: Point, status: Status) throws -> Point {
            if status == .inactive {
                throw MathError.negativeInput
            }
            return Point(x: point.x + 100.0, y: point.y + 200.0)
        }
    }

    final class SwiftDataProvider: DataProvider {
        private let items: [DataPoint]

        init(items: [DataPoint]) {
            self.items = items
        }

        func getCount() -> UInt32 {
            UInt32(items.count)
        }

        func getItem(index: UInt32) -> DataPoint {
            items[Int(index)]
        }
    }

    func testSyncTraitFns() throws {
        let doubler = Doubler()
        let tripler = Tripler()
        let pointTransformer = SwiftPointTransformer()
        let statusMapper = SwiftStatusMapper()
        let flipper = makeStatusFlipper()
        let multiMethod = SwiftMultiMethodCallback()
        let optionCallback = SwiftOptionCallback()
        let resultCallback = SwiftResultCallback()
        let offsetCallback = SwiftOffsetCallback()
        let falliblePointTransformer = SwiftFalliblePointTransformer()
        let vecProcessor = SwiftVecProcessor()
        let messageFormatter = SwiftMessageFormatter()
        let optionalMessageCallback = SwiftOptionalMessageCallback()
        let resultMessageCallback = SwiftResultMessageCallback()
        let incrementer = makeIncrementingCallback(delta: 5)

        XCTAssertEqual(invokeValueCallback(callback: doubler, input: 4), 8)
        XCTAssertEqual(invokeValueCallbackTwice(callback: doubler, a: 3, b: 4), 14)
        XCTAssertEqual(invokeBoxedValueCallback(callback: doubler, input: 5), 10)
        XCTAssertEqual(incrementer.onValue(value: 4), 9)
        XCTAssertEqual(invokeValueCallback(callback: incrementer, input: 4), 9)
        XCTAssertEqual(invokeOptionalValueCallback(callback: doubler, input: 4), 8)
        XCTAssertEqual(invokeOptionalValueCallback(callback: nil, input: 4), 4)
        XCTAssertEqual(
            formatMessageWithCallback(formatter: messageFormatter, scope: "sync", message: "borrowed strings"),
            "sync::BORROWED STRINGS"
        )
        XCTAssertEqual(
            formatMessageWithBoxedCallback(formatter: messageFormatter, scope: "boxed", message: "borrowed strings"),
            "boxed::BORROWED STRINGS"
        )
        XCTAssertEqual(
            formatMessageWithOptionalCallback(formatter: messageFormatter, scope: "optional", message: "borrowed strings"),
            "optional::BORROWED STRINGS"
        )
        XCTAssertEqual(
            formatMessageWithOptionalCallback(formatter: nil, scope: "fallback", message: "message"),
            "fallback::message"
        )
        let prefixer = makeMessagePrefixer(prefix: "prefix")
        XCTAssertEqual(prefixer.formatMessage(scope: "scope", message: "message"), "prefix::scope::message")
        XCTAssertEqual(
            formatMessageWithCallback(formatter: prefixer, scope: "sync", message: "formatter"),
            "prefix::sync::formatter"
        )
        XCTAssertEqual(invokeOptionalMessageCallback(callback: optionalMessageCallback, key: 7), "message:7")
        XCTAssertNil(invokeOptionalMessageCallback(callback: optionalMessageCallback, key: 0))
        XCTAssertEqual(try invokeResultMessageCallback(callback: resultMessageCallback, key: 8), "message:8")
        XCTAssertThrowsError(try invokeResultMessageCallback(callback: resultMessageCallback, key: -1)) { error in
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
        XCTAssertEqual(transformPoint(transformer: pointTransformer, point: Point(x: 1.0, y: 2.0)), Point(x: 11.0, y: 22.0))
        XCTAssertEqual(transformPointBoxed(transformer: pointTransformer, point: Point(x: 3.0, y: 4.0)), Point(x: 13.0, y: 24.0))
        XCTAssertEqual(mapStatus(mapper: statusMapper, status: .pending), .active)
        XCTAssertEqual(flipper.mapStatus(status: .active), .inactive)
        XCTAssertEqual(mapStatus(mapper: flipper, status: .inactive), .pending)
        XCTAssertEqual(processVec(processor: vecProcessor, values: [1, 2, 3]), [1, 4, 9])
        XCTAssertEqual(invokeMultiMethod(callback: multiMethod, x: 3, y: 4), 21)
        XCTAssertEqual(invokeMultiMethodBoxed(callback: multiMethod, x: 3, y: 4), 21)
        XCTAssertEqual(invokeTwoCallbacks(first: doubler, second: tripler, value: 5), 25)
        XCTAssertEqual(invokeOptionCallback(callback: optionCallback, key: 7), 70)
        XCTAssertNil(invokeOptionCallback(callback: optionCallback, key: 0))
        XCTAssertEqual(try invokeResultCallback(callback: resultCallback, value: 7), 70)
        XCTAssertThrowsError(try invokeResultCallback(callback: resultCallback, value: -1)) { error in
            XCTAssertEqual(error as? MathError, .negativeInput)
        }
        XCTAssertEqual(invokeOffsetCallback(callback: offsetCallback, value: -5, delta: 8), 3)
        XCTAssertEqual(invokeBoxedOffsetCallback(callback: offsetCallback, value: 10, delta: 4), 14)
        XCTAssertEqual(
            try invokeFalliblePointTransformer(
                callback: falliblePointTransformer,
                point: Point(x: 2.0, y: 3.0),
                status: .active
            ),
            Point(x: 102.0, y: 203.0)
        )
        XCTAssertThrowsError(
            try invokeFalliblePointTransformer(
                callback: falliblePointTransformer,
                point: Point(x: 2.0, y: 3.0),
                status: .inactive
            )
        ) { error in
            XCTAssertEqual(error as? MathError, .negativeInput)
        }

        let dataConsumer = DataConsumer()
        let dataProvider = SwiftDataProvider(items: [
            DataPoint(x: 1.0, y: 2.0, timestamp: 3),
            DataPoint(x: 4.0, y: 5.0, timestamp: 6),
        ])
        dataConsumer.setProvider(provider: dataProvider)
        XCTAssertEqual(dataConsumer.computeSum(), 12)
    }
}
