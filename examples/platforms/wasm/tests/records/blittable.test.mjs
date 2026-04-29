import { assert, assertPoint, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  assertPoint(demo.echoPoint({ x: 1, y: 2 }), { x: 1, y: 2 });
  assertPoint(demo.tryMakePoint(2, 3), { x: 2, y: 3 });
  assert.equal(demo.tryMakePoint(0, 0), null);
  assertPoint(demo.makePoint(1, 2), { x: 1, y: 2 });
  assertPoint(demo.addPoints({ x: 3, y: 4 }, { x: 5, y: 6 }), { x: 8, y: 10 });
  assertPoint(demo.Point.new(1, 2), { x: 1, y: 2 });
  assertPoint(demo.Point.origin(), { x: 0, y: 0 });
  assertPoint(demo.Point.fromPolar(2, Math.PI / 2), { x: 0, y: 2 }, 1e-9);
  assertPoint(demo.Point.tryUnit(3, 4), { x: 0.6, y: 0.8 });
  assertThrowsWithMessage(() => demo.Point.tryUnit(0, 0), "cannot normalize zero vector");
  assert.equal(demo.Point.checkedUnit(0, 0), null);
  assertPoint(demo.Point.checkedUnit(3, 4), { x: 0.6, y: 0.8 });
  assert.equal(demo.Point.distance({ x: 3, y: 4 }), 5);
  assertPoint(demo.Point.scale({ x: 3, y: 4 }, 2), { x: 6, y: 8 });
  assertPoint(demo.Point.add({ x: 3, y: 4 }, { x: 5, y: 6 }), { x: 8, y: 10 });
  assert.equal(demo.Point.pathLength([{ x: 0, y: 0 }, { x: 3, y: 4 }, { x: 6, y: 8 }]), 10);
  assert.equal(demo.Point.dimensions(), 2);

  const color = { r: 1, g: 2, b: 3, a: 255 };
  assert.deepEqual(demo.echoColor(color), color);
  assert.deepEqual(demo.makeColor(9, 8, 7, 6), { r: 9, g: 8, b: 7, a: 6 });
}
