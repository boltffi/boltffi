import { assert, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.echoString(""), "", "case:primitives.strings.string.should_roundtrip_empty");
  assert.equal(demo.echoString("hello 🌍"), "hello 🌍", "case:primitives.strings.string.should_roundtrip_emoji");
  assert.equal(demo.concatStrings("foo", "bar"), "foobar", "case:primitives.strings.string.should_concatenate_values");
  assert.equal(demo.stringLength("café"), 5, "case:primitives.strings.string.should_report_utf8_byte_length");
  assert.equal(demo.stringIsEmpty(""), true, "case:primitives.strings.string.should_detect_empty");
  assert.equal(demo.repeatString("ab", 3), "ababab", "case:primitives.strings.string.should_repeat_value");
  assert.equal(demo.borrowedStaticString(), "borrowed static", "case:primitives.strings.borrowed_static_string.should_return_value");
}
