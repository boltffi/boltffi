#include <stdint.h>
#include <string.h>

#include "test.h"

bool test_builtins(void) {
    DemoDuration duration = demo_echo_duration((DemoDuration){42, 123456789});
    CHECK(duration.seconds == 42 && duration.nanoseconds == 123456789, "case:builtins.duration.should_roundtrip_value");
    duration = demo_make_duration(3, 1500000000);
    CHECK(duration.seconds == 4 && duration.nanoseconds == 500000000, "case:builtins.duration.should_construct_from_parts");
    CHECK(demo_duration_as_millis((DemoDuration){42, 123456789}) == 42123, "case:builtins.duration.should_report_milliseconds");
    DemoSystemTime timestamp = demo_echo_system_time((DemoSystemTime){1234567, 987654300});
    CHECK(timestamp.seconds == 1234567 && timestamp.nanoseconds == 987654300, "case:builtins.system_time.should_roundtrip_value");
    timestamp = demo_echo_system_time((DemoSystemTime){-1, 500000000});
    CHECK(timestamp.seconds == -1 && timestamp.nanoseconds == 500000000, "case:builtins.system_time.should_roundtrip_pre_epoch_value");
    CHECK(demo_system_time_to_millis((DemoSystemTime){42, 123456789}) == 42123, "case:builtins.system_time.should_convert_to_epoch_milliseconds");
    timestamp = demo_millis_to_system_time(42123);
    CHECK(timestamp.seconds == 42 && timestamp.nanoseconds == 123000000, "case:builtins.system_time.should_construct_from_epoch_milliseconds");
    const DemoUuid uuid = {{0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x46, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff}};
    DemoUuid returned_uuid = demo_echo_uuid(uuid);
    CHECK(memcmp(returned_uuid.bytes, uuid.bytes, sizeof(uuid.bytes)) == 0, "case:builtins.uuid.should_roundtrip_value");
    DemoString text = demo_uuid_to_string(uuid);
    const char expected_uuid[] = "00112233-4455-4677-8899-aabbccddeeff";
    CHECK(text.len == sizeof(expected_uuid) - 1 && memcmp(text.ptr, expected_uuid, text.len) == 0, "case:builtins.uuid.should_format_canonical_string");
    demo_string_free(&text);
    const char expected_url[] = "https://example.com/path?q=hello#fragment";
    const DemoStringView url = {expected_url, sizeof(expected_url) - 1};
    text = demo_echo_url(url);
    CHECK(text.len == url.len && memcmp(text.ptr, expected_url, text.len) == 0, "case:builtins.url.should_roundtrip_value");
    demo_string_free(&text);
    text = demo_url_to_string(url);
    CHECK(text.len == url.len && memcmp(text.ptr, expected_url, text.len) == 0, "case:builtins.url.should_format_string");
    demo_string_free(&text);
    return true;
}

bool test_maps(void) {
    DemoMapStringToI32 values = demo_make_hash_map();
    bool first_found = false;
    bool second_found = false;
    for (uintptr_t index = 0; index < values.len; ++index) {
        const DemoMapStringToI32Entry entry = values.ptr[index];
        if (entry.key.len == 5 && memcmp(entry.key.ptr, "first", 5) == 0) first_found = entry.value == 10;
        if (entry.key.len == 6 && memcmp(entry.key.ptr, "second", 6) == 0) second_found = entry.value == 20;
    }
    CHECK(values.len == 2 && first_found && second_found, "case:collections.hash_map.should_return_values");
    demo_map_string_to_i32_free(&values);
    DemoMapStringToI32Sequence empty = demo_echo_hash_map((DemoMapStringViewToI32SliceView){NULL, 0});
    CHECK(empty.len == 0, "case:collections.hash_map.should_roundtrip_empty");
    demo_map_string_to_i32_sequence_free(&empty);
    const int32_t numbers[] = {INT32_MIN, 42, INT32_MAX};
    const DemoMapStringViewToI32SliceViewEntry entries[] = {{{"numbers", 7}, {numbers, 3}}, {{"empty", 5}, {NULL, 0}}};
    DemoMapStringToI32Sequence nested = demo_echo_hash_map((DemoMapStringViewToI32SliceView){entries, 2});
    bool numbers_found = false;
    bool empty_found = false;
    for (uintptr_t index = 0; index < nested.len; ++index) {
        const DemoMapStringToI32SequenceEntry entry = nested.ptr[index];
        if (entry.key.len == 7 && memcmp(entry.key.ptr, "numbers", 7) == 0) numbers_found = entry.value.len == 3 && memcmp(entry.value.ptr, numbers, sizeof(numbers)) == 0;
        if (entry.key.len == 5 && memcmp(entry.key.ptr, "empty", 5) == 0) empty_found = entry.value.len == 0;
    }
    CHECK(nested.len == 2 && numbers_found && empty_found, "case:collections.hash_map.should_roundtrip_nested_values");
    demo_map_string_to_i32_sequence_free(&nested);
    return true;
}

