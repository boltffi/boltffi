import Demo
import XCTest

final class DataEnumTests: DemoTestCase {
    func testShapeFns() throws {
        demoCase("case:enums.data_enum.shape.should_support_primary_constructor")
        let circle = Shape(radius: 5.0)
        XCTAssertEqual(circle, Shape.circle(radius: 5.0))

        demoCase("case:enums.data_enum.shape.unit_circle.should_construct_circle")
        XCTAssertEqual(Shape.unitCircle(), Shape.circle(radius: 1.0))
        demoCase("case:enums.data_enum.shape.square.should_construct_rectangle")
        XCTAssertEqual(Shape(square: 3.0), Shape.rectangle(width: 3.0, height: 3.0))
        demoCase("case:enums.data_enum.shape.try_circle.should_return_circle_for_positive_radius")
        XCTAssertEqual(try Shape(tryCircle: 2.0), Shape.circle(radius: 2.0))

        demoCase("case:enums.data_enum.shape.should_reject_non_positive_circle_radius")
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

        demoCase("case:enums.data_enum.shape.apex.should_roundtrip_some_point_payload")
        XCTAssertEqual(echoShape(s: .apex(tip: Point(x: 3.0, y: 4.0))), .apex(tip: Point(x: 3.0, y: 4.0)))
        demoCase("case:enums.data_enum.shape.apex.should_roundtrip_none_payload")
        XCTAssertEqual(echoShape(s: .apex(tip: nil)), .apex(tip: nil))

        demoCase("case:enums.data_enum.shape.should_roundtrip_vector_record_fields")
        XCTAssertEqual(echoShape(s: .cluster(members: [Point(x: 1.0, y: 2.0)])), .cluster(members: [Point(x: 1.0, y: 2.0)]))

        demoCase("case:enums.data_enum.shape.try_apex_point.should_return_some_for_positive_radius")
        XCTAssertEqual(Shape.tryApexPoint(radius: 2.5), Point(x: 0.0, y: 2.5))
        demoCase("case:enums.data_enum.shape.try_apex_point.should_return_none_for_non_positive_radius")
        XCTAssertNil(Shape.tryApexPoint(radius: -1.0))

        demoCase("case:enums.data_enum.shape.should_roundtrip_vectors")
        XCTAssertEqual(echoVecShape(values: [.circle(radius: 2.0), .rectangle(width: 3.0, height: 4.0), .point]).count, 3)
    }

    func testMessageFns() {
        demoCase("case:enums.data_enum.message.text.should_roundtrip_string_payload")
        XCTAssertEqual(echoMessage(m: Message.text(body: "hello")), Message.text(body: "hello"))
        demoCase("case:enums.data_enum.message.image.should_roundtrip_url_dimensions_payload")
        XCTAssertEqual(
            echoMessage(m: Message.image(url: "https://example.com/image.png", width: 640, height: 480)),
            Message.image(url: "https://example.com/image.png", width: 640, height: 480)
        )
        demoCase("case:enums.data_enum.message.text.should_render_text_summary")
        XCTAssertEqual(messageSummary(m: Message.text(body: "hi")), "text: hi")
        demoCase("case:enums.data_enum.message.image.should_render_image_summary")
        XCTAssertEqual(messageSummary(m: Message.image(url: "https://example.com/image.png", width: 640, height: 480)), "image: 640x480 at https://example.com/image.png")
        demoCase("case:enums.data_enum.message.ping.should_render_ping_summary")
        XCTAssertEqual(messageSummary(m: Message.ping), "ping")
        demoCase("case:enums.data_enum.message.ping.should_roundtrip_unit_variant")
        XCTAssertEqual(echoMessage(m: Message.ping), Message.ping)
    }

    func testAnimalFns() {
        demoCase("case:enums.data_enum.animal.dog.should_roundtrip_string_payloads")
        XCTAssertEqual(echoAnimal(a: Animal.dog(name: "Rex", breed: "Labrador")), Animal.dog(name: "Rex", breed: "Labrador"))
        demoCase("case:enums.data_enum.animal.cat.should_roundtrip_name_and_bool_payload")
        XCTAssertEqual(echoAnimal(a: Animal.cat(name: "Milo", indoor: true)), Animal.cat(name: "Milo", indoor: true))
        demoCase("case:enums.data_enum.animal.fish.should_roundtrip_count_payload")
        XCTAssertEqual(echoAnimal(a: Animal.fish(count: 5)), Animal.fish(count: 5))
        demoCase("case:enums.data_enum.animal.fish.should_derive_count_label")
        XCTAssertEqual(animalName(a: Animal.fish(count: 5)), "5 fish")
        demoCase("case:enums.data_enum.animal.dog.should_derive_name")
        XCTAssertEqual(animalName(a: Animal.dog(name: "Rex", breed: "Lab")), "Rex")
        demoCase("case:enums.data_enum.animal.cat.should_derive_name")
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
        demoCase("case:enums.data_enum.lifecycle_event.should_make_critical_event")
        let started = makeCriticalLifecycleEvent(id: 7)
        XCTAssertEqual(started, LifecycleEvent.taskStarted(priority: .critical, id: 7))
        demoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_priority_payload")
        XCTAssertEqual(echoLifecycleEvent(ev: started), started)
        demoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_tick_variant")
        XCTAssertEqual(echoLifecycleEvent(ev: .tick), .tick)
    }
}
