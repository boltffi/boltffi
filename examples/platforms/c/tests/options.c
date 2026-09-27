#include <math.h>
#include <stdint.h>
#include <string.h>

#include "test.h"

bool test_options(void) {
    DemoOptionI32 integer = demo_echo_optional_i32((DemoOptionI32){true, INT32_MIN});
    CHECK(integer.has_value && integer.value == INT32_MIN, "case:options.primitives.i32.should_roundtrip_some");
    CHECK(!demo_echo_optional_i32((DemoOptionI32){false, 42}).has_value, "case:options.primitives.i32.should_roundtrip_none");
    DemoOptionF64 real = demo_echo_optional_f64((DemoOptionF64){true, -3.25});
    CHECK(real.has_value && fabs(real.value + 3.25) < 1e-12, "case:options.primitives.f64.should_roundtrip_some");
    CHECK(!demo_echo_optional_f64((DemoOptionF64){false, 42.0}).has_value, "case:options.primitives.f64.should_roundtrip_none");
    DemoOptionBool boolean = demo_echo_optional_bool((DemoOptionBool){true, false});
    CHECK(boolean.has_value && !boolean.value, "case:options.primitives.bool.should_roundtrip_some");
    CHECK(!demo_echo_optional_bool((DemoOptionBool){false, true}).has_value, "case:options.primitives.bool.should_roundtrip_none");
    CHECK(demo_unwrap_or_default_i32((DemoOptionI32){true, -17}, 42) == -17, "case:options.primitives.i32.should_unwrap_some");
    CHECK(demo_unwrap_or_default_i32((DemoOptionI32){false, -17}, 42) == 42, "case:options.primitives.i32.should_use_default_for_none");
    integer = demo_make_some_i32(-42);
    CHECK(integer.has_value && integer.value == -42, "case:options.primitives.i32.should_make_some");
    CHECK(!demo_make_none_i32().has_value, "case:options.primitives.i32.should_make_none");
    integer = demo_double_if_some((DemoOptionI32){true, -21});
    CHECK(integer.has_value && integer.value == -42, "case:options.primitives.i32.should_double_some");
    CHECK(!demo_double_if_some((DemoOptionI32){false, 21}).has_value, "case:options.primitives.i32.should_preserve_none_when_doubling");
    integer = demo_find_even(24);
    CHECK(integer.has_value && integer.value == 24, "case:options.primitives.i32.should_find_even_value");
    CHECK(!demo_find_even(23).has_value, "case:options.primitives.i32.should_return_none_for_odd_value");
    DemoOptionI64 wide_integer = demo_find_positive_i64(INT64_MAX);
    CHECK(wide_integer.has_value && wide_integer.value == INT64_MAX, "case:options.primitives.i64.should_find_positive_value");
    CHECK(!demo_find_positive_i64(0).has_value && !demo_find_positive_i64(-1).has_value, "case:options.primitives.i64.should_return_none_for_non_positive_value");
    real = demo_find_positive_f64(1.25);
    CHECK(real.has_value && fabs(real.value - 1.25) < 1e-12, "case:options.primitives.f64.should_find_positive_value");
    CHECK(!demo_find_positive_f64(0.0).has_value && !demo_find_positive_f64(-1.0).has_value, "case:options.primitives.f64.should_return_none_for_non_positive_value");
    return true;
}

