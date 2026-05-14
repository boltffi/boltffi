import { assert, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.echoString(""), "", "case:primitives.strings.echo.empty");
  assert.equal(demo.echoString("hello 🌍"), "hello 🌍", "case:primitives.strings.echo.emoji");
  assert.equal(demo.concatStrings("foo", "bar"), "foobar", "case:primitives.strings.concat.basic");
  assert.equal(demo.stringLength("café"), 5, "case:primitives.strings.length.utf8_bytes");
  assert.equal(demo.stringIsEmpty(""), true, "case:primitives.strings.is_empty.empty");
  assert.equal(demo.repeatString("ab", 3), "ababab", "case:primitives.strings.repeat.basic");
}
