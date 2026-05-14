import { assert, demo } from "../support/index.mjs";

export async function run() {
  // case:options.primitives.i32.some_none_and_helpers
  assert.equal(demo.echoOptionalI32(7), 7);
  assert.equal(demo.echoOptionalI32(null), null);
  assert.equal(demo.echoOptionalF64(4.5), 4.5);
  assert.equal(demo.echoOptionalF64(null), null);
  assert.equal(demo.echoOptionalBool(true), true);
  assert.equal(demo.echoOptionalBool(null), null);
  assert.equal(demo.unwrapOrDefaultI32(9, 4), 9);
  assert.equal(demo.unwrapOrDefaultI32(null, 4), 4);
  assert.equal(demo.makeSomeI32(12), 12);
  assert.equal(demo.makeNoneI32(), null);
  assert.equal(demo.doubleIfSome(8), 16);
  assert.equal(demo.doubleIfSome(null), null);
}
