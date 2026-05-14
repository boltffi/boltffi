import {
  assert,
  assertArrayEqual,
  assertRejectsWithMessage,
  demo,
  sampleMixedRecord,
} from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:async_fns.basic.scalar_string_vec");
  assert.equal(await demo.asyncAdd(3, 7), 10);
  assert.equal(await demo.asyncEcho("hello async"), "Echo: hello async");
  assertArrayEqual(await demo.asyncDoubleAll([1, 2, 3]), [2, 4, 6]);
  assert.equal(await demo.asyncFindPositive([-1, 0, 5, 3]), 5);
  assert.equal(await demo.asyncFindPositive([-1, -2, -3]), null);
  assert.equal(await demo.asyncConcat(["a", "b", "c"]), "a, b, c");
  globalThis.demoCase("case:async_fns.results.compute_fetch");
  assert.equal(await demo.tryComputeAsync(4), 8);
  try {
    await demo.tryComputeAsync(-1);
    assert.fail("expected tryComputeAsync(-1) to reject");
  } catch (error) {
    assert.ok(error instanceof demo.ComputeErrorException);
    assert.deepEqual(error.value, { tag: "Overflow", value: -1, limit: 0 });
  }
  assert.equal(await demo.fetchData(7), 70);
  await assertRejectsWithMessage(() => demo.fetchData(0), "invalid id");
  globalThis.demoCase("case:async_fns.mixed_record.roundtrip");
  const record = sampleMixedRecord();
  assert.deepEqual(await demo.asyncEchoMixedRecord(record), record);
  assert.deepEqual(
    await demo.asyncMakeMixedRecord(
      record.name,
      record.anchor,
      record.priority,
      record.shape,
      record.parameters,
    ),
    record,
  );
}
