use boltffi::*;

use super::error_enums::MathError;

#[demo_bench_macros::demo_case(
    "results.async_results.safe_divide.should_return_quotient",
    justification = "Ensure async_safe_divide resolves to the integer quotient when the divisor is non-zero.",
    directions = "Call `results::async_results::async_safe_divide` through the generated binding and assert async_safe_divide resolves to the integer quotient when the divisor is non-zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.safe_divide.should_reject_division_by_zero",
    justification = "Ensure async_safe_divide rejects with a typed MathError when the divisor is zero.",
    directions = "Call `results::async_results::async_safe_divide` through the generated binding and assert async_safe_divide rejects with a typed MathError when the divisor is zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
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
    justification = "Ensure async_fallible_fetch resolves to a value string for a non-negative key.",
    directions = "Call `results::async_results::async_fallible_fetch` through the generated binding and assert async_fallible_fetch resolves to a value string for a non-negative key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.fallible_fetch.should_reject_negative_key",
    justification = "Ensure async_fallible_fetch rejects with a string error for a negative key.",
    directions = "Call `results::async_results::async_fallible_fetch` through the generated binding and assert async_fallible_fetch rejects with a string error for a negative key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
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
    justification = "Ensure async_find_value resolves to Ok(Some) for a positive key.",
    directions = "Call `results::async_results::async_find_value` through the generated binding and assert async_find_value resolves to Ok(Some) for a positive key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.find_value.should_return_none_for_zero_key",
    justification = "Ensure async_find_value resolves to Ok(None) for key zero.",
    directions = "Call `results::async_results::async_find_value` through the generated binding and assert async_find_value resolves to Ok(None) for key zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.async_results.find_value.should_reject_negative_key",
    justification = "Ensure async_find_value rejects with a string error for a negative key.",
    directions = "Call `results::async_results::async_find_value` through the generated binding and assert async_find_value rejects with a string error for a negative key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions and Result returns. Include this case when async Result support is implemented for Python."
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
