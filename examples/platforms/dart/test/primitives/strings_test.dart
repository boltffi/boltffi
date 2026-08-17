import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('strings', () {
    expect(echoString('hello'), 'hello');
    expect(
      borrowedStaticString(),
      'borrowed static',
      reason: "case:primitives.strings.borrowed_static_string.should_return_value",
    );
    expect(
      echoString(''),
      '',
      reason: "case:primitives.strings.string.should_roundtrip_empty",
    );
    expect(echoString('café'), 'café');
    expect(echoString('日本語'), '日本語');
    expect(
      echoString('hello 🌍 world'),
      'hello 🌍 world',
      reason: "case:primitives.strings.string.should_roundtrip_emoji",
    );
    expect(
      concatStrings('foo', 'bar'),
      'foobar',
      reason: "case:primitives.strings.string.should_concatenate_values",
    );
    expect(concatStrings('', 'bar'), 'bar');
    expect(concatStrings('foo', ''), 'foo');
    expect(concatStrings('🎉', '🎊'), '🎉🎊');
    expect(
      stringLength('hello'),
      5,
      reason: "case:primitives.strings.string.should_report_utf8_byte_length",
    );
    expect(stringLength(''), 0);
    expect(stringLength('café'), 5);
    expect(stringLength('🌍'), 4);
    expect(
      stringIsEmpty(''),
      isTrue,
      reason: "case:primitives.strings.string.should_detect_empty",
    );
    expect(
      repeatString('ab', 3),
      'ababab',
      reason: "case:primitives.strings.string.should_repeat_value",
    );
  });
}
