import Demo
import XCTest

final class ConstructorsTests: DemoTestCase {
    func testInventoryConstructorsAndCollectionMethods() throws {
        let inventory = Inventory()
        XCTAssertEqual(inventory.capacity(), 100)
        XCTAssertEqual(inventory.count(), 0)
        XCTAssertEqual(inventory.add(item: "hammer"), true)
        XCTAssertEqual(inventory.getAll(), ["hammer"])
        XCTAssertEqual(inventory.remove(index: 0), "hammer")
        XCTAssertNil(inventory.remove(index: 0))

        let smallInventory = Inventory(withCapacity: 2)
        XCTAssertEqual(smallInventory.capacity(), 2)
        XCTAssertEqual(smallInventory.add(item: "a"), true)
        XCTAssertEqual(smallInventory.add(item: "b"), true)
        XCTAssertEqual(smallInventory.add(item: "c"), false)
        XCTAssertEqual(smallInventory.getAll(), ["a", "b"])

        let tryInventory = try Inventory(tryNew: 1)
        XCTAssertEqual(tryInventory.capacity(), 1)
        XCTAssertEqual(tryInventory.add(item: "only"), true)
        XCTAssertEqual(tryInventory.add(item: "overflow"), false)
        assertThrowsMessageContains("capacity must be greater than zero", try Inventory(tryNew: 0))
    }
}

