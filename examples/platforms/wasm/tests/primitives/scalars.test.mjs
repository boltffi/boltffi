import { assert, assertApprox, demo } from "../support/index.mjs";

export async function run() {
  assert.equal(demo.echoBool(true), true, "case:primitives.scalars.bool.should_roundtrip_true");
  assert.equal(demo.negateBool(false), true, "case:primitives.scalars.bool.should_negate_false_to_true");
  assert.equal(demo.echoI8(-7), -7, "case:primitives.scalars.i8.should_roundtrip_negative_value");
  assert.equal(demo.echoU8(255), 255, "case:primitives.scalars.u8.should_roundtrip_max_value");
  assert.equal(demo.echoI16(-1234), -1234, "case:primitives.scalars.i16.should_roundtrip_negative_value");
  assert.equal(demo.echoU16(55_000), 55_000, "case:primitives.scalars.u16.should_roundtrip_large_value");
  assert.equal(demo.echoI32(-42), -42, "case:primitives.scalars.i32.should_roundtrip_negative_value");
  assert.equal(demo.addI32(10, 20), 30, "case:primitives.scalars.i32.should_add_two_values");
  assert.equal(demo.echoU32(2_147_483_647), 2_147_483_647, "case:primitives.scalars.u32.should_roundtrip_large_value");
  assert.equal(demo.echoI64(-9_999_999_999n), -9_999_999_999n, "case:primitives.scalars.i64.should_roundtrip_large_negative_value");
  assert.equal(demo.echoU64(9_999_999_999n), 9_999_999_999n, "case:primitives.scalars.u64.should_roundtrip_large_value");
  globalThis.demoCase("case:primitives.scalars.f32.should_roundtrip_value_with_tolerance");
  assertApprox(demo.echoF32(3.5), 3.5, 1e-6);
  globalThis.demoCase("case:primitives.scalars.f32.should_add_two_values_with_tolerance");
  assertApprox(demo.addF32(1.5, 2.5), 4.0, 1e-6);
  globalThis.demoCase("case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance");
  assertApprox(demo.echoF64(3.14159265359), 3.14159265359, 1e-12);
  globalThis.demoCase("case:primitives.scalars.f64.should_add_two_values_with_tolerance");
  assertApprox(demo.addF64(1.5, 2.5), 4.0, 1e-12);
  assert.equal(demo.echoUsize(123), 123, "case:primitives.scalars.usize.should_roundtrip_value");
  assert.equal(demo.echoIsize(-123), -123, "case:primitives.scalars.isize.should_roundtrip_negative_value");
  assert.equal(demo.noop(), undefined, "case:primitives.scalars.noop.should_cross_without_values");
  assert.equal(demo.add(2, 3), 5, "case:primitives.scalars.i32.should_add_with_benchmark_alias");
  globalThis.demoCase("case:primitives.scalars.f64.should_multiply_two_values");
  assertApprox(demo.multiply(1.5, 4.0), 6.0, 1e-12);
}
