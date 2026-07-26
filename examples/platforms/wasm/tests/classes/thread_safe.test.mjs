import { assert, demo } from "../support/index.mjs";

export async function run() {
  const counter = demo.SharedCounter.new(5);
  assert.equal(counter.get(), 5);
  counter.set(6);
  assert.equal(counter.get(), 6);
  assert.equal(counter.increment(), 7);
  assert.equal(counter.add(3), 10);
  assert.equal(await counter.asyncGet(), 10);
  assert.equal(await counter.asyncAdd(5), 15);
  counter.dispose();

  const emptyStore = demo.DataStore.new();
  assert.equal(emptyStore.isEmpty(), true);
  assert.equal(emptyStore.len(), 0);
  emptyStore.addParts(1, 2, 3n);
  emptyStore.add({ x: 4, y: 5, timestamp: 6n });
  assert.equal(emptyStore.isEmpty(), false);
  assert.equal(emptyStore.len(), 2);
  assert.equal(emptyStore.sum(), 12);
  const visitedPoints = [];
  emptyStore.foreach((point) => {
    visitedPoints.push(point);
  });
  assert.deepEqual(visitedPoints, [
    { x: 1, y: 2, timestamp: 3n },
    { x: 4, y: 5, timestamp: 6n },
  ]);
  assert.equal(await emptyStore.asyncSum(), 12);
  assert.equal(await emptyStore.asyncLen(), 2);
  emptyStore.dispose();

  const sampledStore = demo.DataStore.withSampleData();
  assert.equal(sampledStore.len(), 3);
  sampledStore.dispose();

  const capacityStore = demo.DataStore.withCapacity(8);
  assert.equal(capacityStore.len(), 0);
  capacityStore.dispose();

  const seededStore = demo.DataStore.withInitialPoint(7, 8, 9n);
  assert.equal(seededStore.sum(), 15);
  seededStore.dispose();

  if (demo.Accumulator) {
    const accumulator = demo.Accumulator.new();
    accumulator.add(4n);
    accumulator.add(6n);
    assert.equal(accumulator.get(), 10n);
    accumulator.reset();
    assert.equal(accumulator.get(), 0n);
    accumulator.dispose();
  }
}
