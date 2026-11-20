import { assert, assertPoint, assertThrowsWithMessage, demo, sampleMixedRecord } from "../support/index.mjs";

export async function run() {
  const counter = demo.Counter.new(2);
  assert.equal(counter.get(), 2);
  counter.increment();
  assert.equal(counter.get(), 3);
  globalThis.demoCase("case:classes.methods.counter.decrement.should_call_a_method_declared_in_a_methods_block");
  counter.decrement();
  assert.equal(counter.get(), 2);
  counter.increment();
  counter.add(7);
  assert.equal(counter.get(), 10);
  assert.equal(counter.tryGetPositive(), 10);
  assert.equal(counter.maybeDouble(), 20);
  assertPoint(counter.asPoint(), { x: 10, y: 0 });
  counter.reset();
  assert.equal(counter.get(), 0);
  assert.equal(counter.maybeDouble(), null);
  assertThrowsWithMessage(() => counter.tryGetPositive(), "count is not positive");
  counter.dispose();

  const service = demo.MixedRecordService.new("records");
  const record = sampleMixedRecord();
  assert.equal(service.getLabel(), "records");
  assert.equal(service.storedCount(), 0);
  assert.deepEqual(service.echoRecord(record), record);
  assert.deepEqual(
    service.storeRecordParts(
      record.name,
      record.anchor,
      record.priority,
      record.shape,
      record.parameters,
    ),
    record,
  );
  assert.equal(service.storedCount(), 1);
  assert.deepEqual(await service.asyncEchoRecord(record), record);
  assert.deepEqual(
    await service.asyncStoreRecordParts(
      record.name,
      record.anchor,
      record.priority,
      record.shape,
      record.parameters,
    ),
    record,
  );
  assert.equal(service.storedCount(), 2);
  service.dispose();
}
