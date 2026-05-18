import {
  assert,
  assertArrayEqual,
  assertRejectsWithMessage,
  demo,
  sampleMixedRecord,
} from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:async_fns.basic.add.should_return_sum");
  assert.equal(await demo.asyncAdd(3, 7), 10);
  globalThis.demoCase("case:async_fns.basic.echo.should_prefix_message");
  assert.equal(await demo.asyncEcho("hello async"), "Echo: hello async");
  globalThis.demoCase("case:async_fns.basic.double_all.should_double_i32_vector");
  assertArrayEqual(await demo.asyncDoubleAll([1, 2, 3]), [2, 4, 6]);
  globalThis.demoCase("case:async_fns.basic.find_positive.should_return_first_positive");
  assert.equal(await demo.asyncFindPositive([-1, 0, 5, 3]), 5);
  globalThis.demoCase("case:async_fns.basic.find_positive.should_return_none_for_all_negative");
  assert.equal(await demo.asyncFindPositive([-1, -2, -3]), null);
  globalThis.demoCase("case:async_fns.basic.concat.should_join_string_vector");
  assert.equal(await demo.asyncConcat(["a", "b", "c"]), "a, b, c");
  globalThis.demoCase("case:async_fns.results.try_compute.should_return_doubled_value");
  assert.equal(await demo.tryComputeAsync(4), 8);
  globalThis.demoCase("case:async_fns.results.try_compute.should_return_overflow_for_negative_value");
  try {
    await demo.tryComputeAsync(-1);
    assert.fail("expected tryComputeAsync(-1) to reject");
  } catch (error) {
    assert.ok(error instanceof demo.ComputeErrorException);
    assert.deepEqual(error.value, { tag: "Overflow", value: -1, limit: 0 });
  }
  globalThis.demoCase("case:async_fns.results.try_compute.should_return_invalid_input_for_zero");
  try {
    await demo.tryComputeAsync(0);
    assert.fail("expected tryComputeAsync(0) to reject");
  } catch (error) {
    assert.ok(error instanceof demo.ComputeErrorException);
    assert.deepEqual(error.value, { tag: "InvalidInput", value0: -999 });
  }
  globalThis.demoCase("case:async_fns.basic.get_numbers.should_return_counting_sequence");
  assertArrayEqual(await demo.asyncGetNumbers(4), [0, 1, 2, 3]);
  globalThis.demoCase("case:async_fns.results.fetch_data.should_return_scaled_positive_id");
  assert.equal(await demo.fetchData(7), 70);
  globalThis.demoCase("case:async_fns.results.fetch_data.should_reject_non_positive_id");
  await assertRejectsWithMessage(() => demo.fetchData(0), "invalid id");
  globalThis.demoCase("case:async_fns.mixed_record.echo.should_roundtrip_record");
  const record = sampleMixedRecord();
  assert.deepEqual(await demo.asyncEchoMixedRecord(record), record);
  globalThis.demoCase("case:async_fns.mixed_record.make.should_construct_record");
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
