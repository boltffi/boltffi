import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.with_strings.person.should_make_from_fields");
  const person = demo.makePerson("Ali", 30);
  globalThis.demoCase("case:records.with_strings.person.should_roundtrip_value");
  assert.deepEqual(demo.echoPerson(person), person);
  globalThis.demoCase("case:records.with_strings.person.should_format_greeting");
  assert.equal(demo.greetPerson(person), "Hello, Ali! You are 30 years old.");

  globalThis.demoCase("case:records.with_strings.address.should_roundtrip_value");
  const address = { street: "Main St", city: "Amsterdam", zip: "1000AA" };
  assert.deepEqual(demo.echoAddress(address), address);
  globalThis.demoCase("case:records.with_strings.address.should_format_value");
  assert.equal(demo.formatAddress(address), "Main St, Amsterdam, 1000AA");
}
