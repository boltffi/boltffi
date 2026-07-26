use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[demo_bench_macros::demo_case(
    "primitives.strings.string.should_roundtrip_empty",
    justification = "Ensure an empty host string crosses the wire and returns as an empty string.",
    directions = "Call `primitives::strings::echo_string` through the generated binding and assert an empty host string crosses the wire and returns as an empty string."
)]
#[demo_bench_macros::demo_case(
    "primitives.strings.string.should_roundtrip_emoji",
    justification = "Ensure a host string containing an emoji crosses the wire and returns unchanged.",
    directions = "Call `primitives::strings::echo_string` through the generated binding and assert a host string containing an emoji crosses the wire and returns unchanged."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_string(v: String) -> String {
    v
}

/// Concatenates two strings and returns the combined result.
#[demo_bench_macros::demo_case(
    "primitives.strings.string.should_concatenate_values",
    justification = "Ensure two non-empty host strings cross the wire and return as one concatenated string.",
    directions = "Call `primitives::strings::concat_strings` through the generated binding and assert two non-empty host strings cross the wire and return as one concatenated string."
)]
#[export]
pub fn concat_strings(a: String, b: String) -> String {
    format!("{}{}", a, b)
}

#[demo_bench_macros::demo_case(
    "primitives.strings.string.should_report_utf8_byte_length",
    justification = "Ensure a string containing a non-ASCII code point reports its UTF-8 byte length.",
    directions = "Call `primitives::strings::string_length` through the generated binding and assert a string containing a non-ASCII code point reports its UTF-8 byte length."
)]
#[export]
pub fn string_length(v: String) -> u32 {
    v.len() as u32
}

#[demo_bench_macros::demo_case(
    "primitives.strings.string.should_detect_empty",
    justification = "Ensure an empty host string crosses the wire and is reported as empty.",
    directions = "Call `primitives::strings::string_is_empty` through the generated binding and assert an empty host string crosses the wire and is reported as empty."
)]
#[export]
pub fn string_is_empty(v: String) -> bool {
    v.is_empty()
}

#[demo_bench_macros::demo_case(
    "primitives.strings.string.should_repeat_value",
    justification = "Ensure a host string and repeat count cross the wire and return the repeated string.",
    directions = "Call `primitives::strings::repeat_string` through the generated binding and assert a host string and repeat count cross the wire and return the repeated string."
)]
#[export]
pub fn repeat_string(v: String, count: u32) -> String {
    v.repeat(count as usize)
}

#[demo_bench_macros::demo_case(
    "primitives.strings.borrowed_static_string.should_return_value",
    justification = "Ensure an exported borrowed static string return is encoded as a host string.",
    directions = "Call `primitives::strings::borrowed_static_string` through the generated binding and assert it returns the expected host string."
)]
#[export]
pub fn borrowed_static_string() -> &'static str {
    "borrowed static"
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_string(size: i32) -> String {
    "x".repeat(size.max(0) as usize)
}
