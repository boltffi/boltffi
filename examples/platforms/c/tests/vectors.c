#include <limits.h>
#include <math.h>
#include <stdint.h>
#include <string.h>

#include "test.h"

#define CHECK_VECTOR(c_type, stem, member, scenario, ...)                     \
    do {                                                                      \
        const c_type values[] = {__VA_ARGS__};                                \
        const uintptr_t count = sizeof(values) / sizeof(values[0]);           \
        Demo##stem##Sequence returned =                                      \
            demo_echo_vec_##member((Demo##stem##Slice){values, count});       \
        CHECK(returned.len == count &&                                       \
              memcmp(returned.ptr, values, sizeof(values)) == 0, scenario);   \
        demo_##member##_sequence_free(&returned);                             \
        CHECK(returned.ptr == NULL && returned.len == 0, "vector cleanup");   \
    } while (0)

bool test_vectors(void) {
    CHECK_VECTOR(int8_t, I8, i8, "case:primitives.vecs.i8.should_roundtrip_values", INT8_MIN, 0, INT8_MAX);
    const uint8_t bytes[] = {0, 42, UINT8_MAX};
    DemoBytes returned_bytes = demo_echo_vec_u8((DemoBytesView){bytes, sizeof(bytes)});
    CHECK(returned_bytes.len == sizeof(bytes) && memcmp(returned_bytes.ptr, bytes, sizeof(bytes)) == 0, "case:primitives.vecs.u8.should_roundtrip_values");
    demo_bytes_free(&returned_bytes);
    CHECK_VECTOR(int16_t, I16, i16, "case:primitives.vecs.i16.should_roundtrip_values", INT16_MIN, 0, INT16_MAX);
    CHECK_VECTOR(uint16_t, U16, u16, "case:primitives.vecs.u16.should_roundtrip_values", 0, 42, UINT16_MAX);
    CHECK_VECTOR(int32_t, I32, i32, "case:primitives.vecs.i32.should_roundtrip_non_empty", INT32_MIN, 0, INT32_MAX);
    CHECK_VECTOR(uint32_t, U32, u32, "case:primitives.vecs.u32.should_roundtrip_values", 0, 42, UINT32_MAX);
    CHECK_VECTOR(int64_t, I64, i64, "case:primitives.vecs.i64.should_roundtrip_values", INT64_MIN, 0, INT64_MAX);
    CHECK_VECTOR(uint64_t, U64, u64, "case:primitives.vecs.u64.should_roundtrip_values", 0, 42, UINT64_MAX);
    CHECK_VECTOR(intptr_t, ISize, isize, "case:primitives.vecs.isize.should_roundtrip_values", INTPTR_MIN, 0, INTPTR_MAX);
    CHECK_VECTOR(uintptr_t, USize, usize, "case:primitives.vecs.usize.should_roundtrip_values", 0, 42, UINTPTR_MAX);
    CHECK_VECTOR(float, F32, f32, "case:primitives.vecs.f32.should_roundtrip_values_with_tolerance", -3.25f, 0.0f, 42.5f);
    CHECK_VECTOR(double, F64, f64, "case:primitives.vecs.f64.should_roundtrip_values", -3.25, -0.0, 42.5);
    CHECK_VECTOR(bool, Bool, bool, "case:primitives.vecs.bool.should_roundtrip_values", true, false, true);

    DemoI32Sequence empty = demo_echo_vec_i32((DemoI32Slice){NULL, 0});
    CHECK(empty.len == 0, "case:primitives.vecs.i32.should_roundtrip_empty");
    demo_i32_sequence_free(&empty);
    const int32_t numbers[] = {-10, 20, 30};
    CHECK(demo_sum_vec_i32((DemoI32Slice){numbers, 3}) == 40, "case:primitives.vecs.i32.should_sum_values");
    CHECK(demo_sum_i32_vec((DemoI32Slice){numbers, 3}) == 40, "case:primitives.vecs.i32.should_sum_benchmark_values");
    DemoI32Sequence range = demo_make_range(-2, 2);
    CHECK(range.len == 4 && range.ptr[0] == -2 && range.ptr[3] == 1, "case:primitives.vecs.i32.should_make_range");
    demo_i32_sequence_free(&range);
    DemoI32Sequence reversed = demo_reverse_vec_i32((DemoI32Slice){numbers, 3});
    CHECK(reversed.len == 3 && reversed.ptr[0] == 30 && reversed.ptr[2] == -10, "case:primitives.vecs.i32.should_reverse_values");
    demo_i32_sequence_free(&reversed);
    DemoI32Sequence generated = demo_generate_i32_vec(4);
    CHECK(generated.len == 4 && generated.ptr[0] == 0 && generated.ptr[3] == 3, "case:primitives.vecs.i32.should_generate_sequence");
    demo_i32_sequence_free(&generated);
    DemoF64Sequence reals = demo_generate_f64_vec(4);
    CHECK(reals.len == 4 && reals.ptr[0] == 0.0 && fabs(reals.ptr[3] - 0.3) < 1e-12, "case:primitives.vecs.f64.should_generate_sequence");
    CHECK(fabs(demo_sum_f64_vec((DemoF64Slice){reals.ptr, reals.len}) - 0.6) < 1e-12, "case:primitives.vecs.f64.should_sum_values");
    demo_f64_sequence_free(&reals);
    uint64_t mutable_numbers[] = {4, 8, 12};
    demo_inc_u64((DemoU64MutSlice){mutable_numbers, 3});
    CHECK(mutable_numbers[0] == 5 && mutable_numbers[1] == 8 && mutable_numbers[2] == 12, "case:primitives.vecs.u64.should_increment_first_value_in_place");
    CHECK(demo_inc_u64_value(41) == 42, "case:primitives.vecs.u64.should_increment_value");

    const DemoStringView strings[] = {{"hello", 5}, {"caf\xc3\xa9", 5}, {"", 0}};
    DemoStringSequence returned = demo_echo_vec_string((DemoStringSlice){strings, 3});
    CHECK(returned.len == 3 && returned.ptr[0].len == 5 && memcmp(returned.ptr[0].ptr, "hello", 5) == 0 && returned.ptr[1].len == 5 && memcmp(returned.ptr[1].ptr, strings[1].ptr, 5) == 0 && returned.ptr[2].len == 0, "case:primitives.vecs.string.should_roundtrip_values");
    demo_string_sequence_free(&returned);
    DemoU32Sequence lengths = demo_vec_string_lengths((DemoStringSlice){strings, 3});
    CHECK(lengths.len == 3 && lengths.ptr[0] == 5 && lengths.ptr[1] == 5 && lengths.ptr[2] == 0, "case:primitives.vecs.string.should_report_utf8_byte_lengths");
    demo_u32_sequence_free(&lengths);
    return true;
}

