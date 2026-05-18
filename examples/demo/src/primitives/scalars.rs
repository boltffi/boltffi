use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[demo_bench_macros::demo_case(
    "primitives.scalars.bool.should_roundtrip_true",
    justification = "Ensure a true boolean crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_bool` through the generated binding and assert a true boolean crosses the wire and returns unchanged."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_bool(v: bool) -> bool {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.bool.should_negate_false_to_true",
    justification = "Ensure a false boolean crosses the wire and returns as true through the negation helper.",
    directions = "Call `primitives::scalars::negate_bool` through the generated binding and assert a false boolean crosses the wire and returns as true through the negation helper."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn negate_bool(v: bool) -> bool {
    !v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.i8.should_roundtrip_negative_value",
    justification = "Ensure a negative i8 value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_i8` through the generated binding and assert a negative i8 value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_i8(v: i8) -> i8 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.u8.should_roundtrip_max_value",
    justification = "Ensure a maximum u8 value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_u8` through the generated binding and assert a maximum u8 value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_u8(v: u8) -> u8 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.i16.should_roundtrip_negative_value",
    justification = "Ensure a negative i16 value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_i16` through the generated binding and assert a negative i16 value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_i16(v: i16) -> i16 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.u16.should_roundtrip_large_value",
    justification = "Ensure a large u16 value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_u16` through the generated binding and assert a large u16 value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_u16(v: u16) -> u16 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.i32.should_roundtrip_negative_value",
    justification = "Ensure a negative i32 crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_i32` through the generated binding and assert a negative i32 crosses the wire and returns unchanged."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_i32(v: i32) -> i32 {
    v
}

/// Adds two 32-bit signed integers and returns the result.
#[demo_bench_macros::demo_case(
    "primitives.scalars.i32.should_add_two_values",
    justification = "Ensure two i32 values cross the wire and return as their sum.",
    directions = "Call `primitives::scalars::add_i32` through the generated binding and assert two i32 values cross the wire and return as their sum."
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn add_i32(a: i32, b: i32) -> i32 {
    a + b
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.u32.should_roundtrip_large_value",
    justification = "Ensure a large u32 value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_u32` through the generated binding and assert a large u32 value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_u32(v: u32) -> u32 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.i64.should_roundtrip_large_negative_value",
    justification = "Ensure a large negative i64 crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_i64` through the generated binding and assert a large negative i64 crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_i64(v: i64) -> i64 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.u64.should_roundtrip_large_value",
    justification = "Ensure a large u64 value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_u64` through the generated binding and assert a large u64 value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_u64(v: u64) -> u64 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.f32.should_roundtrip_value_with_tolerance",
    justification = "Ensure an f32 crosses the wire and returns within the expected floating-point tolerance.",
    directions = "Call `primitives::scalars::echo_f32` through the generated binding and assert an f32 crosses the wire and returns within the expected floating-point tolerance."
)]
#[export]
pub fn echo_f32(v: f32) -> f32 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.f32.should_add_two_values_with_tolerance",
    justification = "Ensure two f32 values cross the wire and return as their sum within tolerance.",
    directions = "Call `primitives::scalars::add_f32` through the generated binding and assert two f32 values cross the wire and return as their sum within tolerance."
)]
#[export]
pub fn add_f32(a: f32, b: f32) -> f32 {
    a + b
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.f64.should_roundtrip_pi_with_tolerance",
    justification = "Ensure a high-precision f64 crosses the wire and returns within tolerance.",
    directions = "Call `primitives::scalars::echo_f64` through the generated binding and assert a high-precision f64 crosses the wire and returns within tolerance."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_f64(v: f64) -> f64 {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.f64.should_add_two_values_with_tolerance",
    justification = "Ensure two f64 values cross the wire and return as their sum within tolerance.",
    directions = "Call `primitives::scalars::add_f64` through the generated binding and assert two f64 values cross the wire and return as their sum within tolerance."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn add_f64(a: f64, b: f64) -> f64 {
    a + b
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.usize.should_roundtrip_value",
    justification = "Ensure a usize value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_usize` through the generated binding and assert a usize value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_usize(v: usize) -> usize {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.isize.should_roundtrip_negative_value",
    justification = "Ensure a negative isize value crosses the wire and returns unchanged.",
    directions = "Call `primitives::scalars::echo_isize` through the generated binding and assert a negative isize value crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_isize(v: isize) -> isize {
    v
}

/// A no-op call used to measure raw FFI overhead.
#[demo_bench_macros::demo_case(
    "primitives.scalars.noop.should_cross_without_values",
    justification = "Ensure a no-argument no-op call crosses the wire without returning a value.",
    directions = "Call `primitives::scalars::noop` through the generated binding and assert a no-argument no-op call crosses the wire without returning a value.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for the scalar noop helper in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive scalar calls, but the demo suite has no assertion for the scalar noop benchmark helper yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn noop() {}

#[demo_bench_macros::demo_case(
    "primitives.scalars.i32.should_add_with_benchmark_alias",
    justification = "Ensure two i32 values cross the wire through the benchmark add alias and return as their sum.",
    directions = "Call `primitives::scalars::add` through the generated binding and assert two i32 values cross the wire through the benchmark add alias and return as their sum.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for the benchmark add alias in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive scalar calls, but the demo suite has no assertion for the benchmark add alias yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn add(a: i32, b: i32) -> i32 {
    add_i32(a, b)
}

#[demo_bench_macros::demo_case(
    "primitives.scalars.f64.should_multiply_two_values",
    justification = "Ensure two f64 values cross the wire and return as their product.",
    directions = "Call `primitives::scalars::multiply` through the generated binding and assert two f64 values cross the wire and return as their product.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for the scalar multiply helper in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive scalar calls, but the demo suite has no assertion for the scalar multiply helper yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn multiply(a: f64, b: f64) -> f64 {
    a * b
}
