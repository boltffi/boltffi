use boltffi::*;

#[demo_bench_macros::demo_case(
    "results.nested_results.option.should_return_some_for_positive_key",
    justification = "Ensure result_of_option returns an Ok(Some) integer for a positive key.",
    directions = "Call `results::nested_results::result_of_option` through the generated binding and assert result_of_option returns an Ok(Some) integer for a positive key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.nested_results.option.should_return_none_for_zero_key",
    justification = "Ensure result_of_option returns Ok(None) for key zero.",
    directions = "Call `results::nested_results::result_of_option` through the generated binding and assert result_of_option returns Ok(None) for key zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.nested_results.option.should_reject_negative_key",
    justification = "Ensure result_of_option returns a language-native error for a negative key.",
    directions = "Call `results::nested_results::result_of_option` through the generated binding and assert result_of_option returns a language-native error for a negative key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[export]
pub fn result_of_option(key: i32) -> Result<Option<i32>, String> {
    if key < 0 {
        Err("invalid key".to_string())
    } else if key == 0 {
        Ok(None)
    } else {
        Ok(Some(key * 2))
    }
}

#[demo_bench_macros::demo_case(
    "results.nested_results.vec.should_return_values_for_non_negative_count",
    justification = "Ensure result_of_vec returns an Ok vector containing values from zero up to the count.",
    directions = "Call `results::nested_results::result_of_vec` through the generated binding and assert result_of_vec returns an Ok vector containing values from zero up to the count.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.nested_results.vec.should_reject_negative_count",
    justification = "Ensure result_of_vec returns a language-native error for a negative count.",
    directions = "Call `results::nested_results::result_of_vec` through the generated binding and assert result_of_vec returns a language-native error for a negative count.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[export]
pub fn result_of_vec(count: i32) -> Result<Vec<i32>, String> {
    if count < 0 {
        Err("negative count".to_string())
    } else {
        Ok((0..count).collect())
    }
}

#[demo_bench_macros::demo_case(
    "results.nested_results.string.should_return_value_for_non_negative_key",
    justification = "Ensure result_of_string returns an Ok string value for a non-negative key.",
    directions = "Call `results::nested_results::result_of_string` through the generated binding and assert result_of_string returns an Ok string value for a non-negative key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.nested_results.string.should_reject_negative_key",
    justification = "Ensure result_of_string returns a language-native error for a negative key.",
    directions = "Call `results::nested_results::result_of_string` through the generated binding and assert result_of_string returns a language-native error for a negative key.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit nested Result shapes. Include this case when nested Result support is implemented for Python."
    )
)]
#[export]
pub fn result_of_string(key: i32) -> Result<String, String> {
    if key < 0 {
        Err("invalid key".to_string())
    } else {
        Ok(format!("item_{}", key))
    }
}
