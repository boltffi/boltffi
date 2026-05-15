use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_roundtrip_non_empty",
    description = "A non-empty i32 vector crosses the wire and returns unchanged."
)]
#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_roundtrip_empty",
    description = "An empty i32 vector crosses the wire and returns as an empty vector."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_vec_i32(v: Vec<i32>) -> Vec<i32> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i8.should_roundtrip_values",
    description = "A non-empty i8 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_i8(v: Vec<i8>) -> Vec<i8> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u8.should_roundtrip_values",
    description = "A non-empty u8 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u8(v: Vec<u8>) -> Vec<u8> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i16.should_roundtrip_values",
    description = "A non-empty i16 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_i16(v: Vec<i16>) -> Vec<i16> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u16.should_roundtrip_values",
    description = "A non-empty u16 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u16(v: Vec<u16>) -> Vec<u16> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u32.should_roundtrip_values",
    description = "A non-empty u32 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u32(v: Vec<u32>) -> Vec<u32> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i64.should_roundtrip_values",
    description = "A non-empty i64 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_i64(v: Vec<i64>) -> Vec<i64> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.u64.should_roundtrip_values",
    description = "A non-empty u64 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_u64(v: Vec<u64>) -> Vec<u64> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.isize.should_roundtrip_values",
    description = "A non-empty isize vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_isize(v: Vec<isize>) -> Vec<isize> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.usize.should_roundtrip_values",
    description = "A non-empty usize vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_usize(v: Vec<usize>) -> Vec<usize> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.f32.should_roundtrip_values_with_tolerance",
    description = "A non-empty f32 vector crosses the wire and returns unchanged within tolerance."
)]
#[export]
pub fn echo_vec_f32(v: Vec<f32>) -> Vec<f32> {
    v
}

/// Sums all elements in the vector. Uses i64 to avoid overflow
/// on large inputs.
#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_sum_values",
    description = "An i32 vector crosses the wire and returns as the sum of its values."
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn sum_vec_i32(v: Vec<i32>) -> i64 {
    v.iter().map(|&x| x as i64).sum()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.f64.should_roundtrip_values",
    description = "A non-empty f64 vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_f64(v: Vec<f64>) -> Vec<f64> {
    v
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.bool.should_roundtrip_values",
    description = "A non-empty boolean vector crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_vec_bool(v: Vec<bool>) -> Vec<bool> {
    v
}

#[export]
pub fn echo_vec_string(v: Vec<String>) -> Vec<String> {
    v
}

#[export]
pub fn vec_string_lengths(v: Vec<String>) -> Vec<u32> {
    v.iter().map(|s| s.len() as u32).collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_make_range",
    description = "Start and end bounds cross the wire and return as an i32 range."
)]
#[export]
pub fn make_range(start: i32, end: i32) -> Vec<i32> {
    (start..end).collect()
}

#[demo_bench_macros::demo_case(
    "primitives.vecs.i32.should_reverse_values",
    description = "A non-empty i32 vector crosses the wire and returns in reverse order."
)]
#[export]
pub fn reverse_vec_i32(v: Vec<i32>) -> Vec<i32> {
    v.into_iter().rev().collect()
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_i32_vec(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_i32_vec(values: Vec<i32>) -> i64 {
    values.iter().map(|&value| i64::from(value)).sum()
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_f64_vec(count: i32) -> Vec<f64> {
    (0..count).map(|index| f64::from(index) * 0.1).collect()
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_f64_vec(values: Vec<f64>) -> f64 {
    values.iter().sum()
}

/// BoltFFI benchmarks use the in-place slice form; UniFFI benchmarks use `inc_u64_value`.
#[export]
pub fn inc_u64(values: &mut [u64]) {
    if let Some(first) = values.first_mut() {
        *first += 1;
    }
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn inc_u64_value(value: u64) -> u64 {
    value + 1
}

#[export]
pub fn echo_vec_vec_i32(v: Vec<Vec<i32>>) -> Vec<Vec<i32>> {
    v
}

#[export]
pub fn echo_vec_vec_bool(v: Vec<Vec<bool>>) -> Vec<Vec<bool>> {
    v
}

#[export]
pub fn echo_vec_vec_isize(v: Vec<Vec<isize>>) -> Vec<Vec<isize>> {
    v
}

#[export]
pub fn echo_vec_vec_usize(v: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    v
}

#[export]
pub fn echo_vec_vec_string(v: Vec<Vec<String>>) -> Vec<Vec<String>> {
    v
}

#[export]
pub fn flatten_vec_vec_i32(v: Vec<Vec<i32>>) -> Vec<i32> {
    v.into_iter().flatten().collect()
}
