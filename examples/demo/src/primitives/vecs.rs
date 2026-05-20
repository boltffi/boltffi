use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_roundtrip_non_empty",
    justification = "Ensure a non-empty i32 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_i32` through the generated binding and assert a non-empty i32 vector crosses the wire and returns unchanged."
)]
#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_roundtrip_empty",
    justification = "Ensure an empty i32 vector crosses the wire and returns as an empty vector.",
    directions = "Call `primitives::vecs::echo_vec_i32` through the generated binding and assert an empty i32 vector crosses the wire and returns as an empty vector."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_vec_i32(v: Vec<i32>) -> Vec<i32> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i8.should_roundtrip_values",
    justification = "Ensure a non-empty i8 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_i8` through the generated binding and assert a non-empty i8 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_i8(v: Vec<i8>) -> Vec<i8> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u8.should_roundtrip_values",
    justification = "Ensure a non-empty u8 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_u8` through the generated binding and assert a non-empty u8 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u8(v: Vec<u8>) -> Vec<u8> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i16.should_roundtrip_values",
    justification = "Ensure a non-empty i16 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_i16` through the generated binding and assert a non-empty i16 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_i16(v: Vec<i16>) -> Vec<i16> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u16.should_roundtrip_values",
    justification = "Ensure a non-empty u16 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_u16` through the generated binding and assert a non-empty u16 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u16(v: Vec<u16>) -> Vec<u16> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u32.should_roundtrip_values",
    justification = "Ensure a non-empty u32 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_u32` through the generated binding and assert a non-empty u32 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u32(v: Vec<u32>) -> Vec<u32> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i64.should_roundtrip_values",
    justification = "Ensure a non-empty i64 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_i64` through the generated binding and assert a non-empty i64 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_i64(v: Vec<i64>) -> Vec<i64> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u64.should_roundtrip_values",
    justification = "Ensure a non-empty u64 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_u64` through the generated binding and assert a non-empty u64 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u64(v: Vec<u64>) -> Vec<u64> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.isize.should_roundtrip_values",
    justification = "Ensure a non-empty isize vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_isize` through the generated binding and assert a non-empty isize vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_isize(v: Vec<isize>) -> Vec<isize> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.usize.should_roundtrip_values",
    justification = "Ensure a non-empty usize vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_usize` through the generated binding and assert a non-empty usize vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_usize(v: Vec<usize>) -> Vec<usize> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.f32.should_roundtrip_values_with_tolerance",
    justification = "Ensure a non-empty f32 vector crosses the wire and returns unchanged within tolerance.",
    directions = "Call `primitives::vecs::echo_vec_f32` through the generated binding and assert a non-empty f32 vector crosses the wire and returns unchanged within tolerance."
)]
#[export]
pub fn echo_vec_f32(v: Vec<f32>) -> Vec<f32> {
    v
}

/// Sums all elements in the vector. Uses i64 to avoid overflow
/// on large inputs.
#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_sum_values",
    justification = "Ensure an i32 vector crosses the wire and returns as the sum of its values.",
    directions = "Call `primitives::vecs::sum_vec_i32` through the generated binding and assert an i32 vector crosses the wire and returns as the sum of its values."
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn sum_vec_i32(v: Vec<i32>) -> i64 {
    v.iter().map(|&x| x as i64).sum()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.f64.should_roundtrip_values",
    justification = "Ensure a non-empty f64 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_f64` through the generated binding and assert a non-empty f64 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_f64(v: Vec<f64>) -> Vec<f64> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.bool.should_roundtrip_values",
    justification = "Ensure a non-empty boolean vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_bool` through the generated binding and assert a non-empty boolean vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_bool(v: Vec<bool>) -> Vec<bool> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.string.should_roundtrip_values",
    justification = "Ensure a non-empty string vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_string` through the generated binding and assert a non-empty string vector crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Vec<String>. Include this case when string-vector support is implemented for Python."
    )
)]
#[export]
pub fn echo_vec_string(v: Vec<String>) -> Vec<String> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.string.should_report_utf8_byte_lengths",
    justification = "Ensure a string vector crosses the wire and returns UTF-8 byte lengths for each string.",
    directions = "Call `primitives::vecs::vec_string_lengths` through the generated binding and assert a string vector crosses the wire and returns UTF-8 byte lengths for each string.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Vec<String>. Include this case when string-vector support is implemented for Python."
    )
)]
#[export]
pub fn vec_string_lengths(v: Vec<String>) -> Vec<u32> {
    v.iter().map(|s| s.len() as u32).collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_make_range",
    justification = "Ensure Start and end bounds cross the wire and return as an i32 range.",
    directions = "Call `primitives::vecs::make_range` through the generated binding and assert Start and end bounds cross the wire and return as an i32 range."
)]
#[export]
pub fn make_range(start: i32, end: i32) -> Vec<i32> {
    (start..end).collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_reverse_values",
    justification = "Ensure a non-empty i32 vector crosses the wire and returns in reverse order.",
    directions = "Call `primitives::vecs::reverse_vec_i32` through the generated binding and assert a non-empty i32 vector crosses the wire and returns in reverse order."
)]
#[export]
pub fn reverse_vec_i32(v: Vec<i32>) -> Vec<i32> {
    v.into_iter().rev().collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_generate_sequence",
    justification = "Ensure an i32 count crosses the wire and returns a generated i32 sequence.",
    directions = "Call `primitives::vecs::generate_i32_vec` through the generated binding and assert an i32 count crosses the wire and returns a generated i32 sequence.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive vector returns, but the demo suite has no assertion for this benchmark vector generator yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_i32_vec(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_sum_benchmark_values",
    justification = "Ensure an i32 vector crosses the wire through the benchmark sum helper and returns as an i64 sum.",
    directions = "Call `primitives::vecs::sum_i32_vec` through the generated binding and assert an i32 vector crosses the wire through the benchmark sum helper and returns as an i64 sum.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive vector parameters and primitive returns, but the demo suite has no assertion for this benchmark sum helper yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_i32_vec(values: Vec<i32>) -> i64 {
    values.iter().map(|&value| i64::from(value)).sum()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.f64.should_generate_sequence",
    justification = "Ensure an i32 count crosses the wire and returns a generated f64 sequence.",
    directions = "Call `primitives::vecs::generate_f64_vec` through the generated binding and assert an i32 count crosses the wire and returns a generated f64 sequence.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive vector returns, but the demo suite has no assertion for this benchmark vector generator yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_f64_vec(count: i32) -> Vec<f64> {
    (0..count).map(|index| f64::from(index) * 0.1).collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.f64.should_sum_values",
    justification = "Ensure a f64 vector crosses the wire and returns as a f64 sum.",
    directions = "Call `primitives::vecs::sum_f64_vec` through the generated binding and assert a f64 vector crosses the wire and returns as a f64 sum.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive vector parameters and f64 returns, but the demo suite has no assertion for this vector sum helper yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_f64_vec(values: Vec<f64>) -> f64 {
    values.iter().sum()
}

