use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_roundtrip_some",
    justification = "Ensure an Option<i32> carrying Some crosses the wire and returns the same value.",
    directions = "Call `options::primitives::echo_optional_i32` through the generated binding and assert an Option<i32> carrying Some crosses the wire and returns the same value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_roundtrip_none",
    justification = "Ensure an Option<i32> carrying None crosses the wire and returns None.",
    directions = "Call `options::primitives::echo_optional_i32` through the generated binding and assert an Option<i32> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn echo_optional_i32(v: Option<i32>) -> Option<i32> {
    v
}

#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_roundtrip_some",
    justification = "Ensure an Option<f64> carrying Some crosses the wire and returns the same value.",
    directions = "Call `options::primitives::echo_optional_f64` through the generated binding and assert an Option<f64> carrying Some crosses the wire and returns the same value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_roundtrip_none",
    justification = "Ensure an Option<f64> carrying None crosses the wire and returns None.",
    directions = "Call `options::primitives::echo_optional_f64` through the generated binding and assert an Option<f64> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn echo_optional_f64(v: Option<f64>) -> Option<f64> {
    v
}

#[demo_bench_macros::demo_case(
    "options.primitives.bool.should_roundtrip_some",
    justification = "Ensure an Option<bool> carrying Some crosses the wire and returns the same value.",
    directions = "Call `options::primitives::echo_optional_bool` through the generated binding and assert an Option<bool> carrying Some crosses the wire and returns the same value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.bool.should_roundtrip_none",
    justification = "Ensure an Option<bool> carrying None crosses the wire and returns None.",
    directions = "Call `options::primitives::echo_optional_bool` through the generated binding and assert an Option<bool> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn echo_optional_bool(v: Option<bool>) -> Option<bool> {
    v
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_unwrap_some",
    justification = "Ensure unwrap_or_default_i32 returns the contained value when Option<i32> is Some.",
    directions = "Call `options::primitives::unwrap_or_default_i32` through the generated binding and assert unwrap_or_default_i32 returns the contained value when Option<i32> is Some.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_use_default_for_none",
    justification = "Ensure unwrap_or_default_i32 returns the fallback when Option<i32> is None.",
    directions = "Call `options::primitives::unwrap_or_default_i32` through the generated binding and assert unwrap_or_default_i32 returns the fallback when Option<i32> is None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn unwrap_or_default_i32(v: Option<i32>, fallback: i32) -> i32 {
    v.unwrap_or(fallback)
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_make_some",
    justification = "Ensure make_some_i32 returns Some containing the provided i32 value.",
    directions = "Call `options::primitives::make_some_i32` through the generated binding and assert make_some_i32 returns Some containing the provided i32 value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn make_some_i32(v: i32) -> Option<i32> {
    Some(v)
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_make_none",
    justification = "Ensure make_none_i32 returns None for Option<i32>.",
    directions = "Call `options::primitives::make_none_i32` through the generated binding and assert make_none_i32 returns None for Option<i32>.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn make_none_i32() -> Option<i32> {
    None
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_double_some",
    justification = "Ensure double_if_some doubles the contained i32 when the option is Some.",
    directions = "Call `options::primitives::double_if_some` through the generated binding and assert double_if_some doubles the contained i32 when the option is Some.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_preserve_none_when_doubling",
    justification = "Ensure double_if_some preserves None rather than producing a value.",
    directions = "Call `options::primitives::double_if_some` through the generated binding and assert double_if_some preserves None rather than producing a value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn double_if_some(v: Option<i32>) -> Option<i32> {
    v.map(|x| x * 2)
}

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_find_even_value",
    justification = "Ensure find_even returns Some containing the input when the i32 value is even.",
    directions = "Call `options::primitives::find_even` through the generated binding and assert find_even returns Some containing the input when the i32 value is even.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_return_none_for_odd_value",
    justification = "Ensure find_even returns None when the i32 value is odd.",
    directions = "Call `options::primitives::find_even` through the generated binding and assert find_even returns None when the i32 value is odd.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn find_even(value: i32) -> Option<i32> {
    if value % 2 == 0 { Some(value) } else { None }
}

#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "options.primitives.i64.should_find_positive_value",
    justification = "Ensure find_positive_i64 returns Some containing the input when the i64 value is positive.",
    directions = "Call `options::primitives::find_positive_i64` through the generated binding and assert find_positive_i64 returns Some containing the input when the i64 value is positive.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "TypeScript wrapper for Option<i64> uses takePackedBuffer expecting a packed BigInt, but the wasm export returns a NaN-boxed f64. Include this case when the Option<i64> lowerer is aligned with the i64 NaN-boxed return convention."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i64.should_return_none_for_non_positive_value",
    justification = "Ensure find_positive_i64 returns None when the i64 value is zero or negative.",
    directions = "Call `options::primitives::find_positive_i64` through the generated binding and assert find_positive_i64 returns None when the i64 value is zero or negative.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "TypeScript wrapper for Option<i64> uses takePackedBuffer expecting a packed BigInt, but the wasm export returns a NaN-boxed f64. Include this case when the Option<i64> lowerer is aligned with the i64 NaN-boxed return convention."
    )
)]
#[export]
pub fn find_positive_i64(value: i64) -> Option<i64> {
    if value > 0 { Some(value) } else { None }
}

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_find_positive_value",
    justification = "Ensure find_positive_f64 returns Some containing the input when the f64 value is positive.",
    directions = "Call `options::primitives::find_positive_f64` through the generated binding and assert find_positive_f64 returns Some containing the input when the f64 value is positive.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_return_none_for_non_positive_value",
    justification = "Ensure find_positive_f64 returns None when the f64 value is zero or negative.",
    directions = "Call `options::primitives::find_positive_f64` through the generated binding and assert find_positive_f64 returns None when the f64 value is zero or negative.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn find_positive_f64(value: f64) -> Option<f64> {
    if value > 0.0 { Some(value) } else { None }
}
