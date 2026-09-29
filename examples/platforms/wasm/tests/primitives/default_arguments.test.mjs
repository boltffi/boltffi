import { assert, demo } from "../support/index.mjs";

// TypeScript renders no parameter defaults yet (the demo cases exclude it), so
// every argument is passed here; the values match the Rust defaults.
export async function run() {
  assert.equal(demo.repeatGreeting("ada", "hello", 2, false), "hello ada, hello ada");
  assert.equal(demo.describeLimit(null, 7), "none:7");
  assert.equal(demo.applyOptionalCallback(21, null), 21);
  assert.equal(demo.applyOptionalCallback(21, { onValue: (value) => value * 2 }), 42);

  const counter = demo.DefaultedCounter.new(10);
  assert.equal(counter.offset(1), 11);
  counter.dispose();
}
