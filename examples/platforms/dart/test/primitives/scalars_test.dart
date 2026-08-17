import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('bool', () {
    expect(
      echoBool(true),
      isTrue,
      reason: "case:primitives.scalars.bool.should_roundtrip_true",
    );
    expect(echoBool(false), isFalse);
    expect(
      negateBool(true),
      isFalse,
      reason: "case:primitives.scalars.bool.should_negate_false_to_true",
    );
    expect(negateBool(false), isTrue);
  });

  test('i8', () {
    expect(echoI8(0), 0);
    expect(
      echoI8(-7),
      -7,
      reason: "case:primitives.scalars.i8.should_roundtrip_negative_value",
    );
  });

  test('u8', () {
    expect(echoU8(0), 0);
    expect(
      echoU8(0xFF),
      0xFF,
      reason: "case:primitives.scalars.u8.should_roundtrip_max_value",
    );
  });

  test('i16', () {
    expect(echoI16(0), 0);
    expect(
      echoI16(-1234),
      -1234,
      reason: "case:primitives.scalars.i16.should_roundtrip_negative_value",
    );
  });

  test('u16', () {
    expect(echoU16(0), 0);
    expect(
      echoU16(55000),
      55000,
      reason: "case:primitives.scalars.u16.should_roundtrip_large_value",
    );
  });

  test('i32', () {
    expect(echoI32(42), 42);
    expect(
      echoI32(-100),
      -100,
      reason: "case:primitives.scalars.i32.should_roundtrip_negative_value",
    );
    expect(
      addI32(10, 20),
      30,
      reason: "case:primitives.scalars.i32.should_add_two_values",
    );
    expect(
      add(7, 9),
      16,
      reason: "case:primitives.scalars.i32.should_add_with_benchmark_alias",
    );
  });

  test('u32', () {
    expect(echoU32(0), 0);
    expect(
      echoU32(4000000000),
      4000000000,
      reason: "case:primitives.scalars.u32.should_roundtrip_large_value",
    );
  });

  test('i64', () {
    expect(echoI64(9999999999), 9999999999);
    expect(
      echoI64(-9999999999),
      -9999999999,
      reason:
          "case:primitives.scalars.i64.should_roundtrip_large_negative_value",
    );
  });

  test('u64', () {
    expect(echoU64(0), 0);
    expect(
      echoU64(9999999999),
      9999999999,
      reason: "case:primitives.scalars.u64.should_roundtrip_large_value",
    );
  });

  test('f32', () {
    expect(
      echoF32(3.14),
      closeTo(3.14, 0.001),
      reason:
          "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance",
    );
    expect(
      addF32(1.5, 2.5),
      closeTo(4.0, 0.001),
      reason:
          "case:primitives.scalars.f32.should_add_two_values_with_tolerance",
    );
  });

  test('f64', () {
    expect(
      echoF64(3.14159265359),
      closeTo(3.14159265359, 0.0000001),
      reason: "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance",
    );
    expect(
      addF64(1.5, 2.5),
      closeTo(4.0, 0.0000001),
      reason:
          "case:primitives.scalars.f64.should_add_two_values_with_tolerance",
    );
    expect(
      multiply(1.5, 4.0),
      closeTo(6.0, 0.0000001),
      reason: "case:primitives.scalars.f64.should_multiply_two_values",
    );
  });

  test('usize', () {
    expect(echoUsize(0), 0);
    expect(
      echoUsize(123),
      123,
      reason: "case:primitives.scalars.usize.should_roundtrip_value",
    );
  });

  test('isize', () {
    expect(echoIsize(0), 0);
    expect(
      echoIsize(-123),
      -123,
      reason: "case:primitives.scalars.isize.should_roundtrip_negative_value",
    );
  });

  test('noop', () {
    expect(
      noop,
      returnsNormally,
      reason: "case:primitives.scalars.noop.should_cross_without_values",
    );
  });
}
