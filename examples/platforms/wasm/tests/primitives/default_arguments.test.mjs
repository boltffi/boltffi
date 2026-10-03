import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults");
  assert.equal(demo.repeatGreeting("ada"), "hello ada, hello ada");
  assert.equal(demo.repeatGreeting("ada", undefined, undefined, true), "HELLO ADA, HELLO ADA");
  assert.equal(demo.repeatGreeting("ada", "hi", 1, false), "hi ada");
  assert.equal(demo.repeatGreeting("ada", "", 0, false), "");

  globalThis.demoCase("case:primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals");
  assert.equal(demo.describeLimit(), "none:7");
  assert.equal(demo.describeLimit("daily"), "daily:7");
  assert.equal(demo.describeLimit(undefined, null), "none:unlimited");
  assert.equal(demo.describeLimit("", 0), ":0");

  globalThis.demoCase("case:primitives.default_arguments.should_default_an_optional_callback_to_none");
  assert.equal(demo.applyOptionalCallback(21), 21);
  assert.equal(demo.applyOptionalCallback(21, null), 21);
  assert.equal(demo.applyOptionalCallback(21, { onValue: (value) => value * 2 }), 42);
  assert.equal(demo.applyOptionalClosure(21), 21);
  assert.equal(demo.applyOptionalClosure(21, undefined), 21);
  assert.equal(demo.applyOptionalClosure(21, null), 21);
  assert.equal(demo.applyOptionalClosure(21, (value) => value * 2), 42);

  globalThis.demoCase("case:primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults");
  const counter = demo.DefaultedCounter.new();
  try {
    assert.equal(counter.offset(), 11);
    assert.equal(counter.offset(3), 13);
    assert.equal(counter.offset(0), 10);
  } finally {
    counter.dispose();
  }

  const explicitCounter = demo.DefaultedCounter.new(5);
  try {
    assert.equal(explicitCounter.offset(3), 8);
  } finally {
    explicitCounter.dispose();
  }

  const offsetCounter = demo.DefaultedCounter.withOffset(undefined, 3);
  try {
    assert.equal(offsetCounter.offset(), 24);
  } finally {
    offsetCounter.dispose();
  }

  const textCounter = demo.DefaultedCounter.fromText();
  try {
    assert.equal(textCounter.offset(), 41);
  } finally {
    textCounter.dispose();
  }

  const wideCounter = demo.DefaultedWideCounter.new();
  try {
    assert.equal(wideCounter.value(), 10n);
  } finally {
    wideCounter.dispose();
  }

  [0n, 5n, -9223372036854775808n, 9223372036854775807n].forEach((value) => {
    const suppliedCounter = demo.DefaultedWideCounter.new(value);
    try {
      assert.equal(suppliedCounter.value(), value);
    } finally {
      suppliedCounter.dispose();
    }
  });

  const originalWideCounter = demo.DefaultedWideCounter.new(20n);
  try {
    const adjustedCounter = demo.DefaultedWideCounter.withOffset(originalWideCounter);
    try {
      assert.equal(adjustedCounter.value(), 21n);
      assert.throws(() => originalWideCounter.value(), /disposed/);
    } finally {
      adjustedCounter.dispose();
    }
  } finally {
    originalWideCounter.dispose();
  }

  assert.deepEqual(
    demo.DefaultedCounter.integerLimits(),
    { lower: -9223372036854775808n, upper: 18446744073709551615n },
  );

  globalThis.demoCase("case:primitives.default_arguments.should_apply_async_defaults");
  const started = await demo.DefaultedCounter.start();
  try {
    assert.equal(await started.asyncOffset(), 33);
    assert.equal(await started.asyncOffset(0), 30);
  } finally {
    started.dispose();
  }

  const callbackCounter = await demo.DefaultedCounter.start(undefined, undefined, { onValue: (value) => value * 2 });
  try {
    assert.equal(callbackCounter.offset(), 61);
  } finally {
    callbackCounter.dispose();
  }

  assert.equal(await demo.asyncDefault(), 9);
  assert.equal(await demo.asyncDefault(undefined, { signal: new AbortController().signal }), 9);
  await assert.rejects(demo.asyncDefault(undefined, { signal: AbortSignal.abort() }), { name: "BoltFFICancelledError" });
  assert.equal(await demo.asyncDefault(0), 0);
  assert.equal(await demo.asyncDefault(0x80000000), 0x80000000);
  assert.equal(await demo.asyncDefault(0xffffffff), 0xffffffff);

  globalThis.demoCase("case:primitives.default_arguments.should_apply_float_and_enum_defaults");
  assert.equal(demo.scaleDefault(), 0.75);
  assert.equal(demo.scaleDefault(2, 4, demo.DefaultMode.Loud), 16);
  assert.equal(demo.scaleDefault(undefined, null), 0.5);
  assert.equal(demo.chooseDefault(), 0);
  assert.equal(demo.chooseDefault({ tag: "Value", value0: 7 }), 7);
  assert.equal(demo.chooseDefault(undefined, { tag: "Value", value0: 7 }), -7);
  assert.equal(demo.chooseDefault(undefined, null), 0);
  assert.equal(demo.defaultFloatBits(), 0x80000000);
  assert.equal(demo.defaultFloatBits(0), 0);
  assert.equal(demo.defaultDoubleBits(), 0x8000000000000000n);
  assert.equal(demo.defaultDoubleBits(0), 0n);
  assert.equal(demo.DefaultMode.matches(demo.DefaultMode.Quiet), true);
  assert.equal(demo.DefaultMode.matches(demo.DefaultMode.Loud), false);
  assert.equal(demo.DefaultMode.matches(demo.DefaultMode.Loud, demo.DefaultMode.Loud), true);
  assert.equal(await demo.DefaultMode.load(), demo.DefaultMode.Quiet);
  assert.equal(await demo.DefaultMode.load(demo.DefaultMode.Loud), demo.DefaultMode.Loud);

  globalThis.demoCase("case:primitives.default_arguments.should_apply_record_defaults");
  assert.equal(demo.DefaultAmount.offset({}), 5);
  assert.equal(demo.DefaultAmount.offset({ value: 4 }, 3), 7);
  assert.deepEqual(demo.DefaultAmount.withScaledValue(), { value: 4 });
  assert.deepEqual(demo.DefaultAmount.withScaledValue(3), { value: 6 });
  assert.deepEqual(demo.DefaultAmount.tryScaledValue(), { value: 4 });
  assert.equal(demo.DefaultAmount.tryScaledValue(-1), null);
  assert.deepEqual(demo.NamedAmount.withValue(), { value: 5 });
  assert.deepEqual(demo.NamedAmount.withValue(8), { value: 8 });
  assert.deepEqual(await demo.NamedAmount.load(), { value: 6 });
  assert.deepEqual(await demo.NamedAmount.load(8), { value: 8 });

  globalThis.demoCase("case:primitives.default_arguments.should_apply_custom_type_defaults");
  assert.equal(demo.defaultLimit(), null);
  assert.equal(demo.defaultLimit(7), 7);
  assert.equal(demo.defaultTimeoutSeconds(), 1.5);
  assert.equal(demo.defaultTimeoutSeconds({ seconds: 2.5 }), 2.5);
  assert.equal(demo.defaultEmail(), "mailto:ada@example.com");
  assert.equal(demo.defaultEmail("mailto:grace@example.com"), "mailto:grace@example.com");
  assert.equal(demo.defaultOptionalEmail(), "mailto:ada@example.com");
  assert.equal(demo.defaultOptionalEmail("mailto:grace@example.com"), "mailto:grace@example.com");
  assert.equal(demo.defaultOptionalEmail(null), null);
}
