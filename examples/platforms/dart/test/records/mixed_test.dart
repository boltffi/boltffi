import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('mixed records', () {
    final record = sampleMixedRecord();
    expect(
      echoMixedRecord(record),
      record,
      reason: "case:records.mixed.should_roundtrip_composed_record",
    );
    expect(
      makeMixedRecord(
        record.name,
        record.anchor,
        record.priority,
        record.shape,
        record.parameters,
      ),
      record,
      reason: "case:records.mixed.should_make_from_composed_parts",
    );
  });
}
