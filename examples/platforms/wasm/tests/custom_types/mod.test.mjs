import { assert, demo } from "../support/index.mjs";

export async function run() {
  const length = { value: 2.5 };
  globalThis.demoCase("case:custom_types.length.should_roundtrip_wrapper");
  const returned = demo.echoLength(length);
  assert.equal(returned.value, 2.5);
  assert.equal(demo.Length.centimeters(returned), 250);
  assert.equal(demo.Length.centimeters(demo.echoLength({ value: -1.25 })), -125);

  const updated = demo.Length.setCentimeters(returned, 75);
  assert.equal(updated.value, 0.75);
  assert.equal(demo.Length.centimeters(updated), 75);
  assert.equal(length.value, 2.5);
  assert.equal(demo.Length.centimeters(length), 250);

  globalThis.demoCase("case:custom_types.length.should_roundtrip_nested_wrapper");
  const fabric = demo.echoFabric({ length: { value: 1.25 } });
  assert.equal(fabric.length.value, 1.25);
  assert.equal(demo.Length.centimeters(fabric.length), 125);

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

  const event = { name: "launch", timestamp: datetime };
  globalThis.demoCase("case:custom_types.event.should_expose_datetime_field");
  assert.equal(event.name, "launch");
  assert.equal(event.timestamp, datetime);
  globalThis.demoCase("case:custom_types.event.should_roundtrip_datetime_field");
  assert.deepEqual(demo.echoEvent(event), event);
  globalThis.demoCase("case:custom_types.event.should_extract_timestamp_millis");
  assert.equal(demo.eventTimestamp(event), datetime);

  const emails = ["café@example.com", "user@example.org"];
  globalThis.demoCase("case:custom_types.vectors.emails.should_roundtrip_values");
  assert.deepEqual(demo.echoEmails(emails), emails);

  const dts = [1_710_000_000_000n, 1_710_000_001_000n, 1_710_000_002_000n];
  globalThis.demoCase("case:custom_types.vectors.datetimes.should_roundtrip_millis_values");
  assert.deepEqual(demo.echoDatetimes(dts), dts);
}
