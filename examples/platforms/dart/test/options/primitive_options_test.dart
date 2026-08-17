import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('primitive options', () {
    expect(
      echoOptionalI32(7),
      7,
      reason: "case:options.primitives.i32.should_roundtrip_some",
    );
    expect(
      echoOptionalI32(null),
      isNull,
      reason: "case:options.primitives.i32.should_roundtrip_none",
    );

    expect(
      echoOptionalF64(1.5),
      closeTo(1.5, 0.0001),
      reason: "case:options.primitives.f64.should_roundtrip_some",
    );
    expect(
      echoOptionalF64(null),
      isNull,
      reason: "case:options.primitives.f64.should_roundtrip_none",
    );

    expect(
      echoOptionalBool(true),
      isTrue,
      reason: "case:options.primitives.bool.should_roundtrip_some",
    );
    expect(
      echoOptionalBool(null),
      isNull,
      reason: "case:options.primitives.bool.should_roundtrip_none",
    );

    expect(
      unwrapOrDefaultI32(9, 4),
      9,
      reason: "case:options.primitives.i32.should_unwrap_some",
    );
    expect(
      unwrapOrDefaultI32(null, 4),
      4,
      reason: "case:options.primitives.i32.should_use_default_for_none",
    );

    expect(
      makeSomeI32(12),
      12,
      reason: "case:options.primitives.i32.should_make_some",
    );
    expect(
      makeNoneI32(),
      isNull,
      reason: "case:options.primitives.i32.should_make_none",
    );

    expect(
      doubleIfSome(8),
      16,
      reason: "case:options.primitives.i32.should_double_some",
    );
    expect(
      doubleIfSome(null),
      isNull,
      reason: "case:options.primitives.i32.should_preserve_none_when_doubling",
    );

    expect(
      findEven(8),
      8,
      reason: "case:options.primitives.i32.should_find_even_value",
    );
    expect(
      findEven(7),
      isNull,
      reason: "case:options.primitives.i32.should_return_none_for_odd_value",
    );

    expect(
      findPositiveI64(123),
      123,
      reason: "case:options.primitives.i64.should_find_positive_value",
    );
    expect(
      findPositiveI64(-5),
      isNull,
      reason:
          "case:options.primitives.i64.should_return_none_for_non_positive_value",
    );

    expect(
      findPositiveF64(3.5),
      closeTo(3.5, 0.0001),
      reason: "case:options.primitives.f64.should_find_positive_value",
    );
    expect(
      findPositiveF64(-1.0),
      isNull,
      reason:
          "case:options.primitives.f64.should_return_none_for_non_positive_value",
    );
  });
}
