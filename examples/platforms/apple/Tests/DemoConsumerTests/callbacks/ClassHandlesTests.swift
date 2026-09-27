import Demo
import XCTest

final class ClassHandlesTests: DemoTestCase {
    final class Receiver: MessageReceiver, FallibleMessageReceiver {
        var first: OwnedMessage?
        var second: OwnedMessage?
        var fail = false
        var forward: MessageReceiver?

        func attach(handle: OwnedMessage, callback: UInt32) {
            XCTAssertEqual(callback, 42)
            XCTAssertEqual(handle.length(), 9)
            if let forward {
                forward.attach(handle: handle, callback: callback)
            } else {
                first = handle
            }
        }

        func optional(handle: OwnedMessage?) {
            first = handle
        }

        func pair(first: OwnedMessage, label: String, second: OwnedMessage?) throws -> Int32 {
            XCTAssertEqual(label, "pair")
            self.first = first
            self.second = second
            if fail { throw MathError.negativeInput }
            return Int32(first.length() + (second?.length() ?? 0))
        }
    }

    func testRetainedMessageOutlivesCallback() {
        demoCase("case:callbacks.class_handles.should_retain_after_return")
        let drops = MessageDrops()
        let receiver = Receiver()
        deliverMessage(receiver: receiver, drops: drops)
        XCTAssertEqual(drops.count(), 0)
        XCTAssertEqual(receiver.first?.length(), 9)
        receiver.first = nil
        XCTAssertEqual(drops.count(), 1)
    }

    func testMultipleAndOptionalMessages() throws {
        demoCase("case:callbacks.class_handles.should_deliver_multiple_and_optional")
        let drops = MessageDrops()
        let receiver = Receiver()
        XCTAssertEqual(try deliverMessagePair(receiver: receiver, drops: drops, present: true), 11)
        XCTAssertEqual(drops.count(), 0)
        XCTAssertEqual(receiver.first?.length(), 5)
        XCTAssertEqual(receiver.second?.length(), 6)
        receiver.first = nil
        XCTAssertEqual(drops.count(), 1)
        receiver.second = nil
        XCTAssertEqual(drops.count(), 2)
        XCTAssertEqual(try deliverMessagePair(receiver: receiver, drops: drops, present: false), 5)
        XCTAssertNil(receiver.second)
        XCTAssertEqual(drops.count(), 2)
        receiver.first = nil
        XCTAssertEqual(drops.count(), 3)
    }

    func testStoredMessagesSurviveCallbackError() {
        demoCase("case:callbacks.class_handles.should_retain_after_error")
        let drops = MessageDrops()
        let receiver = Receiver()
        receiver.fail = true
        XCTAssertThrowsError(try deliverMessagePair(receiver: receiver, drops: drops, present: true)) {
            XCTAssertEqual($0 as? MathError, .negativeInput)
        }
        XCTAssertEqual(drops.count(), 0)
        XCTAssertEqual(receiver.first?.length(), 5)
        XCTAssertEqual(receiver.second?.length(), 6)
        receiver.first = nil
        receiver.second = nil
        XCTAssertEqual(drops.count(), 2)
    }

    func testRustCallbackConsumesMessages() {
        demoCase("case:callbacks.class_handles.should_consume_in_rust_callback")
        let drops = MessageDrops()
        let receiver = makeMessageReceiver()
        let first = OwnedMessage(text: "hello", drops: drops)
        receiver.attach(handle: first, callback: 42)
        XCTAssertEqual(drops.count(), 1)
        let second = OwnedMessage(text: "two", drops: drops)
        receiver.optional(handle: second)
        XCTAssertEqual(drops.count(), 2)
        receiver.optional(handle: nil)
        XCTAssertEqual(drops.count(), 2)
    }

    func testCallbackCanImmediatelyPassOwnershipBackToRust() {
        let drops = MessageDrops()
        let receiver = Receiver()
        receiver.forward = makeMessageReceiver()
        deliverMessage(receiver: receiver, drops: drops)
        XCTAssertNil(receiver.first)
        XCTAssertEqual(drops.count(), 1)
    }
}
