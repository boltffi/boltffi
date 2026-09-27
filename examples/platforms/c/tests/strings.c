#include <string.h>

#include "test.h"

bool test_strings(void) {
    DemoString empty = demo_echo_string(demo_string_view("", 0));
    CHECK(empty.len == 0, "case:primitives.strings.string.should_roundtrip_empty");
    demo_string_free(&empty);

    const char emoji[] = "hello \xf0\x9f\x8c\x8d";
    DemoString echoed = demo_echo_string(demo_string_view(emoji, sizeof(emoji) - 1));
    CHECK(echoed.len == sizeof(emoji) - 1 && memcmp(echoed.ptr, emoji, echoed.len) == 0,
          "case:primitives.strings.string.should_roundtrip_emoji");
    demo_string_free(&echoed);

    DemoString joined = demo_concat_strings(demo_string_view("hello ", 6), demo_string_view("world", 5));
    CHECK(joined.len == 11 && memcmp(joined.ptr, "hello world", 11) == 0,
          "case:primitives.strings.string.should_concatenate_values");
    demo_string_free(&joined);
    CHECK(demo_string_length(demo_string_view(emoji, sizeof(emoji) - 1)) == sizeof(emoji) - 1,
          "case:primitives.strings.string.should_report_utf8_byte_length");
    CHECK(demo_string_is_empty(demo_string_view("", 0)) && !demo_string_is_empty(demo_string_view("x", 1)),
          "case:primitives.strings.string.should_detect_empty");

    DemoString repeated = demo_repeat_string(demo_string_view("ab", 2), 3);
    CHECK(repeated.len == 6 && memcmp(repeated.ptr, "ababab", 6) == 0,
          "case:primitives.strings.string.should_repeat_value");
    demo_string_free(&repeated);
    DemoString borrowed = demo_borrowed_static_string();
    CHECK(borrowed.len == 15 && memcmp(borrowed.ptr, "borrowed static", 15) == 0,
          "case:primitives.strings.borrowed_static_string.should_return_value");
    demo_string_free(&borrowed);

    const char embedded_zero[] = {'a', '\0', 'b'};
    DemoString binary_string = demo_echo_string(demo_string_view(embedded_zero, sizeof(embedded_zero)));
    CHECK(binary_string.len == sizeof(embedded_zero) && memcmp(binary_string.ptr, embedded_zero, sizeof(embedded_zero)) == 0,
          "strings preserve embedded zero bytes");
    demo_string_free(&binary_string);
    return true;
}

bool test_bytes(void) {
    const uint8_t bytes[] = {0, 1, 128, 255};
    DemoBytes echoed = demo_echo_bytes(demo_bytes_view(bytes, sizeof(bytes)));
    CHECK(echoed.len == sizeof(bytes) && memcmp(echoed.ptr, bytes, sizeof(bytes)) == 0,
          "case:bytes.bytes.should_roundtrip_values");
    demo_bytes_free(&echoed);
    CHECK(demo_bytes_length(demo_bytes_view(bytes, sizeof(bytes))) == sizeof(bytes),
          "case:bytes.bytes.should_report_length");
    CHECK(demo_bytes_sum(demo_bytes_view(bytes, sizeof(bytes))) == 384,
          "case:bytes.bytes.should_sum_values");

    const uint8_t sequence[] = {0, 1, 2, 3};
    DemoBytes generated = demo_make_bytes(4);
    CHECK(generated.len == sizeof(sequence) && memcmp(generated.ptr, sequence, sizeof(sequence)) == 0,
          "case:bytes.bytes.should_make_sequential_values");
    demo_bytes_free(&generated);

    const uint8_t reverse[] = {255, 128, 1, 0};
    DemoBytes reversed = demo_reverse_bytes(demo_bytes_view(bytes, sizeof(bytes)));
    CHECK(reversed.len == sizeof(reverse) && memcmp(reversed.ptr, reverse, sizeof(reverse)) == 0,
          "case:bytes.bytes.should_reverse_values");
    demo_bytes_free(&reversed);
    DemoBytes empty = demo_echo_bytes(demo_bytes_view(NULL, 0));
    CHECK(empty.len == 0, "empty byte buffers round trip");
    demo_bytes_free(&empty);
    return true;
}
