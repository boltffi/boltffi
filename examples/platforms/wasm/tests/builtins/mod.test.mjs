import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:builtins.duration.basic");
  const duration = { secs: 2n, nanos: 500_000_000 };
  assert.deepEqual(demo.echoDuration(duration), duration);
  assert.deepEqual(demo.makeDuration(3n, 25), { secs: 3n, nanos: 25 });
  assert.equal(demo.durationAsMillis(duration), 2_500n);

  globalThis.demoCase("case:builtins.system_time.basic");
  const instant = new Date(1_701_234_567_890);
  assert.equal(demo.echoSystemTime(instant).getTime(), instant.getTime());
  assert.equal(demo.systemTimeToMillis(instant), 1_701_234_567_890n);
  assert.equal(demo.millisToSystemTime(1_701_234_567_890n).getTime(), instant.getTime());

  globalThis.demoCase("case:builtins.uuid.basic");
  const uuid = "123e4567-e89b-12d3-a456-426614174000";
  assert.equal(demo.echoUuid(uuid), uuid);
  assert.equal(demo.uuidToString(uuid), uuid);

  globalThis.demoCase("case:builtins.url.basic");
  const url = "https://example.com/demo?q=boltffi";
  assert.equal(demo.echoUrl(url), url);
  assert.equal(demo.urlToString(url), url);
}
