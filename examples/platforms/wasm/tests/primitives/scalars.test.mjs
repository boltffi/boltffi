import { assert, assertApprox, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.echoBool(true), true, "case:primitives.scalars.echo_bool.true");
  assert.equal(demo.negateBool(false), true, "case:primitives.scalars.negate_bool.false");
  assert.equal(demo.echoI8(-7), -7);
  assert.equal(demo.echoU8(255), 255);
  assert.equal(demo.echoI16(-1234), -1234);
  assert.equal(demo.echoU16(55_000), 55_000);
  assert.equal(demo.echoI32(-42), -42, "case:primitives.scalars.echo_i32.negative");
  assert.equal(demo.addI32(10, 20), 30, "case:primitives.scalars.add_i32.basic");
  assert.equal(demo.echoU32(2_147_483_647), 2_147_483_647);
  assert.equal(demo.echoI64(-9_999_999_999n), -9_999_999_999n, "case:primitives.scalars.echo_i64.negative_large");
  assert.equal(demo.echoU64(9_999_999_999n), 9_999_999_999n);
  globalThis.demoCase("case:primitives.scalars.echo_f32.basic");
  assertApprox(demo.echoF32(3.5), 3.5, 1e-6);
  globalThis.demoCase("case:primitives.scalars.add_f32.basic");
  assertApprox(demo.addF32(1.5, 2.5), 4.0, 1e-6);
  globalThis.demoCase("case:primitives.scalars.echo_f64.pi");
  assertApprox(demo.echoF64(3.14159265359), 3.14159265359, 1e-12);
  globalThis.demoCase("case:primitives.scalars.add_f64.basic");
  assertApprox(demo.addF64(1.5, 2.5), 4.0, 1e-12);
  assert.equal(demo.echoUsize(123), 123);
  assert.equal(demo.echoIsize(-123), -123);
}
