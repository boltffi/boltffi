import Demo
import XCTest

final class DataEnumTests: XCTestCase {
    func testShapeFns() throws {
        let circle = Shape(radius: 5.0)
        XCTAssertEqual(Shape.unitCircle(), Shape.circle(radius: 1.0))
        XCTAssertEqual(Shape(square: 3.0), Shape.rectangle(width: 3.0, height: 3.0))
        XCTAssertEqual(try Shape(tryCircle: 2.0), Shape.circle(radius: 2.0))
        assertThrowsMessageContains("radius must be positive", try Shape(tryCircle: -1.0))
        XCTAssertEqual(Shape.variantCount(), 6)
        XCTAssertEqual(circle.area(), Double.pi * 25.0, accuracy: 1e-6)
        XCTAssertEqual(circle.describe(), "circle r=5")
        XCTAssertEqual(echoShape(s: makeCircle(radius: 2.0)), .circle(radius: 2.0))
        XCTAssertEqual(echoShape(s: makeRectangle(width: 3.0, height: 4.0)), .rectangle(width: 3.0, height: 4.0))
        XCTAssertEqual(echoVecShape(values: [.circle(radius: 2.0), .rectangle(width: 3.0, height: 4.0), .point]).count, 3)
        XCTAssertEqual(echoShape(s: .apex(tip: Point(x: 3.0, y: 4.0))), .apex(tip: Point(x: 3.0, y: 4.0)))
        XCTAssertEqual(echoShape(s: .apex(tip: nil)), .apex(tip: nil))
        XCTAssertEqual(echoShape(s: .cluster(members: [Point(x: 1.0, y: 2.0)])), .cluster(members: [Point(x: 1.0, y: 2.0)]))
        XCTAssertEqual(Shape.tryApexPoint(radius: 2.5), Point(x: 0.0, y: 2.5))
        XCTAssertNil(Shape.tryApexPoint(radius: -1.0))
    }

    func testMessageFns() {
        XCTAssertEqual(echoMessage(m: Message.text(body: "hello")), Message.text(body: "hello"))
        XCTAssertEqual(
            echoMessage(m: Message.image(url: "https://example.com/image.png", width: 640, height: 480)),
            Message.image(url: "https://example.com/image.png", width: 640, height: 480)
        )
        XCTAssertEqual(messageSummary(m: Message.text(body: "hi")), "text: hi")
        XCTAssertEqual(messageSummary(m: Message.image(url: "https://example.com/image.png", width: 640, height: 480)), "image: 640x480 at https://example.com/image.png")
        XCTAssertEqual(messageSummary(m: Message.ping), "ping")
    }

    func testAnimalFns() {
        XCTAssertEqual(echoAnimal(a: Animal.dog(name: "Rex", breed: "Labrador")), Animal.dog(name: "Rex", breed: "Labrador"))
        XCTAssertEqual(echoAnimal(a: Animal.cat(name: "Milo", indoor: true)), Animal.cat(name: "Milo", indoor: true))
        XCTAssertEqual(animalName(a: Animal.fish(count: 5)), "5 fish")
        XCTAssertEqual(animalName(a: Animal.cat(name: "Milo", indoor: true)), "Milo")
    }

    func testTaskStatusFns() {
        XCTAssertEqual(echoTaskStatus(status: .pending), .pending)
        XCTAssertEqual(echoTaskStatus(status: .inProgress(progress: 7)), .inProgress(progress: 7))
        XCTAssertEqual(echoTaskStatus(status: .failed(errorCode: -5, retryCount: 2)), .failed(errorCode: -5, retryCount: 2))
        XCTAssertEqual(getStatusProgress(status: .pending), 0)
        XCTAssertEqual(getStatusProgress(status: .inProgress(progress: 7)), 7)
        XCTAssertEqual(getStatusProgress(status: .completed(result: 9)), 9)
        XCTAssertEqual(getStatusProgress(status: .failed(errorCode: -5, retryCount: 2)), -5)
        XCTAssertFalse(isStatusComplete(status: .pending))
        XCTAssertTrue(isStatusComplete(status: .completed(result: 1)))
    }

    func testLifecycleEventFns() {
        let started = makeCriticalLifecycleEvent(id: 7)
        XCTAssertEqual(started, LifecycleEvent.taskStarted(priority: .critical, id: 7))
        XCTAssertEqual(echoLifecycleEvent(ev: started), started)
        XCTAssertEqual(echoLifecycleEvent(ev: .tick), .tick)
    }
}
