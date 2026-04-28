import { assert, demo } from "../support/index.mjs";

export async function run() {
  const email = "café@example.com";
  assert.equal(demo.echoEmail(email), email);
  assert.equal(demo.emailDomain(email), "example.com");

  const datetime = 1_701_234_567_890n;
  assert.equal(demo.echoDatetime(datetime), datetime);
  assert.equal(demo.datetimeToMillis(datetime), datetime);
  assert.equal(demo.formatTimestamp(datetime), "2023-11-29T05:09:27.890+00:00");

  const event = { name: "launch", timestamp: datetime };
  assert.deepEqual(demo.echoEvent(event), event);
  assert.equal(demo.eventTimestamp(event), datetime);

  const emails = ["café@example.com", "user@example.org"];
  assert.deepEqual(demo.echoEmails(emails), emails);

  const dts = [1_710_000_000_000n, 1_710_000_001_000n, 1_710_000_002_000n];
  assert.deepEqual(demo.echoDatetimes(dts), dts);
}
