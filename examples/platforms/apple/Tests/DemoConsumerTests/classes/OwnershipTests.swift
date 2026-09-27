import Demo
import XCTest

final class OwnershipTests: DemoTestCase {
    func testOwnershipTransfersAndBorrowing() throws {
        let drops = MessageDrops()
        var message: OwnedMessage? = OwnedMessage(text: "hello", drops: drops)
        weak var original = message
        XCTAssertEqual(consumeMessage(message: message!), 5)
        message = nil
        XCTAssertNil(original)
        XCTAssertEqual(drops.count(), 1)
        XCTAssertThrowsError(try consumeMessageResult(message: OwnedMessage(text: "", drops: drops)))
        XCTAssertEqual(drops.count(), 2)

        let duplicate = OwnedMessage(text: "duplicate", drops: drops)
        XCTAssertEqual(consumeMessages(first: duplicate, second: duplicate), 0)
        XCTAssertEqual(drops.count(), 3)
        var borrowed: OwnedMessage? = OwnedMessage(text: "hello", drops: drops)
        XCTAssertEqual(borrowMessage(message: borrowed!), 5)
        mutateMessage(message: borrowed!)
        XCTAssertEqual(borrowed!.length(), 6)
        borrowed = nil
        XCTAssertEqual(drops.count(), 4)

        var returned: OwnedMessage? = returnMessage(message: OwnedMessage(text: "hello", drops: drops))
        XCTAssertEqual(returned!.length(), 5)
        returned = nil
        XCTAssertEqual(drops.count(), 5)
        XCTAssertEqual(consumeOptionalMessage(message: nil), 0)
        XCTAssertEqual(consumeOptionalMessage(message: OwnedMessage(text: "hello", drops: drops)), 5)
        XCTAssertEqual(joinMessage(labels: ["a", "b"], message: OwnedMessage(text: "hello", drops: drops)), "ahellob")
        XCTAssertEqual(consumeMessageWithOffset(message: OwnedMessage(text: "hello", drops: drops), messageOwnedHandle: 1, boltffiCallResult: 2, boltffiOwnedHandle0: 3), 11)
        discardMessage(message: OwnedMessage(text: "hello", drops: drops))
        let store = MessageStore(fromMessage: OwnedMessage(text: "stored", drops: drops))
        XCTAssertEqual(store.count(), 6)
        XCTAssertEqual(consumeMessageWithCallback(callback: { $0 + 1 }, message: OwnedMessage(text: "hello", drops: drops)), 6)
        XCTAssertEqual(consumeMessageBeforeCallback(message: OwnedMessage(text: "hello", drops: drops), callback: { $0 + 1 }), 6)
        XCTAssertEqual(consumeNamedMessage(owner: OwnedMessage(text: "hello", drops: drops), handle: 1, valid: 2, ownerOwned: 3), 11)
        let receiver = OwnedMessage(text: "receiver", drops: drops)
        XCTAssertEqual(receiver.combine(other: OwnedMessage(text: "hello", drops: drops)), 13)
    }

    func testAsyncTransferAndCancellation() async throws {
        let drops = MessageDrops()
        let store = MessageStore()
        try await store.set(message: OwnedMessage(text: "stored", drops: drops), carrier: ["trace": "context"])
        XCTAssertEqual(store.count(), 7)
        XCTAssertEqual(drops.count(), 1)
        let result = try await consumeMessageAsync(message: OwnedMessage(text: "hello", drops: drops))
        XCTAssertEqual(result, 5)
        let combined = try await consumeMessagesAsync(first: OwnedMessage(text: "one", drops: drops), second: OwnedMessage(text: "two", drops: drops))
        XCTAssertEqual(combined, 6)
        let optional = try await consumeOptionalMessageAsync(message: nil)
        XCTAssertEqual(optional, 0)
        let owned = _Concurrency.Task { try await holdOwnedMessage(message: OwnedMessage(text: "owned", drops: drops)) }
        owned.cancel()
        do { _ = try await owned.value; XCTFail("expected cancellation") } catch is CancellationError {}
    }

    func testRejectedMoveDuringBorrowDoesNotLeaveADanglingWrapper() async throws {
        let drops = MessageDrops()
        var message: OwnedMessage? = OwnedMessage(text: "retained", drops: drops)
        weak var wrapper = message
        let pending = _Concurrency.Task { try await holdMessage(message: message!) }
        let deadline = Date().addingTimeInterval(5)
        while drops.borrowCount() == 0 && Date() < deadline {
            await _Concurrency.Task<Never, Never>.yield()
        }
        XCTAssertEqual(drops.borrowCount(), 1)
        XCTAssertEqual(consumeMessage(message: message!), 0)
        message = nil
        XCTAssertEqual(drops.count(), 0)
        pending.cancel()
        do { _ = try await pending.value; XCTFail("expected cancellation") } catch is CancellationError {}
        XCTAssertNil(wrapper)
        XCTAssertEqual(drops.borrowCount(), 0)
        XCTAssertEqual(drops.count(), 1)
    }
}
