import { assert, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  demoCase("case:classes.close_guard.guarded_counter.increment.should_reject_calls_after_close");
  const counter = demo.GuardedCounter.new(1);
  assert.equal(counter.increment(), 2);
  counter.dispose();
  assertThrowsWithMessage(() => counter.increment(), "GuardedCounter has been disposed");

  const gated = demo.GuardedCounter.new(10);
  assert.equal(gated.incrementThroughGate((observed) => observed + 5), 25);
  gated.dispose();
}
