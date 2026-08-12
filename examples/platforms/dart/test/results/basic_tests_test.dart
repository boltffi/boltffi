import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test('basic results', () async {
    expect(
      safeDivide(10, 2),
      5,
      reason: "case:results.basic.safe_divide.should_return_quotient",
    );
    expect(
      () => safeDivide(10, 0),
      throwsBoltException('division by zero'),
      reason: "case:results.basic.safe_divide.should_reject_division_by_zero",
    );

    expect(
      safeSqrt(16.0),
      closeTo(4.0, 0.0001),
      reason: 'case:results.basic.safe_sqrt.should_return_square_root',
    );
    expect(
      () => safeSqrt(-4.0),
      throwsBoltException('negative input'),
      reason: 'case:results.basic.safe_sqrt.should_reject_negative_input',
    );

    expect(
      parsePoint('3.0,4.0'),
      Point(x: 3.0, y: 4.0),
      reason: 'case:results.basic.parse_point.should_parse_coordinates',
    );

    expect(
      () => parsePoint('bad'),
      throwsBoltException("expected format: x,y"),
      reason: 'case:results.basic.parse_point.should_reject_malformed_input',
    );

    expect(
      alwaysOk(21),
      42,
      reason: 'case:results.basic.always_ok.should_return_doubled_value',
    );
    expect(
      () => alwaysErr('boom'),
      throwsBoltException('boom'),
      reason: 'case:results.basic.always_err.should_return_message_error',
    );

    expect(
      resultToString($$BoltResult.ok(7)),
      'ok: 7',
      reason: 'case:results.basic.result_to_string.should_render_ok',
    );
    expect(
      resultToString($$BoltResult.err($$BoltException('bad input'))),
      'err: bad input',
      reason: 'case:results.basic.result_to_string.should_render_err',
    );

    expect(
      divide(20, 4),
      5,
      reason: 'case:results.basic.divide.should_return_quotient',
    );
    expect(
      () => divide(20, 0),
      throwsBoltException('division by zero'),
      reason: 'case:results.basic.divide.should_reject_division_by_zero',
    );

    expect(
      parseInt('42'),
      42,
      reason: 'case:results.basic.parse_int.should_parse_integer',
    );
    expect(
      () => parseInt('not-a-number'),
      throwsBoltException('invalid integer'),
      reason: 'case:results.basic.parse_int.should_reject_invalid_integer',
    );

    expect(
      isEven(4),
      isTrue,
      reason: 'case:results.basic.is_even.should_return_parity',
    );
    expect(isEven(3), isFalse);
    expect(
      () => isEven(-1),
      throwsBoltException('negative input'),
      reason: 'case:results.basic.is_even.should_reject_negative_input',
    );

    expect(
      validateName('Dart'),
      'Hello, Dart!',
      reason: 'case:results.basic.validate_name.should_greet_valid_name',
    );
    expect(
      () => validateName(''),
      throwsBoltException('name cannot be empty'),
      reason: 'case:results.basic.validate_name.should_reject_empty_name',
    );
  });
}
