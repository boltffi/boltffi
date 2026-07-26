import { assert, demo } from "../support/index.mjs";

export async function run() {
  const bus = demo.EventBus.new();

  const values = bus.subscribeValues()[Symbol.asyncIterator]();
  const nextValue = values.next();
  bus.emitValue(1);
  assert.deepEqual(await nextValue, { value: 1, done: false });
  await values.return();

  const points = bus.subscribePoints()[Symbol.asyncIterator]();
  const nextPoint = points.next();
  bus.emitPoint({ x: 1, y: 2 });
  assert.deepEqual(await nextPoint, { value: { x: 1, y: 2 }, done: false });
  await points.return();

  const messages = bus.subscribeMessages()[Symbol.asyncIterator]();
  const nextMessage = messages.next();
  const secondMessage = messages.next();
  bus.emitMessage({ text: "alpha", values: [1, 2] });
  bus.emitMessage({ text: "beta", values: [3, 4] });
  const message = await nextMessage;
  assert.equal(message.done, false);
  assert.equal(message.value.text, "alpha");
  assert.deepEqual(Array.from(message.value.values), [1, 2]);
  const second = await secondMessage;
  assert.equal(second.done, false);
  assert.equal(second.value.text, "beta");
  assert.deepEqual(Array.from(second.value.values), [3, 4]);
  globalThis.demoCase("case:classes.streams.event_bus.subscribe_messages.should_deliver_encoded_record_items");
  await messages.return();

  const batch = bus.subscribeValuesBatch();
  assert.equal(bus.emitBatch([2, 3, 4]), 3);
  assert.deepEqual(batch.popBatch(), [2, 3, 4]);
  batch.dispose();

  const callbackValue = new Promise((resolve) => {
    const cancellable = bus.subscribeValuesCallback((value) => {
      cancellable.cancel();
      resolve(value);
    });
  });
  bus.emitValue(5);
  assert.equal(await callbackValue, 5);

  bus.dispose();
}
