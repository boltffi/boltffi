import Demo
import XCTest

final class DataEnumTests: DemoTestCase {
    func testShapeFns() throws {
        demoCase("case:enums.data_enum.shape.should_support_primary_constructor")
        let circle = Shape(radius: 5.0)
        XCTAssertEqual(circle, Shape.circle(radius: 5.0))

        demoCase("case:enums.data_enum.shape.should_support_static_constructors")
        XCTAssertEqual(Shape.unitCircle(), Shape.circle(radius: 1.0))
        XCTAssertEqual(Shape(square: 3.0), Shape.rectangle(width: 3.0, height: 3.0))
        XCTAssertEqual(try Shape(tryCircle: 2.0), Shape.circle(radius: 2.0))

        demoCase("case:enums.data_enum.shape.should_reject_invalid_circle_constructor_input")
        assertThrowsMessageContains("radius must be positive", try Shape(tryCircle: -1.0))

        demoCase("case:enums.data_enum.shape.should_report_variant_count")
        XCTAssertEqual(Shape.variantCount(), 6)

        demoCase("case:enums.data_enum.shape.should_support_numeric_instance_methods")
        XCTAssertEqual(circle.area(), Double.pi * 25.0, accuracy: 1e-6)

        demoCase("case:enums.data_enum.shape.should_support_string_instance_methods")
        XCTAssertEqual(circle.describe(), "circle r=5")

        demoCase("case:enums.data_enum.shape.should_support_free_function_factories")
        XCTAssertEqual(makeCircle(radius: 2.0), .circle(radius: 2.0))
        XCTAssertEqual(makeRectangle(width: 3.0, height: 4.0), .rectangle(width: 3.0, height: 4.0))

        demoCase("case:enums.data_enum.shape.should_roundtrip_core_variants")
        XCTAssertEqual(echoShape(s: .circle(radius: 2.0)), .circle(radius: 2.0))
        XCTAssertEqual(echoShape(s: .rectangle(width: 3.0, height: 4.0)), .rectangle(width: 3.0, height: 4.0))
        XCTAssertEqual(
            echoShape(s: .triangle(a: Point(x: 0.0, y: 0.0), b: Point(x: 3.0, y: 0.0), c: Point(x: 0.0, y: 4.0))),
            .triangle(a: Point(x: 0.0, y: 0.0), b: Point(x: 3.0, y: 0.0), c: Point(x: 0.0, y: 4.0))
        )
        XCTAssertEqual(echoShape(s: .point), .point)

        demoCase("case:enums.data_enum.shape.should_roundtrip_optional_record_fields")
        XCTAssertEqual(echoShape(s: .apex(tip: Point(x: 3.0, y: 4.0))), .apex(tip: Point(x: 3.0, y: 4.0)))
        XCTAssertEqual(echoShape(s: .apex(tip: nil)), .apex(tip: nil))

        demoCase("case:enums.data_enum.shape.should_roundtrip_vector_record_fields")
        XCTAssertEqual(echoShape(s: .cluster(members: [Point(x: 1.0, y: 2.0)])), .cluster(members: [Point(x: 1.0, y: 2.0)]))

        demoCase("case:enums.data_enum.shape.should_return_optional_records_from_static_methods")
        XCTAssertEqual(Shape.tryApexPoint(radius: 2.5), Point(x: 0.0, y: 2.5))
        XCTAssertNil(Shape.tryApexPoint(radius: -1.0))

        demoCase("case:enums.data_enum.shape.should_roundtrip_vectors")
        XCTAssertEqual(echoVecShape(values: [.circle(radius: 2.0), .rectangle(width: 3.0, height: 4.0), .point]).count, 3)
    }

    func testMessageFns() {
        demoCase("case:enums.data_enum.message.basic")
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
        demoCase("case:enums.data_enum.animal.basic")
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
        demoCase("case:enums.data_enum.lifecycle_event.priority_payload")
        let started = makeCriticalLifecycleEvent(id: 7)
        XCTAssertEqual(started, LifecycleEvent.taskStarted(priority: .critical, id: 7))
        XCTAssertEqual(echoLifecycleEvent(ev: started), started)
        XCTAssertEqual(echoLifecycleEvent(ev: .tick), .tick)
    }
}
