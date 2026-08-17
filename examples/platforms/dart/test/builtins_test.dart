import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('builtins', () {
    final duration = Duration(seconds: 12, microseconds: 345000);
    expect(
      echoDuration(duration),
      duration,
      reason: "case:builtins.duration.should_roundtrip_value",
    );
    expect(
      makeDuration(7, 89),
      Duration(seconds: 7, microseconds: 89 ~/ 1000),
      reason: "case:builtins.duration.should_construct_from_parts",
    );
    expect(
      durationAsMillis(Duration(milliseconds: 1234)),
      1234,
      reason: "case:builtins.duration.should_report_milliseconds",
    );

    final instant = DateTime.fromMillisecondsSinceEpoch(1710000000123);
    expect(
      echoSystemTime(instant),
      instant,
      reason: "case:builtins.system_time.should_roundtrip_value",
    );
    final preEpochInstant = DateTime.fromMillisecondsSinceEpoch(-500);
    expect(
      echoSystemTime(preEpochInstant),
      preEpochInstant,
      reason: "case:builtins.system_time.should_roundtrip_pre_epoch_value",
    );
    expect(
      systemTimeToMillis(instant),
      1710000000123,
      reason: "case:builtins.system_time.should_convert_to_epoch_milliseconds",
    );
    expect(
      millisToSystemTime(1710000000123),
      instant,
      reason:
          "case:builtins.system_time.should_construct_from_epoch_milliseconds",
    );

    final uuidStr = '550e8400-e29b-41d4-a716-446655440000';
    final uuidValue = $$BoltUUIDValue.parse(uuidStr);
    expect(
      echoUuid(uuidValue),
      uuidValue,
      reason: "case:builtins.uuid.should_roundtrip_value",
    );
    expect(uuidToString(uuidValue), uuidStr, reason: "case:builtins.uuid.should_format_canonical_string");

    final uri = Uri.parse('https://example.com/path?q=boltffi');
    expect(
      echoUrl(uri),
      uri,
      reason: "case:builtins.url.should_roundtrip_value",
    );
    expect(
      urlToString(uri),
      'https://example.com/path?q=boltffi',
      reason: "case:builtins.url.should_format_string",
    );
  });
}
