import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('async results', () async {
    expect(
      await asyncSafeDivide(12, 3),
      4,
      reason: "case:results.async_results.safe_divide.should_return_quotient",
    );
    expect(
      () => asyncSafeDivide(12, 0),
      throwsA(MathError.divisionByZero),
      reason:
          "case:results.async_results.safe_divide.should_reject_division_by_zero",
    );

    expect(
      await asyncFallibleFetch(7),
      'value_7',
      reason:
          "case:results.async_results.fallible_fetch.should_return_value_for_non_negative_key",
    );
    expect(
      () => asyncFallibleFetch(-1),
      throwsBoltException("invalid key"),
      reason:
          "case:results.async_results.fallible_fetch.should_reject_negative_key",
    );

    expect(
      await asyncFindValue(4),
      40,
      reason:
          "case:results.async_results.find_value.should_return_some_for_positive_key",
    );
    expect(
      await asyncFindValue(0),
      isNull,
      reason:
          "case:results.async_results.find_value.should_return_none_for_zero_key",
    );
    expect(
      () => asyncFindValue(-1),
      throwsBoltException("invalid key"),
      reason:
          "case:results.async_results.find_value.should_reject_negative_key",
    );
  });
}
