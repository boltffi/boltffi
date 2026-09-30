import * as demo from "../../dist/demo.js";

demo.repeatGreeting("ada");
demo.repeatGreeting("ada", undefined, undefined, true);
demo.describeLimit();
demo.describeLimit("daily", null);
demo.applyOptionalCallback(21);
demo.applyOptionalCallback(21, { onValue: (value) => value * 2 });

const counter = demo.DefaultedCounter.new();
counter.offset();
await counter.asyncOffset();
await counter.asyncOffset(undefined, { signal: new AbortController().signal });
counter.dispose();

demo.DefaultedCounter.withOffset(undefined, 3).dispose();
demo.DefaultedCounter.fromText().dispose();
(await demo.DefaultedCounter.start()).dispose();
(await demo.DefaultedCounter.start(undefined, undefined, { onValue: (value) => value * 2 })).dispose();
demo.DefaultedCounter.integerLimits();
demo.scaleDefault();
demo.scaleDefault(undefined, null);
demo.defaultFloatBits();
demo.defaultDoubleBits();
demo.DefaultMode.matches(demo.DefaultMode.Quiet);
await demo.DefaultMode.load();
await demo.asyncDefault();
await demo.asyncDefault(undefined, { signal: new AbortController().signal });
demo.DefaultAmount.offset({});
demo.DefaultAmount.withScaledValue();
demo.DefaultAmount.tryScaledValue();
demo.NamedAmount.withValue();
await demo.NamedAmount.load();
demo.defaultLimit();
demo.defaultTimeoutSeconds();
demo.defaultEmail();
demo.defaultOptionalEmail();
demo.defaultOptionalEmail(null);