bool test_nested_vectors(void) {
    const int32_t numbers[] = {INT32_MIN, 42, INT32_MAX};
    const DemoI32Slice rows[] = {{numbers, 2}, {NULL, 0}, {numbers + 2, 1}};
    DemoSequenceOfI32Sequence returned = demo_echo_vec_vec_i32((DemoSliceOfI32Slice){rows, 3});
    CHECK(returned.len == 3 && returned.ptr[0].len == 2 && returned.ptr[0].ptr[0] == INT32_MIN && returned.ptr[0].ptr[1] == 42 && returned.ptr[1].len == 0 && returned.ptr[2].len == 1 && returned.ptr[2].ptr[0] == INT32_MAX, "case:primitives.vecs.nested_i32.should_roundtrip_values");
    demo_sequence_of_i32_sequence_free(&returned);
    returned = demo_echo_vec_vec_i32((DemoSliceOfI32Slice){NULL, 0});
    CHECK(returned.len == 0, "case:primitives.vecs.nested_i32.should_roundtrip_empty_outer");
    demo_sequence_of_i32_sequence_free(&returned);
    DemoI32Sequence flat = demo_flatten_vec_vec_i32((DemoSliceOfI32Slice){rows, 3});
    CHECK(flat.len == 3 && memcmp(flat.ptr, numbers, sizeof(numbers)) == 0, "case:primitives.vecs.nested_i32.should_flatten_values");
    demo_i32_sequence_free(&flat);
    flat = demo_flatten_vec_vec_i32((DemoSliceOfI32Slice){NULL, 0});
    CHECK(flat.len == 0, "case:primitives.vecs.nested_i32.should_flatten_empty");
    demo_i32_sequence_free(&flat);

    const bool flags[] = {true, false, true};
    const DemoBoolSlice flag_rows[] = {{flags, 2}, {NULL, 0}, {flags + 2, 1}};
    DemoSequenceOfBoolSequence nested_flags = demo_echo_vec_vec_bool((DemoSliceOfBoolSlice){flag_rows, 3});
    CHECK(nested_flags.len == 3 && nested_flags.ptr[0].len == 2 && nested_flags.ptr[0].ptr[0] && !nested_flags.ptr[0].ptr[1] && nested_flags.ptr[1].len == 0 && nested_flags.ptr[2].ptr[0], "case:primitives.vecs.nested_bool.should_roundtrip_values");
    demo_sequence_of_bool_sequence_free(&nested_flags);
    const intptr_t signed_values[] = {INTPTR_MIN, INTPTR_MAX};
    const DemoISizeSlice signed_rows[] = {{signed_values, 2}, {NULL, 0}};
    DemoSequenceOfISizeSequence signed_result = demo_echo_vec_vec_isize((DemoSliceOfISizeSlice){signed_rows, 2});
    CHECK(signed_result.len == 2 && signed_result.ptr[0].len == 2 && signed_result.ptr[0].ptr[0] == INTPTR_MIN && signed_result.ptr[0].ptr[1] == INTPTR_MAX && signed_result.ptr[1].len == 0, "case:primitives.vecs.nested_isize.should_roundtrip_values");
    demo_sequence_of_isize_sequence_free(&signed_result);
    const uintptr_t unsigned_values[] = {0, UINTPTR_MAX};
    const DemoUSizeSlice unsigned_rows[] = {{unsigned_values, 2}, {NULL, 0}};
    DemoSequenceOfUSizeSequence unsigned_result = demo_echo_vec_vec_usize((DemoSliceOfUSizeSlice){unsigned_rows, 2});
    CHECK(unsigned_result.len == 2 && unsigned_result.ptr[0].len == 2 && unsigned_result.ptr[0].ptr[0] == 0 && unsigned_result.ptr[0].ptr[1] == UINTPTR_MAX && unsigned_result.ptr[1].len == 0, "case:primitives.vecs.nested_usize.should_roundtrip_values");
    demo_sequence_of_usize_sequence_free(&unsigned_result);
    const DemoStringView names[] = {{"caf\xc3\xa9", 5}, {"", 0}};
    const DemoStringSlice name_rows[] = {{names, 2}, {NULL, 0}};
    DemoSequenceOfStringSequence nested_names = demo_echo_vec_vec_string((DemoSliceOfStringSlice){name_rows, 2});
    CHECK(nested_names.len == 2 && nested_names.ptr[0].len == 2 && nested_names.ptr[0].ptr[0].len == 5 && memcmp(nested_names.ptr[0].ptr[0].ptr, names[0].ptr, 5) == 0 && nested_names.ptr[0].ptr[1].len == 0 && nested_names.ptr[1].len == 0, "case:primitives.vecs.nested_string.should_roundtrip_utf8_values");
    demo_sequence_of_string_sequence_free(&nested_names);
    return true;
}
