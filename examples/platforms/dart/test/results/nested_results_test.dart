import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('nested results', () async {
    expect(
      resultOfOption(5),
      10,
      reason:
          'case:results.nested_results.option.should_return_some_for_positive_key',
    );
    expect(
      resultOfOption(0),
      isNull,
      reason:
          'case:results.nested_results.option.should_return_none_for_zero_key',
    );
    expect(
      () => resultOfOption(-1),
      throwsBoltException('invalid key'),
      reason: 'case:results.nested_results.option.should_reject_negative_key',
    );

    final vec = resultOfVec(3);
    expect(
      vec,
      [0, 1, 2],
      reason:
          'case:results.nested_results.vec.should_return_values_for_non_negative_count',
    );
    expect(
      () => resultOfVec(-1),
      throwsBoltException('negative count'),
      reason: 'case:results.nested_results.vec.should_reject_negative_count',
    );

    expect(
      resultOfString(1),
      'item_1',
      reason:
          'case:results.nested_results.string.should_return_value_for_non_negative_key',
    );
    expect(
      () => resultOfString(-1),
      throwsException,
      reason: 'case:results.nested_results.string.should_reject_negative_key',
    );
  });
}
