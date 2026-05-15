import { assert, demo } from "../support/index.mjs";

export async function run() {
  const duration = { secs: 2n, nanos: 500_000_000 };
  globalThis.demoCase("case:builtins.duration.should_roundtrip_value");
  assert.deepEqual(demo.echoDuration(duration), duration);
  globalThis.demoCase("case:builtins.duration.should_construct_from_parts");
  assert.deepEqual(demo.makeDuration(3n, 25), { secs: 3n, nanos: 25 });
  globalThis.demoCase("case:builtins.duration.should_report_milliseconds");
  assert.equal(demo.durationAsMillis(duration), 2_500n);

  const instant = new Date(1_701_234_567_890);
  globalThis.demoCase("case:builtins.system_time.should_roundtrip_value");
  assert.equal(demo.echoSystemTime(instant).getTime(), instant.getTime());
  globalThis.demoCase("case:builtins.system_time.should_convert_to_epoch_milliseconds");
  assert.equal(demo.systemTimeToMillis(instant), 1_701_234_567_890n);
  globalThis.demoCase("case:builtins.system_time.should_construct_from_epoch_milliseconds");
  assert.equal(demo.millisToSystemTime(1_701_234_567_890n).getTime(), instant.getTime());

  const uuid = "123e4567-e89b-12d3-a456-426614174000";
  globalThis.demoCase("case:builtins.uuid.should_roundtrip_value");
  assert.equal(demo.echoUuid(uuid), uuid);
  globalThis.demoCase("case:builtins.uuid.should_format_canonical_string");
  assert.equal(demo.uuidToString(uuid), uuid);

  const url = "https://example.com/demo?q=boltffi";
  globalThis.demoCase("case:builtins.url.should_roundtrip_value");
  assert.equal(demo.echoUrl(url), url);
  globalThis.demoCase("case:builtins.url.should_format_string");
  assert.equal(demo.urlToString(url), url);
}
