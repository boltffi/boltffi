import { assert, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  const circle = demo.makeCircle(5);
  assert.equal(circle.tag, "Circle");
  assert.equal(circle.radius, 5);
  assert.deepEqual(demo.Shape.new(5), { tag: "Circle", radius: 5 });
  assert.deepEqual(demo.Shape.unitCircle(), { tag: "Circle", radius: 1 });
  assert.deepEqual(demo.Shape.square(3), { tag: "Rectangle", width: 3, height: 3 });
  assert.deepEqual(demo.Shape.tryCircle(2), { tag: "Circle", radius: 2 });
  assertThrowsWithMessage(() => demo.Shape.tryCircle(0), "radius must be positive");
  assert.equal(demo.Shape.area({ tag: "Circle", radius: 2 }), Math.PI * 4);
  assert.equal(demo.Shape.describe({ tag: "Point" }), "point");
  assert.equal(demo.Shape.variantCount(), 6);

  const rectangle = demo.makeRectangle(3, 4);
  assert.equal(rectangle.tag, "Rectangle");
  assert.equal(rectangle.width, 3);
  assert.equal(rectangle.height, 4);

  assert.deepEqual(demo.echoShape(demo.makeCircle(2)), demo.makeCircle(2));
  assert.deepEqual(demo.echoShape(demo.makeRectangle(3, 4)), demo.makeRectangle(3, 4));
  assert.equal(demo.echoVecShape([demo.makeCircle(2), demo.makeRectangle(3, 4), { tag: "Point" }]).length, 3);
  assert.deepEqual(demo.Shape.tryApexPoint(2.5), { x: 0, y: 2.5 });
  assert.equal(demo.Shape.tryApexPoint(-1), null);
  assert.deepEqual(demo.echoShape({ tag: "Apex", tip: { x: 3, y: 4 } }), { tag: "Apex", tip: { x: 3, y: 4 } });
  assert.deepEqual(demo.echoShape({ tag: "Apex", tip: null }), { tag: "Apex", tip: null });
  assert.deepEqual(demo.echoShape({ tag: "Cluster", members: [{ x: 1, y: 2 }] }), { tag: "Cluster", members: [{ x: 1, y: 2 }] });

  const textMessage = { tag: "Text", body: "hello" };
  const imageMessage = { tag: "Image", url: "https://example.com/image.png", width: 640, height: 480 };
  assert.deepEqual(demo.echoMessage(textMessage), textMessage);
  assert.deepEqual(demo.echoMessage(imageMessage), imageMessage);
  assert.equal(demo.messageSummary({ tag: "Text", body: "hi" }), "text: hi");
  assert.equal(
    demo.messageSummary(imageMessage),
    "image: 640x480 at https://example.com/image.png",
  );
  assert.equal(demo.messageSummary({ tag: "Ping" }), "ping");

  const dog = { tag: "Dog", name: "Rex", breed: "Labrador" };
  const cat = { tag: "Cat", name: "Milo", indoor: true };
  assert.deepEqual(demo.echoAnimal(dog), dog);
  assert.deepEqual(demo.echoAnimal(cat), cat);
  assert.equal(demo.animalName({ tag: "Fish", count: 5 }), "5 fish");
  assert.equal(demo.animalName(cat), "Milo");

  const started = demo.makeCriticalLifecycleEvent(7n);
  assert.equal(started.tag, "TaskStarted");
  assert.equal(started.priority, demo.Priority.Critical);
  assert.equal(started.id, 7n);
  assert.deepEqual(demo.echoLifecycleEvent(started), started);
  assert.deepEqual(demo.echoLifecycleEvent({ tag: "Tick" }), { tag: "Tick" });
}
