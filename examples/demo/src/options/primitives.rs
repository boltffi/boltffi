use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_roundtrip_some",
    description = "An Option<i32> carrying Some crosses the wire and returns the same value.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_roundtrip_none",
    description = "An Option<i32> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn echo_optional_i32(v: Option<i32>) -> Option<i32> {
    v
}

#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_roundtrip_some",
    description = "An Option<f64> carrying Some crosses the wire and returns the same value.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover Option<f64>."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_roundtrip_none",
    description = "An Option<f64> carrying None crosses the wire and returns None.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover Option<f64>."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn echo_optional_f64(v: Option<f64>) -> Option<f64> {
    v
}

#[demo_bench_macros::demo_case(
    "options.primitives.bool.should_roundtrip_some",
    description = "An Option<bool> carrying Some crosses the wire and returns the same value.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover Option<bool>."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.bool.should_roundtrip_none",
    description = "An Option<bool> carrying None crosses the wire and returns None.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover Option<bool>."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn echo_optional_bool(v: Option<bool>) -> Option<bool> {
    v
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_unwrap_some",
    description = "unwrap_or_default_i32 returns the contained value when Option<i32> is Some.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_use_default_for_none",
    description = "unwrap_or_default_i32 returns the fallback when Option<i32> is None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn unwrap_or_default_i32(v: Option<i32>, fallback: i32) -> i32 {
    v.unwrap_or(fallback)
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_make_some",
    description = "make_some_i32 returns Some containing the provided i32 value.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn make_some_i32(v: i32) -> Option<i32> {
    Some(v)
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_make_none",
    description = "make_none_i32 returns None for Option<i32>.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn make_none_i32() -> Option<i32> {
    None
}

#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_double_some",
    description = "double_if_some doubles the contained i32 when the option is Some.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_preserve_none_when_doubling",
    description = "double_if_some preserves None rather than producing a value.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn double_if_some(v: Option<i32>) -> Option<i32> {
    v.map(|x| x * 2)
}

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_find_even_value",
    description = "find_even returns Some containing the input when the i32 value is even.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_even."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_even."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_even."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i32.should_return_none_for_odd_value",
    description = "find_even returns None when the i32 value is odd.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_even."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_even."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_even."
    )
)]
#[export]
pub fn find_even(value: i32) -> Option<i32> {
    if value % 2 == 0 { Some(value) } else { None }
}

#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "options.primitives.i64.should_find_positive_value",
    description = "find_positive_i64 returns Some containing the input when the i64 value is positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_positive_i64."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_positive_i64."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_positive_i64."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.i64.should_return_none_for_non_positive_value",
    description = "find_positive_i64 returns None when the i64 value is zero or negative.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_positive_i64."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_positive_i64."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_positive_i64."
    )
)]
#[export]
pub fn find_positive_i64(value: i64) -> Option<i64> {
    if value > 0 { Some(value) } else { None }
}

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_find_positive_value",
    description = "find_positive_f64 returns Some containing the input when the f64 value is positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_positive_f64."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_positive_f64."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_positive_f64."
    )
)]
#[demo_bench_macros::demo_case(
    "options.primitives.f64.should_return_none_for_non_positive_value",
    description = "find_positive_f64 returns None when the f64 value is zero or negative.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_positive_f64."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_positive_f64."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_positive_f64."
    )
)]
#[export]
pub fn find_positive_f64(value: f64) -> Option<f64> {
    if value > 0.0 { Some(value) } else { None }
}
