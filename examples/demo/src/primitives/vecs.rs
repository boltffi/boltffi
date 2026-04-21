use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_vec_i32(v: Vec<i32>) -> Vec<i32> {
    v
}

#[export]
pub fn echo_vec_i8(v: Vec<i8>) -> Vec<i8> {
    v
}

#[export]
pub fn echo_vec_u8(v: Vec<u8>) -> Vec<u8> {
    v
}

#[export]
pub fn echo_vec_i16(v: Vec<i16>) -> Vec<i16> {
    v
}

#[export]
pub fn echo_vec_u16(v: Vec<u16>) -> Vec<u16> {
    v
}

#[export]
pub fn echo_vec_u32(v: Vec<u32>) -> Vec<u32> {
    v
}

#[export]
pub fn echo_vec_i64(v: Vec<i64>) -> Vec<i64> {
    v
}

#[export]
pub fn echo_vec_u64(v: Vec<u64>) -> Vec<u64> {
    v
}

#[export]
pub fn echo_vec_isize(v: Vec<isize>) -> Vec<isize> {
    v
}

#[export]
pub fn echo_vec_usize(v: Vec<usize>) -> Vec<usize> {
    v
}

#[export]
pub fn echo_vec_f32(v: Vec<f32>) -> Vec<f32> {
    v
}

/// Sums all elements in the vector. Uses i64 to avoid overflow
/// on large inputs.
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn sum_vec_i32(v: Vec<i32>) -> i64 {
    v.iter().map(|&x| x as i64).sum()
}

#[export]
pub fn echo_vec_f64(v: Vec<f64>) -> Vec<f64> {
    v
}

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

#[export]
pub fn make_range(start: i32, end: i32) -> Vec<i32> {
    (start..end).collect()
}

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
