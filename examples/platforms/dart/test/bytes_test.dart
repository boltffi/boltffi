import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('vec bytes', () {
    final echoed = echoBytes(Uint8List.fromList([1, 2, 3, 4]));
    expect(
      echoed.length,
      4,
      reason: "case:bytes.bytes.should_roundtrip_values",
    );
    expect(echoed, [1, 2, 3, 4]);

    expect(
      bytesLength(Uint8List.fromList([10, 20, 30])),
      3,
      reason: "case:bytes.bytes.should_report_length",
    );
    expect(
      bytesSum(Uint8List.fromList([1, 2, 3, 4])),
      10,
      reason: "case:bytes.bytes.should_sum_values",
    );

    final made = makeBytes(5);
    expect(
      made.length,
      5,
      reason: "case:bytes.bytes.should_make_sequential_values",
    );
    expect(made, [0, 1, 2, 3, 4]);

    final reversed = reverseBytes(Uint8List.fromList([5, 6, 7]));
    expect(
      reversed.length,
      3,
      reason: "case:bytes.bytes.should_reverse_values",
    );
    expect(reversed, [7, 6, 5]);
  });
}
