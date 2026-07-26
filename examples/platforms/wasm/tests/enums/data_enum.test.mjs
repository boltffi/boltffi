import { assert, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:enums.data_enum.shape.should_support_primary_constructor");
  assert.deepEqual(demo.Shape.new(5), { tag: "Circle", radius: 5 });

  globalThis.demoCase("case:enums.data_enum.shape.unit_circle.should_construct_circle");
  assert.deepEqual(demo.Shape.unitCircle(), { tag: "Circle", radius: 1 });
  globalThis.demoCase("case:enums.data_enum.shape.square.should_construct_rectangle");
  assert.deepEqual(demo.Shape.square(3), { tag: "Rectangle", width: 3, height: 3 });
  globalThis.demoCase("case:enums.data_enum.shape.try_circle.should_return_circle_for_positive_radius");
  assert.deepEqual(demo.Shape.tryCircle(2), { tag: "Circle", radius: 2 });
  globalThis.demoCase("case:enums.data_enum.shape.maybe_circle.should_return_some_for_positive_radius");
  assert.deepEqual(demo.Shape.maybeCircle(2), { tag: "Circle", radius: 2 });
  globalThis.demoCase("case:enums.data_enum.shape.maybe_circle.should_return_none_for_non_positive_radius");
  assert.equal(demo.Shape.maybeCircle(0), null);

  globalThis.demoCase("case:enums.data_enum.shape.should_reject_non_positive_circle_radius");
  assertThrowsWithMessage(() => demo.Shape.tryCircle(0), "radius must be positive");

  globalThis.demoCase("case:enums.data_enum.shape.should_support_numeric_instance_methods");
  assert.equal(demo.Shape.area({ tag: "Circle", radius: 2 }), Math.PI * 4);

  globalThis.demoCase("case:enums.data_enum.shape.should_support_string_instance_methods");
  assert.equal(demo.Shape.describe({ tag: "Point" }), "point");

  globalThis.demoCase("case:enums.data_enum.shape.should_report_variant_count");
  assert.equal(demo.Shape.variantCount(), 6);

  globalThis.demoCase("case:enums.data_enum.shape.should_support_free_function_factories");
  const circle = demo.makeCircle(5);
  assert.equal(circle.tag, "Circle");
  assert.equal(circle.radius, 5);
  const rectangle = demo.makeRectangle(3, 4);
  assert.equal(rectangle.tag, "Rectangle");
  assert.equal(rectangle.width, 3);
  assert.equal(rectangle.height, 4);

  globalThis.demoCase("case:enums.data_enum.shape.should_roundtrip_core_variants");
  assert.deepEqual(demo.echoShape(demo.makeCircle(2)), demo.makeCircle(2));
  assert.deepEqual(demo.echoShape(demo.makeRectangle(3, 4)), demo.makeRectangle(3, 4));
  assert.deepEqual(
    demo.echoShape({ tag: "Triangle", a: { x: 0, y: 0 }, b: { x: 3, y: 0 }, c: { x: 0, y: 4 } }),
    { tag: "Triangle", a: { x: 0, y: 0 }, b: { x: 3, y: 0 }, c: { x: 0, y: 4 } },
  );
  assert.deepEqual(demo.echoShape({ tag: "Point" }), { tag: "Point" });

  globalThis.demoCase("case:enums.data_enum.shape.apex.should_roundtrip_some_point_payload");
  assert.deepEqual(demo.echoShape({ tag: "Apex", tip: { x: 3, y: 4 } }), { tag: "Apex", tip: { x: 3, y: 4 } });
  globalThis.demoCase("case:enums.data_enum.shape.apex.should_roundtrip_none_payload");
  assert.deepEqual(demo.echoShape({ tag: "Apex", tip: null }), { tag: "Apex", tip: null });

  globalThis.demoCase("case:enums.data_enum.shape.should_roundtrip_vector_record_fields");
  assert.deepEqual(demo.echoShape({ tag: "Cluster", members: [{ x: 1, y: 2 }] }), { tag: "Cluster", members: [{ x: 1, y: 2 }] });

  globalThis.demoCase("case:enums.data_enum.shape.try_apex_point.should_return_some_for_positive_radius");
  assert.deepEqual(demo.Shape.tryApexPoint(2.5), { x: 0, y: 2.5 });
  globalThis.demoCase("case:enums.data_enum.shape.try_apex_point.should_return_none_for_non_positive_radius");
  assert.equal(demo.Shape.tryApexPoint(-1), null);

  globalThis.demoCase("case:enums.data_enum.shape.should_roundtrip_vectors");
  assert.equal(demo.echoVecShape([demo.makeCircle(2), demo.makeRectangle(3, 4), { tag: "Point" }]).length, 3);

  globalThis.demoCase("case:enums.data_enum.message.text.should_roundtrip_string_payload");
  const textMessage = { tag: "Text", body: "hello" };
  const imageMessage = { tag: "Image", url: "https://example.com/image.png", width: 640, height: 480 };
  assert.deepEqual(demo.echoMessage(textMessage), textMessage);
  globalThis.demoCase("case:enums.data_enum.message.image.should_roundtrip_url_dimensions_payload");
  assert.deepEqual(demo.echoMessage(imageMessage), imageMessage);
  globalThis.demoCase("case:enums.data_enum.message.ping.should_roundtrip_unit_variant");
  assert.deepEqual(demo.echoMessage({ tag: "Ping" }), { tag: "Ping" });
  globalThis.demoCase("case:enums.data_enum.message.text.should_render_text_summary");
  assert.equal(demo.messageSummary({ tag: "Text", body: "hi" }), "text: hi");
  globalThis.demoCase("case:enums.data_enum.message.image.should_render_image_summary");
  assert.equal(
    demo.messageSummary(imageMessage),
    "image: 640x480 at https://example.com/image.png",
  );
  globalThis.demoCase("case:enums.data_enum.message.ping.should_render_ping_summary");
  assert.equal(demo.messageSummary({ tag: "Ping" }), "ping");

  globalThis.demoCase("case:enums.data_enum.animal.dog.should_roundtrip_string_payloads");
  const dog = { tag: "Dog", name: "Rex", breed: "Labrador" };
  const cat = { tag: "Cat", name: "Milo", indoor: true };
  assert.deepEqual(demo.echoAnimal(dog), dog);
  globalThis.demoCase("case:enums.data_enum.animal.cat.should_roundtrip_name_and_bool_payload");
  assert.deepEqual(demo.echoAnimal(cat), cat);
  globalThis.demoCase("case:enums.data_enum.animal.fish.should_roundtrip_count_payload");
  const fish = { tag: "Fish", count: 5 };
  assert.deepEqual(demo.echoAnimal(fish), fish);
  globalThis.demoCase("case:enums.data_enum.animal.fish.should_derive_count_label");
  assert.equal(demo.animalName(fish), "5 fish");
  globalThis.demoCase("case:enums.data_enum.animal.dog.should_derive_name");
  assert.equal(demo.animalName(dog), "Rex");
  globalThis.demoCase("case:enums.data_enum.animal.cat.should_derive_name");
  assert.equal(demo.animalName(cat), "Milo");

  globalThis.demoCase("case:enums.data_enum.lifecycle_event.should_make_critical_event");
  const started = demo.makeCriticalLifecycleEvent(7n);
  assert.equal(started.tag, "TaskStarted");
  assert.equal(started.priority, demo.Priority.Critical);
  assert.equal(started.id, 7n);
  globalThis.demoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_priority_payload");
  assert.deepEqual(demo.echoLifecycleEvent(started), started);
  globalThis.demoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_tick_variant");
  assert.deepEqual(demo.echoLifecycleEvent({ tag: "Tick" }), { tag: "Tick" });
}
