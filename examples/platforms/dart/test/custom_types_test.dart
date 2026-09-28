import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('wrapped length preserves units and value semantics', () {
    final length = Length.$new(2.5);
    expect(
      length.value,
      2.5,
      reason: "case:custom_types.length.should_construct_in_meters",
    );
    expect(
      length.centimeters(),
      250,
      reason: "case:custom_types.length.should_convert_to_centimeters",
    );

    final returned = echoLength(length);
    expect(
      returned.value,
      2.5,
      reason: "case:custom_types.length.should_roundtrip_wrapper",
    );
    expect(returned.centimeters(), 250);
    expect(echoLength(Length(value: -1.25)).centimeters(), -125);

    returned.setCentimeters(75);
    expect(
      returned.value,
      0.75,
      reason: "case:custom_types.length.should_write_back_in_meters",
    );
    expect(returned.centimeters(), 75);
    expect(length.value, 2.5);
    expect(length.centimeters(), 250);

    final fabric = echoFabric(Fabric(length: Length(value: 1.25)));
    expect(
      fabric.length.value,
      1.25,
      reason: "case:custom_types.length.should_roundtrip_nested_wrapper",
    );
    expect(fabric.length.centimeters(), 125);
  });

  test('custom types', () {
    final timestamp = 1710000000000;
    expect(
      echoDatetime(timestamp),
      timestamp,
      reason: "case:custom_types.datetime.should_roundtrip_millis",
    );
    expect(
      formatTimestamp(timestamp),
      startsWith('2024-03-'),
      reason: "case:custom_types.datetime.should_format_rfc3339_timestamp",
    );

    final datetimeMillis = datetimeToMillis(timestamp);
    expect(
      datetimeMillis,
      timestamp,
      reason: "case:custom_types.datetime.should_convert_to_millis",
    );

    final event = Event(name: 'launch', timestamp: timestamp);
    expect(event.name, 'launch');
    expect(
      event.timestamp,
      timestamp,
      reason: "case:custom_types.event.should_expose_datetime_field",
    );

    final echoed = echoEvent(event);
    expect(echoed.name, 'launch');
    expect(
      echoed.timestamp,
      timestamp,
      reason: "case:custom_types.event.should_roundtrip_datetime_field",
    );
    expect(
      eventTimestamp(event),
      timestamp,
      reason: "case:custom_types.event.should_extract_timestamp_millis",
    );

    final email = 'café@example.com';
    expect(
      echoEmail(email),
      email,
      reason: "case:custom_types.email.should_roundtrip_value",
    );
    expect(
      emailDomain(email),
      'example.com',
      reason: "case:custom_types.email.should_extract_domain",
    );

    final emails = ['café@example.com', 'user@example.org'];
    final echoedEmails = echoEmails(emails);
    expect(echoedEmails.length, 2);
    expect(
      echoedEmails,
      ['café@example.com', 'user@example.org'],
      reason: "case:custom_types.vectors.emails.should_roundtrip_values",
    );

    final dts = Int64List.fromList([
      1710000000000,
      1710000001000,
      1710000002000,
    ]);
    final echoedDts = echoDatetimes(dts);
    expect(echoedDts.length, 3);
    expect(
      echoedDts,
      dts,
      reason:
          "case:custom_types.vectors.datetimes.should_roundtrip_millis_values",
    );
  });
}