/// BoltFFI benchmarks use the in-place slice form; UniFFI benchmarks use `inc_u64_value`.
#[demo_bench_macros::demo_case(
    "primitives.vecs.u64.should_increment_first_value_in_place",
    justification = "Ensure a mutable u64 slice crosses the wire and increments its first value in place.",
    directions = "Call `primitives::vecs::inc_u64` through the generated binding and assert a mutable u64 slice crosses the wire and increments its first value in place.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer only accepts value parameters today, so mutable slice parameters are omitted. Include this case when in-place sequence parameters are implemented for Python."
    )
)]
#[export]
pub fn inc_u64(values: &mut [u64]) {
    if let Some(first) = values.first_mut() {
        *first += 1;
    }
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u64.should_increment_value",
    justification = "Ensure a u64 value crosses the wire and returns incremented by one.",
    directions = "Call `primitives::vecs::inc_u64_value` through the generated binding and assert a u64 value crosses the wire and returns incremented by one.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports primitive scalar calls, but the demo suite has no assertion for this benchmark increment helper yet."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn inc_u64_value(value: u64) -> u64 {
    value + 1
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_i32.should_roundtrip_values",
    justification = "Ensure a nested i32 vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_vec_i32` through the generated binding and assert a nested i32 vector crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_i32.should_roundtrip_empty_outer",
    justification = "Ensure an empty outer i32 vector crosses the wire and returns as an empty nested vector.",
    directions = "Call `primitives::vecs::echo_vec_vec_i32` through the generated binding and assert an empty outer i32 vector crosses the wire and returns as an empty nested vector.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    )
)]
#[export]
pub fn echo_vec_vec_i32(v: Vec<Vec<i32>>) -> Vec<Vec<i32>> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_bool.should_roundtrip_values",
    justification = "Ensure a nested boolean vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_vec_bool` through the generated binding and assert a nested boolean vector crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    )
)]
#[export]
pub fn echo_vec_vec_bool(v: Vec<Vec<bool>>) -> Vec<Vec<bool>> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_isize.should_roundtrip_values",
    justification = "Ensure a nested isize vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_vec_isize` through the generated binding and assert a nested isize vector crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "#203: TypeScript nested Vec<Vec<isize>> lowering passes plain Number values to BigInt setters in the generated writer. Include this case when the nested isize writer coerces elements to BigInt."
    )
)]
#[export]
pub fn echo_vec_vec_isize(v: Vec<Vec<isize>>) -> Vec<Vec<isize>> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_usize.should_roundtrip_values",
    justification = "Ensure a nested usize vector crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_vec_usize` through the generated binding and assert a nested usize vector crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "#203: TypeScript nested Vec<Vec<usize>> lowering passes plain Number values to BigInt setters in the generated writer. Include this case when the nested usize writer coerces elements to BigInt."
    )
)]
#[export]
pub fn echo_vec_vec_usize(v: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_string.should_roundtrip_utf8_values",
    justification = "Ensure a nested string vector with UTF-8 values crosses the wire and returns unchanged.",
    directions = "Call `primitives::vecs::echo_vec_vec_string` through the generated binding and assert a nested string vector with UTF-8 values crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    )
)]
#[export]
pub fn echo_vec_vec_string(v: Vec<Vec<String>>) -> Vec<Vec<String>> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_i32.should_flatten_values",
    justification = "Ensure a nested i32 vector crosses the wire and returns as a flattened i32 vector.",
    directions = "Call `primitives::vecs::flatten_vec_vec_i32` through the generated binding and assert a nested i32 vector crosses the wire and returns as a flattened i32 vector.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "primitives.vecs.nested_i32.should_flatten_empty",
    justification = "Ensure an empty nested i32 vector crosses the wire and returns as an empty i32 vector.",
    directions = "Call `primitives::vecs::flatten_vec_vec_i32` through the generated binding and assert an empty nested i32 vector crosses the wire and returns as an empty i32 vector.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle nested Vec<T> shapes. Include this case when nested vectors are implemented for Python."
    )
)]
#[export]
pub fn flatten_vec_vec_i32(v: Vec<Vec<i32>>) -> Vec<i32> {
    v.into_iter().flatten().collect()
}
