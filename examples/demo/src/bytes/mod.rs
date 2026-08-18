use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[demo_bench_macros::demo_case(
    "bytes.bytes.should_roundtrip_values",
    justification = "Ensure a byte buffer crosses the wire and returns unchanged.",
    directions = "Call `bytes::echo_bytes` through the generated binding and assert a byte buffer crosses the wire and returns unchanged."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_bytes(data: Vec<u8>) -> Vec<u8> {
    data
}

#[demo_bench_macros::demo_case(
    "bytes.bytes.should_report_length",
    justification = "Ensure a byte buffer crosses the wire and returns its element count.",
    directions = "Call `bytes::bytes_length` through the generated binding and assert a byte buffer crosses the wire and returns its element count."
)]
#[export]
pub fn bytes_length(data: Vec<u8>) -> u32 {
    data.len() as u32
}

#[demo_bench_macros::demo_case(
    "bytes.bytes.should_sum_values",
    justification = "Ensure a byte buffer crosses the wire and returns the sum of its values.",
    directions = "Call `bytes::bytes_sum` through the generated binding and assert a byte buffer crosses the wire and returns the sum of its values."
)]
#[export]
pub fn bytes_sum(data: Vec<u8>) -> u32 {
    data.iter().map(|&b| b as u32).sum()
}

#[demo_bench_macros::demo_case(
    "bytes.bytes.should_make_sequential_values",
    justification = "Ensure a requested byte-buffer length crosses the wire and returns sequential values.",
    directions = "Call `bytes::make_bytes` through the generated binding and assert a requested byte-buffer length crosses the wire and returns sequential values."
)]
#[export]
pub fn make_bytes(len: u32) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

#[demo_bench_macros::demo_case(
    "bytes.bytes.should_reverse_values",
    justification = "Ensure a byte buffer crosses the wire and returns in reverse order.",
    directions = "Call `bytes::reverse_bytes` through the generated binding and assert a byte buffer crosses the wire and returns in reverse order."
)]
#[export]
pub fn reverse_bytes(data: Vec<u8>) -> Vec<u8> {
    data.into_iter().rev().collect()
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_bytes(size: i32) -> Vec<u8> {
    vec![42u8; size.max(0) as usize]
}

/// Returns a buffer whose capacity overshoots its length.
///
/// `generate_bytes` hands back a `Vec` the allocator sized exactly, which is
/// the shape almost nothing real produces. A decompressor, a serializer, any
/// loop that pushes without knowing the final count — all of them land on a
/// capacity the growth curve picked, not the one the payload needs. That is
/// the case the packed return has to handle without copying.
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_bytes_overallocated(size: i32) -> Vec<u8> {
    let size = size.max(0) as usize;
    let mut out = Vec::with_capacity(size + size / 8 + 64);
    out.resize(size, 42u8);
    out
}
