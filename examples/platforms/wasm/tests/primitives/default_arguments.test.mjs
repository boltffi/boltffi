import { assert, demo } from "../support/index.mjs";

// TypeScript renders no parameter defaults yet (the demo cases exclude it), so
// every argument is passed here; the values match the Rust defaults.
export async function run() {
  assert.equal(demo.repeatGreeting("ada", "hello", 2, false), "hello ada, hello ada");
  assert.equal(demo.describeLimit(null, 7), "none:7");
  assert.equal(demo.applyOptionalCallback(21, null), 21);
  assert.equal(demo.applyOptionalCallback(21, { onValue: (value) => value * 2 }), 42);

  const counter = demo.DefaultedCounter.new(10);
  try {
    assert.equal(counter.offset(1), 11);
    assert.equal(await counter.asyncOffset(3), 13);
  } finally {
    counter.dispose();
  }

  const offsetCounter = demo.DefaultedCounter.withOffset(20, 3);
  try {
    assert.equal(offsetCounter.offset(1), 24);
  } finally {
    offsetCounter.dispose();
  }

  const started = await demo.DefaultedCounter.start(30, null, { onValue: (value) => value * 2 });
  try {
    assert.equal(started.offset(1), 61);
  } finally {
    started.dispose();
  }

  assert.deepEqual(
    demo.DefaultedCounter.integerLimits(-9223372036854775808n, 18446744073709551615n),
    { lower: -9223372036854775808n, upper: 18446744073709551615n },
  );
  assert.equal(demo.scaleDefault(0.5, 1.5, demo.DefaultMode.Quiet), 0.75);
  assert.equal(demo.scaleDefault(2, 4, demo.DefaultMode.Loud), 16);
  assert.equal(demo.scaleDefault(0.5, null, demo.DefaultMode.Quiet), 0.5);
  assert.equal(demo.DefaultMode.matches(demo.DefaultMode.Quiet, demo.DefaultMode.Quiet), true);
  assert.equal(demo.DefaultMode.matches(demo.DefaultMode.Loud, demo.DefaultMode.Quiet), false);
  assert.equal(await demo.DefaultMode.load(demo.DefaultMode.Loud), demo.DefaultMode.Loud);
  assert.equal(await demo.asyncDefault(9), 9);
  assert.equal(demo.DefaultAmount.offset({ value: 3 }, 2), 5);
  assert.deepEqual(demo.NamedAmount.withValue(5), { value: 5 });
  assert.deepEqual(await demo.NamedAmount.load(6), { value: 6 });
  assert.equal(demo.defaultLimit(null), null);
  assert.equal(demo.defaultLimit(7), 7);
  assert.equal(demo.defaultTimeoutSeconds({ seconds: 1.5 }), 1.5);
  assert.equal(demo.defaultTimeoutSeconds({ seconds: 2.5 }), 2.5);
}