bool test_nested_options(void) {
    const char utf8_name[] = "caf\xc3\xa9";
    const DemoOptionStringView name = {true, {utf8_name, sizeof(utf8_name) - 1}};
    DemoOptionString returned_name = demo_echo_optional_string(name);
    CHECK(returned_name.has_value && returned_name.value.len == name.value.len && memcmp(returned_name.value.ptr, utf8_name, name.value.len) == 0, "case:options.complex.string.should_roundtrip_some");
    demo_option_string_free(&returned_name);
    returned_name = demo_echo_optional_string((DemoOptionStringView){0});
    CHECK(!returned_name.has_value, "case:options.complex.string.should_roundtrip_none");
    demo_option_string_free(&returned_name);
    CHECK(demo_is_some_string(name), "case:options.complex.string.should_report_some");
    CHECK(!demo_is_some_string((DemoOptionStringView){0}), "case:options.complex.string.should_report_none");
    DemoOptionPoint point = demo_echo_optional_point((DemoOptionPoint){true, {1.5, -2.5}});
    CHECK(point.has_value && point.value.x == 1.5 && point.value.y == -2.5, "case:options.complex.point.should_roundtrip_some");
    CHECK(!demo_echo_optional_point((DemoOptionPoint){0}).has_value, "case:options.complex.point.should_roundtrip_none");
    point = demo_make_some_point(3.5, -4.5);
    CHECK(point.has_value && point.value.x == 3.5 && point.value.y == -4.5, "case:options.complex.point.should_make_some");
    CHECK(!demo_make_none_point().has_value, "case:options.complex.point.should_make_none");
    DemoOptionStatus status = demo_echo_optional_status((DemoOptionStatus){true, DEMO_STATUS_PENDING});
    CHECK(status.has_value && status.value == DEMO_STATUS_PENDING, "case:options.complex.status.should_roundtrip_some");
    CHECK(!demo_echo_optional_status((DemoOptionStatus){0}).has_value, "case:options.complex.status.should_roundtrip_none");

    const int32_t values[] = {INT32_MIN, 0, INT32_MAX};
    const DemoOptionOfI32Slice numbers = {true, {values, 3}};
    DemoOptionOfI32Sequence returned_numbers = demo_echo_optional_vec(numbers);
    CHECK(returned_numbers.has_value && returned_numbers.value.len == 3 && memcmp(returned_numbers.value.ptr, values, sizeof(values)) == 0, "case:options.complex.vec.should_roundtrip_some");
    demo_option_of_i32_sequence_free(&returned_numbers);
    returned_numbers = demo_echo_optional_vec((DemoOptionOfI32Slice){0});
    CHECK(!returned_numbers.has_value, "case:options.complex.vec.should_roundtrip_none");
    demo_option_of_i32_sequence_free(&returned_numbers);
    returned_numbers = demo_echo_optional_vec((DemoOptionOfI32Slice){true, {NULL, 0}});
    CHECK(returned_numbers.has_value && returned_numbers.value.len == 0, "case:options.complex.vec.should_roundtrip_empty_some");
    demo_option_of_i32_sequence_free(&returned_numbers);
    DemoOptionU32 length = demo_optional_vec_length(numbers);
    CHECK(length.has_value && length.value == 3, "case:options.complex.vec.should_report_length_for_some");
    CHECK(!demo_optional_vec_length((DemoOptionOfI32Slice){0}).has_value, "case:options.complex.vec.should_return_none_for_absent_length");
    returned_name = demo_find_name(7);
    CHECK(returned_name.has_value && returned_name.value.len == 6 && memcmp(returned_name.value.ptr, "Name_7", 6) == 0, "case:options.complex.string.should_find_name_for_positive_id");
    demo_option_string_free(&returned_name);
    returned_name = demo_find_name(0);
    CHECK(!returned_name.has_value, "case:options.complex.string.should_return_none_for_non_positive_id");
    demo_option_string_free(&returned_name);
    returned_numbers = demo_find_numbers(3);
    CHECK(returned_numbers.has_value && returned_numbers.value.len == 3 && returned_numbers.value.ptr[0] == 0 && returned_numbers.value.ptr[2] == 2, "case:options.complex.vec.should_find_numbers_for_positive_count");
    demo_option_of_i32_sequence_free(&returned_numbers);
    returned_numbers = demo_find_numbers(0);
    CHECK(!returned_numbers.has_value, "case:options.complex.vec.should_return_none_for_non_positive_number_count");
    demo_option_of_i32_sequence_free(&returned_numbers);
    DemoOptionOfStringSequence names = demo_find_names(2);
    CHECK(names.has_value && names.value.len == 2 && names.value.ptr[0].len == 6 && memcmp(names.value.ptr[0].ptr, "Name_0", 6) == 0 && names.value.ptr[1].len == 6 && memcmp(names.value.ptr[1].ptr, "Name_1", 6) == 0, "case:options.complex.vec_string.should_find_names_for_positive_count");
    demo_option_of_string_sequence_free(&names);
    names = demo_find_names(0);
    CHECK(!names.has_value, "case:options.complex.vec_string.should_return_none_for_non_positive_name_count");
    demo_option_of_string_sequence_free(&names);
    const DemoOptionI32 mixed[] = {{true, INT32_MIN}, {false, 42}, {true, INT32_MAX}};
    DemoSequenceOfOptionI32 returned_mixed = demo_echo_vec_optional_i32((DemoSliceOfOptionI32){mixed, 3});
    CHECK(returned_mixed.len == 3 && returned_mixed.ptr[0].has_value && returned_mixed.ptr[0].value == INT32_MIN && !returned_mixed.ptr[1].has_value && returned_mixed.ptr[2].has_value && returned_mixed.ptr[2].value == INT32_MAX, "case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence");
    demo_sequence_of_option_i32_free(&returned_mixed);
    returned_mixed = demo_echo_vec_optional_i32((DemoSliceOfOptionI32){NULL, 0});
    CHECK(returned_mixed.len == 0, "case:options.complex.vec_optional_i32.should_roundtrip_empty");
    demo_sequence_of_option_i32_free(&returned_mixed);
    const DemoOptionI32 absent[] = {{false, 0}, {false, 0}};
    returned_mixed = demo_echo_vec_optional_i32((DemoSliceOfOptionI32){absent, 2});
    CHECK(returned_mixed.len == 2 && !returned_mixed.ptr[0].has_value && !returned_mixed.ptr[1].has_value, "case:options.complex.vec_optional_i32.should_roundtrip_all_none");
    demo_sequence_of_option_i32_free(&returned_mixed);
    return true;
}
