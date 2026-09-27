#include <math.h>
#include <stdint.h>

#include "test.h"

bool test_scalars(void) {
    CHECK(DEMO_ANSWER == 42, "exported integer constant");
    CHECK(demo_echo_bool(true), "case:primitives.scalars.bool.should_roundtrip_true");
    CHECK(demo_negate_bool(false), "case:primitives.scalars.bool.should_negate_false_to_true");
    CHECK(demo_echo_i8(-7) == -7, "case:primitives.scalars.i8.should_roundtrip_negative_value");
    CHECK(demo_echo_u8(UINT8_MAX) == UINT8_MAX, "case:primitives.scalars.u8.should_roundtrip_max_value");
    CHECK(demo_echo_i16(-1234) == -1234, "case:primitives.scalars.i16.should_roundtrip_negative_value");
    CHECK(demo_echo_u16(55000) == 55000, "case:primitives.scalars.u16.should_roundtrip_large_value");
    CHECK(demo_echo_i32(-42) == -42, "case:primitives.scalars.i32.should_roundtrip_negative_value");
    CHECK(demo_add_i32(10, 20) == 30, "case:primitives.scalars.i32.should_add_two_values");
    puts("case:primitives.scalars.named_status.should_accept_both_names");
    demo_notify_status_collision(7, 11);
    CHECK(demo_add(2, 3) == 5, "case:primitives.scalars.i32.should_add_with_benchmark_alias");
    CHECK(demo_echo_u32(UINT32_C(4000000000)) == UINT32_C(4000000000), "case:primitives.scalars.u32.should_roundtrip_large_value");
    CHECK(demo_echo_i64(-INT64_C(9999999999)) == -INT64_C(9999999999), "case:primitives.scalars.i64.should_roundtrip_large_negative_value");
    CHECK(demo_echo_u64(UINT64_C(9999999999)) == UINT64_C(9999999999), "case:primitives.scalars.u64.should_roundtrip_large_value");
    CHECK(fabsf(demo_echo_f32(3.5f) - 3.5f) < 1e-6f, "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance");
    CHECK(fabsf(demo_add_f32(1.5f, 2.5f) - 4.0f) < 1e-6f, "case:primitives.scalars.f32.should_add_two_values_with_tolerance");
    CHECK(fabs(demo_echo_f64(3.14159265359) - 3.14159265359) < 1e-12, "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance");
    CHECK(fabs(demo_add_f64(1.5, 2.5) - 4.0) < 1e-12, "case:primitives.scalars.f64.should_add_two_values_with_tolerance");
    CHECK(fabs(demo_multiply(1.5, 2.5) - 3.75) < 1e-12, "case:primitives.scalars.f64.should_multiply_two_values");
    CHECK(demo_echo_usize(123) == 123, "case:primitives.scalars.usize.should_roundtrip_value");
    CHECK(demo_echo_isize(-123) == -123, "case:primitives.scalars.isize.should_roundtrip_negative_value");
    puts("case:primitives.scalars.noop.should_cross_without_values");
    demo_noop();
    return true;
}
