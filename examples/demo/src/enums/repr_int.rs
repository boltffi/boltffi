use boltffi::*;

/// Task priority with explicit integer discriminants.
///
/// The `#[repr(i32)]` means these values are stable across
/// versions and safe to persist or send over the network.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Priority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.priority.should_roundtrip_value",
    description = "A repr(i32) Priority enum value crosses the FFI boundary and returns unchanged.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover Priority helper functions."
    )
)]
#[export]
pub fn echo_priority(p: Priority) -> Priority {
    p
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.priority.should_render_label",
    description = "priority_label maps Priority enum values to their string labels.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover Priority helper functions."
    )
)]
#[export]
pub fn priority_label(p: Priority) -> String {
    match p {
        Priority::Low => "low".to_string(),
        Priority::Medium => "medium".to_string(),
        Priority::High => "high".to_string(),
        Priority::Critical => "critical".to_string(),
    }
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.priority.should_identify_high_priority",
    description = "is_high_priority returns true for High and Critical priorities.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover Priority helper functions."
    )
)]
#[export]
pub fn is_high_priority(p: Priority) -> bool {
    matches!(p, Priority::High | Priority::Critical)
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.log_level.should_roundtrip_value",
    description = "A repr(u8) LogLevel enum value crosses the FFI boundary and returns unchanged."
)]
#[export]
pub fn echo_log_level(level: LogLevel) -> LogLevel {
    level
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.log_level.should_compare_against_minimum",
    description = "should_log compares u8-backed LogLevel values against a minimum level."
)]
#[export]
pub fn should_log(level: LogLevel, min_level: LogLevel) -> bool {
    (level as u8) >= (min_level as u8)
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.log_level.should_roundtrip_vectors",
    description = "A vector of repr(u8) LogLevel values preserves variant order and values."
)]
#[export]
pub fn echo_vec_log_level(levels: Vec<LogLevel>) -> Vec<LogLevel> {
    levels
}

/// HTTP status codes with gapped, real-world discriminants.
///
/// Every variant's numeric value is meaningful on its own — `404` is a
/// wire-level protocol constant, not a label that could be renumbered.
/// A round-trip of `NotFound` must preserve `404`, not some positional
/// index that happens to name the same variant.
///
/// This ensures that in languages which expose numbered enum members
/// (C#, Kotlin, Swift, Java, etc.), the generated enum carries the Rust
/// discriminant — `Ok = 200, NotFound = 404, ServerError = 500` — so the
/// numeric value is usable directly in consuming code (e.g. comparing
/// against an HTTP response status) without routing through a separate
/// lookup table.
#[demo_bench_macros::demo_case(
    "enums.repr_int.http_code.should_expose_discriminant_values",
    description = "HttpCode exposes the exact repr(u16) discriminants generated from Rust.",
    exclude(
        python,
        reason = "The Python enum demo tests do not currently cover HttpCode discriminants."
    )
)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum HttpCode {
    Ok = 200,
    NotFound = 404,
    ServerError = 500,
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.http_code.should_roundtrip_values",
    description = "A host-provided repr(u16) HttpCode value crosses the FFI boundary and returns unchanged.",
    exclude(
        python,
        reason = "The Python enum demo tests do not currently cover HttpCode round-trips."
    )
)]
#[export]
pub fn echo_http_code(code: HttpCode) -> HttpCode {
    code
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.http_code.should_return_not_found",
    description = "Rust can return the gapped HttpCode::NotFound discriminant to generated bindings.",
    exclude(
        python,
        reason = "The Python enum demo tests do not currently cover HttpCode constructors."
    )
)]
#[export]
pub fn http_code_not_found() -> HttpCode {
    HttpCode::NotFound
}

/// Signedness sentinel with a negative discriminant.
///
/// Rust allows negative values on any signed `#[repr(iN)]` enum, and
/// the numeric value must survive the crossing intact — flipping
/// `Negative` to `255` (an unsigned reinterpretation of the low byte)
/// changes the meaning of the value for every consumer.
///
/// This ensures that in languages which expose numbered enum members,
/// the backing type stays signed all the way through: the emitted C#
/// `enum : sbyte`, Swift `enum Sign: Int8`, Kotlin `value: Byte`,
/// Java `byte value`, etc. all preserve `-1` rather than truncating
/// it to its two's-complement unsigned form.
#[demo_bench_macros::demo_case(
    "enums.repr_int.sign.should_expose_signed_discriminant_values",
    description = "Sign exposes the exact signed repr(i8) discriminants generated from Rust.",
    exclude(
        python,
        reason = "The Python enum demo tests do not currently cover Sign discriminants."
    )
)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i8)]
pub enum Sign {
    Negative = -1,
    Zero = 0,
    Positive = 1,
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.sign.should_roundtrip_signed_values",
    description = "A host-provided repr(i8) Sign value crosses the FFI boundary with its signed value intact.",
    exclude(
        python,
        reason = "The Python enum demo tests do not currently cover Sign round-trips."
    )
)]
#[export]
pub fn echo_sign(s: Sign) -> Sign {
    s
}

#[demo_bench_macros::demo_case(
    "enums.repr_int.sign.should_return_negative",
    description = "Rust can return the negative Sign discriminant to generated bindings.",
    exclude(
        python,
        reason = "The Python enum demo tests do not currently cover Sign constructors."
    )
)]
#[export]
pub fn sign_negative() -> Sign {
    Sign::Negative
}
