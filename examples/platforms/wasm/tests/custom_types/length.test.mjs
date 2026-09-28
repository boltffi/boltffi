import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:custom_types.length.should_construct_in_meters");
  const length = demo.Length.new(2.5);
  assert.equal(length.value, 2.5);
  globalThis.demoCase("case:custom_types.length.should_convert_to_centimeters");
  assert.equal(demo.Length.centimeters(length), 250);

  globalThis.demoCase("case:custom_types.length.should_write_back_in_meters");
  const updated = demo.Length.setCentimeters(length, 75);
  assert.equal(updated.value, 0.75);
  assert.equal(demo.Length.centimeters(updated), 75);
  assert.equal(length.value, 2.5);
}
