import { assert, assertArrayEqual, assertRejectsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  const worker = demo.AsyncWorker.new("test");
  assert.equal(worker.getPrefix(), "test");
  assert.equal(await worker.process("data"), "test: data");
  assert.equal(await worker.tryProcess("data"), "test: data");
  await assertRejectsWithMessage(() => worker.tryProcess(""), "input must not be empty");
  assert.equal(await worker.findItem(42), "test_42");
  assert.equal(await worker.findItem(-1), null);
  assertArrayEqual(await worker.processBatch(["x", "y"]), ["test: x", "test: y"]);
  // Unlike every other method here, this one genuinely suspends across
  // several polls instead of resolving on the first one -- real regression
  // coverage for a genuine Pending/wake/re-poll cycle on an async class
  // method.
  assert.equal(await worker.processAfterPolls("data", 50), "test: data");

  await assertPreAbortedSignalRejectsImmediately(worker);
  await assertMidFlightCancelRejects(worker);
  assert.equal(
    await worker.process("still works", { signal: new AbortController().signal }),
    "test: still works"
  );

  worker.dispose();
}

async function assertPreAbortedSignalRejectsImmediately(worker) {
  const controller = new AbortController();
  controller.abort();
  await assertRejectsWithMessage(
    () => worker.processAfterPolls("never-seen", 5, { signal: controller.signal }),
    "cancelled"
  );
}

async function assertMidFlightCancelRejects(worker) {
  const controller = new AbortController();
  const pending = worker.processAfterPolls("never-seen", 1_000_000, {
    signal: controller.signal,
  });
  controller.abort();
  await assertRejectsWithMessage(() => pending, "cancelled");
}
