use boltffi::*;

use super::error_enums::MathError;

#[demo_bench_macros::demo_case(
    "results.async_results.safe_divide.should_return_quotient",
    description = "async_safe_divide resolves to the integer quotient when the divisor is non-zero.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.safe_divide.should_reject_division_by_zero",
    description = "async_safe_divide rejects with a typed MathError when the divisor is zero.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[export]
pub async fn async_safe_divide(a: i32, b: i32) -> Result<i32, MathError> {
    if b == 0 {
        Err(MathError::DivisionByZero)
    } else {
        Ok(a / b)
    }
}

#[demo_bench_macros::demo_case(
    "results.async_results.fallible_fetch.should_return_value_for_non_negative_key",
    description = "async_fallible_fetch resolves to a value string for a non-negative key.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.fallible_fetch.should_reject_negative_key",
    description = "async_fallible_fetch rejects with a string error for a negative key.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[export]
pub async fn async_fallible_fetch(key: i32) -> Result<String, String> {
    if key < 0 {
        Err("invalid key".to_string())
    } else {
        Ok(format!("value_{}", key))
    }
}

/// Looks up a value by key. Negative keys are invalid, key 0
/// means "not found" (returns Ok(None)), positive keys return
/// the value multiplied by 10.
#[demo_bench_macros::demo_case(
    "results.async_results.find_value.should_return_some_for_positive_key",
    description = "async_find_value resolves to Ok(Some) for a positive key.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.find_value.should_return_none_for_zero_key",
    description = "async_find_value resolves to Ok(None) for key zero.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.find_value.should_reject_negative_key",
    description = "async_find_value rejects with a string error for a negative key.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover async result exports."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async result exports."
    )
)]
#[export]
pub async fn async_find_value(key: i32) -> Result<Option<i32>, String> {
    if key < 0 {
        Err("invalid key".to_string())
    } else if key == 0 {
        Ok(None)
    } else {
        Ok(Some(key * 10))
    }
}
