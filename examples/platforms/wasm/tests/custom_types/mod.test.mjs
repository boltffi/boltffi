import { assert, demo } from "../support/index.mjs";

export async function run() {
  const email = "café@example.com";
  globalThis.demoCase("case:custom_types.email.should_roundtrip_value");
  assert.equal(demo.echoEmail(email), email);
  globalThis.demoCase("case:custom_types.email.should_extract_domain");
  assert.equal(demo.emailDomain(email), "example.com");

  const datetime = 1_701_234_567_890n;
  globalThis.demoCase("case:custom_types.datetime.should_roundtrip_millis");
  assert.equal(demo.echoDatetime(datetime), datetime);
  globalThis.demoCase("case:custom_types.datetime.should_convert_to_millis");
  assert.equal(demo.datetimeToMillis(datetime), datetime);

  globalThis.demoCase("case:custom_types.datetime.should_format_rfc3339_timestamp");
  assert.equal(demo.formatTimestamp(datetime), "2023-11-29T05:09:27.890+00:00");

  globalThis.demoCase("case:custom_types.event.basic");
  const event = { name: "launch", timestamp: datetime };
  assert.deepEqual(demo.echoEvent(event), event);
  assert.equal(demo.eventTimestamp(event), datetime);

  globalThis.demoCase("case:custom_types.vectors.basic");
  const emails = ["café@example.com", "user@example.org"];
  assert.deepEqual(demo.echoEmails(emails), emails);

  const dts = [1_710_000_000_000n, 1_710_000_001_000n, 1_710_000_002_000n];
  assert.deepEqual(demo.echoDatetimes(dts), dts);
}
