import { assert, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.demoEnabled, true);
  assert.equal(demo.demoAnswer, 42);
  assert.equal(demo.demoLarge, 9_007_199_254_740_993n);
  assert.equal(demo.demoHalf, 0.5);
  assert.equal(demo.demoLabel, "boltffi");
  assert.deepEqual(Array.from(demo.demoBytes), [102, 102, 105]);
  assert.equal(demo.demoMode, demo.DemoMode.Fast);
  assert.deepEqual(demo.demoIdle, { tag: "Idle" });
  assert.equal(demo.demoAlias, "boltffi");
  assert.equal(demo.demoComputed, 42);
  assert.deepEqual(demo.demoPair, [3, 5]);
  assert.deepEqual(demo.demoBusy, { tag: "Busy", jobs: 3 });
  globalThis.demoCase("case:constants.values.should_expose_inline_and_accessor_values");
}