bool test_custom_types(void) {
    const DemoEmailView email = {"ali@example.com", 15};
    DemoEmail returned_email = demo_echo_email(email);
    CHECK(returned_email.len == email.len && memcmp(returned_email.ptr, email.ptr, email.len) == 0, "case:custom_types.email.should_roundtrip_value");
    demo_email_free(&returned_email);
    DemoString domain = demo_email_domain(email);
    CHECK(domain.len == 11 && memcmp(domain.ptr, "example.com", 11) == 0, "case:custom_types.email.should_extract_domain");
    demo_string_free(&domain);
    const DemoUtcDateTime instant = -500;
    CHECK(demo_echo_datetime(instant) == instant, "case:custom_types.datetime.should_roundtrip_millis");
    CHECK(demo_datetime_to_millis(instant) == -500, "case:custom_types.datetime.should_convert_to_millis");
    DemoString timestamp = demo_format_timestamp(0);
    const char epoch[] = "1970-01-01T00:00:00+00:00";
    CHECK(timestamp.len == sizeof(epoch) - 1 && memcmp(timestamp.ptr, epoch, timestamp.len) == 0, "case:custom_types.datetime.should_format_rfc3339_timestamp");
    demo_string_free(&timestamp);
    const DemoEventView event = {{"launch", 6}, 123456789};
    CHECK(event.timestamp == 123456789, "case:custom_types.event.should_expose_datetime_field");
    DemoEvent returned_event = demo_echo_event(event);
    CHECK(returned_event.timestamp == event.timestamp && returned_event.name.len == 6 && memcmp(returned_event.name.ptr, event.name.ptr, 6) == 0, "case:custom_types.event.should_roundtrip_datetime_field");
    demo_event_free(&returned_event);
    CHECK(demo_event_timestamp(event) == 123456789, "case:custom_types.event.should_extract_timestamp_millis");
    const DemoEmailView emails[] = {{"ali@example.com", 15}, {"caf\xc3\xa9@example.com", 17}};
    DemoSequenceOfEmail returned_emails = demo_echo_emails((DemoSliceOfEmailView){emails, 2});
    CHECK(returned_emails.len == 2 && returned_emails.ptr[0].len == emails[0].len && memcmp(returned_emails.ptr[0].ptr, emails[0].ptr, emails[0].len) == 0 && returned_emails.ptr[1].len == emails[1].len && memcmp(returned_emails.ptr[1].ptr, emails[1].ptr, emails[1].len) == 0, "case:custom_types.vectors.emails.should_roundtrip_values");
    demo_sequence_of_email_free(&returned_emails);
    const DemoUtcDateTime instants[] = {-500, 0, 123456789};
    DemoSequenceOfUtcDateTime returned_instants = demo_echo_datetimes((DemoSliceOfUtcDateTime){instants, 3});
    CHECK(returned_instants.len == 3 && memcmp(returned_instants.ptr, instants, sizeof(instants)) == 0, "case:custom_types.vectors.datetimes.should_roundtrip_millis_values");
    demo_sequence_of_utc_date_time_free(&returned_instants);
    return true;
}
