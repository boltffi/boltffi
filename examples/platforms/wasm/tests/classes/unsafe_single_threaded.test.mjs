import { assert, assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  const holder = demo.StateHolder.new("local");
  assert.equal(holder.getLabel(), "local");
  assert.equal(holder.getValue(), 0);
  holder.setValue(5);
  assert.equal(holder.getValue(), 5);
  assert.equal(holder.increment(), 6);
  holder.addItem("a");
  holder.addItem("b");
  assert.equal(holder.itemCount(), 2);
  assertArrayEqual(holder.getItems(), ["a", "b"]);
  assert.equal(holder.removeLast(), "b");
  assert.equal(holder.transformValue((value) => Math.trunc(value / 2)), 3);
  assert.equal(holder.applyValueCallback({ onValue: (value) => value + 3 }), 6);
  assert.equal(await holder.asyncGetValue(), 6);
  await holder.asyncSetValue(9);
  assert.equal(holder.getValue(), 9);
  assert.equal(await holder.asyncAddItem("z"), 2);
  assertArrayEqual(holder.getItems(), ["a", "z"]);
  holder.clear();
  assert.equal(holder.getValue(), 0);
  assertArrayEqual(holder.getItems(), []);
  holder.dispose();

  globalThis.demoCase("case:classes.unsafe_single_threaded.map_view.add_marker.should_return_single_threaded_marker_handle");
  const mapView = demo.MapView.new();
  const marker = mapView.addMarker({ id: 7, title: "harbor" });
  assert.equal(marker.id(), 7);
  assert.equal(marker.title(), "harbor");
  assert.equal(mapView.markerCount(), 1);
  marker.dispose();
  mapView.dispose();

  const counter = demo.CounterSingleThreaded.new();
  assert.equal(counter.get(), 0);
  counter.set(4);
  counter.increment();
  assert.equal(counter.get(), 5);
  counter.dispose();

  const accumulator = demo.AccumulatorSingleThreaded.new();
  accumulator.add(7n);
  accumulator.add(5n);
  assert.equal(accumulator.get(), 12n);
  accumulator.reset();
  assert.equal(accumulator.get(), 0n);
  accumulator.dispose();
}
