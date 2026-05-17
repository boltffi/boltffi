use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::enums::c_style::Status;
use crate::records::blittable::Point;
use crate::results::ApiResult;

#[demo_bench_macros::demo_case(
    "options.complex.string.should_roundtrip_some",
    justification = "Ensure an Option<String> carrying Some crosses the wire as UTF-8 and returns unchanged.",
    directions = "Call `options::complex::echo_optional_string` through the generated binding and assert an Option<String> carrying Some crosses the wire as UTF-8 and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_roundtrip_none",
    justification = "Ensure an Option<String> carrying None crosses the wire and returns None.",
    directions = "Call `options::complex::echo_optional_string` through the generated binding and assert an Option<String> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn echo_optional_string(v: Option<String>) -> Option<String> {
    v
}

#[demo_bench_macros::demo_case(
    "options.complex.string.should_report_some",
    justification = "Ensure is_some_string returns true when an Option<String> is Some.",
    directions = "Call `options::complex::is_some_string` through the generated binding and assert is_some_string returns true when an Option<String> is Some.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_report_none",
    justification = "Ensure is_some_string returns false when an Option<String> is None.",
    directions = "Call `options::complex::is_some_string` through the generated binding and assert is_some_string returns false when an Option<String> is None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn is_some_string(v: Option<String>) -> bool {
    v.is_some()
}

#[demo_bench_macros::demo_case(
    "options.complex.point.should_roundtrip_some",
    justification = "Ensure an Option<Point> carrying Some crosses the wire and returns the same Point.",
    directions = "Call `options::complex::echo_optional_point` through the generated binding and assert an Option<Point> carrying Some crosses the wire and returns the same Point.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.point.should_roundtrip_none",
    justification = "Ensure an Option<Point> carrying None crosses the wire and returns None.",
    directions = "Call `options::complex::echo_optional_point` through the generated binding and assert an Option<Point> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn echo_optional_point(v: Option<Point>) -> Option<Point> {
    v
}

/// Returns a Point if both coordinates are valid, None otherwise.
#[demo_bench_macros::demo_case(
    "options.complex.point.should_make_some",
    justification = "Ensure make_some_point returns Some containing a Point built from coordinates.",
    directions = "Call `options::complex::make_some_point` through the generated binding and assert make_some_point returns Some containing a Point built from coordinates.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn make_some_point(x: f64, y: f64) -> Option<Point> {
    Some(Point { x, y })
}

#[demo_bench_macros::demo_case(
    "options.complex.point.should_make_none",
    justification = "Ensure make_none_point returns None for Option<Point>.",
    directions = "Call `options::complex::make_none_point` through the generated binding and assert make_none_point returns None for Option<Point>.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn make_none_point() -> Option<Point> {
    None
}

#[demo_bench_macros::demo_case(
    "options.complex.status.should_roundtrip_some",
    justification = "Ensure an Option<Status> carrying Some crosses the wire and returns the same enum value.",
    directions = "Call `options::complex::echo_optional_status` through the generated binding and assert an Option<Status> carrying Some crosses the wire and returns the same enum value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.status.should_roundtrip_none",
    justification = "Ensure an Option<Status> carrying None crosses the wire and returns None.",
    directions = "Call `options::complex::echo_optional_status` through the generated binding and assert an Option<Status> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn echo_optional_status(v: Option<Status>) -> Option<Status> {
    v
}

#[demo_bench_macros::demo_case(
    "options.complex.vec.should_roundtrip_some",
    justification = "Ensure an Option<Vec<i32>> carrying Some crosses the wire and returns the same vector.",
    directions = "Call `options::complex::echo_optional_vec` through the generated binding and assert an Option<Vec<i32>> carrying Some crosses the wire and returns the same vector.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_roundtrip_none",
    justification = "Ensure an Option<Vec<i32>> carrying None crosses the wire and returns None.",
    directions = "Call `options::complex::echo_optional_vec` through the generated binding and assert an Option<Vec<i32>> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_roundtrip_empty_some",
    justification = "Ensure an Option<Vec<i32>> carrying Some(empty vector) remains distinct from None.",
    directions = "Call `options::complex::echo_optional_vec` through the generated binding and assert an Option<Vec<i32>> carrying Some(empty vector) remains distinct from None.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for Some(empty Vec) for echo_optional_vec in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for Some(empty Vec) for echo_optional_vec in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for Some(empty Vec) for echo_optional_vec in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
pub fn echo_optional_vec(v: Option<Vec<i32>>) -> Option<Vec<i32>> {
    v
}

#[demo_bench_macros::demo_case(
    "options.complex.vec.should_report_length_for_some",
    justification = "Ensure optional_vec_length returns Some(length) when an Option<Vec<i32>> contains a vector.",
    directions = "Call `options::complex::optional_vec_length` through the generated binding and assert optional_vec_length returns Some(length) when an Option<Vec<i32>> contains a vector.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_return_none_for_absent_length",
    justification = "Ensure optional_vec_length returns None when the vector option is absent.",
    directions = "Call `options::complex::optional_vec_length` through the generated binding and assert optional_vec_length returns None when the vector option is absent.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn optional_vec_length(v: Option<Vec<i32>>) -> Option<u32> {
    v.map(|vec| vec.len() as u32)
}

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_find_name_for_positive_id",
    justification = "Ensure find_name returns Some generated string when the id is positive.",
    directions = "Call `options::complex::find_name` through the generated binding and assert find_name returns Some generated string when the id is positive.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_name in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_name in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_return_none_for_non_positive_id",
    justification = "Ensure find_name returns None when the id is not positive.",
    directions = "Call `options::complex::find_name` through the generated binding and assert find_name returns None when the id is not positive.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_name in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_name in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn find_name(id: i32) -> Option<String> {
    if id > 0 {
        Some(format!("Name_{}", id))
    } else {
        None
    }
}

#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_find_numbers_for_positive_count",
    justification = "Ensure find_numbers returns Some vector of i32 values when count is positive.",
    directions = "Call `options::complex::find_numbers` through the generated binding and assert find_numbers returns Some vector of i32 values when count is positive.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_numbers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_numbers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_return_none_for_non_positive_number_count",
    justification = "Ensure find_numbers returns None when count is not positive.",
    directions = "Call `options::complex::find_numbers` through the generated binding and assert find_numbers returns None when count is not positive.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_numbers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_numbers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn find_numbers(count: i32) -> Option<Vec<i32>> {
    if count > 0 {
        Some((0..count).collect())
    } else {
        None
    }
}

#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_string.should_find_names_for_positive_count",
    justification = "Ensure find_names returns Some vector of generated strings when count is positive.",
    directions = "Call `options::complex::find_names` through the generated binding and assert find_names returns Some vector of generated strings when count is positive.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_names in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_names in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_string.should_return_none_for_non_positive_name_count",
    justification = "Ensure find_names returns None when count is not positive.",
    directions = "Call `options::complex::find_names` through the generated binding and assert find_names returns None when count is not positive.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_names in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_names in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn find_names(count: i32) -> Option<Vec<String>> {
    if count > 0 {
        Some((0..count).map(|index| format!("Name_{}", index)).collect())
    } else {
        None
    }
}

#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_find_success_variant",
    justification = "Ensure find_api_result returns Some(ApiResult::Success) for code 0.",
    directions = "Call `options::complex::find_api_result` through the generated binding and assert find_api_result returns Some(ApiResult::Success) for code 0.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_find_error_code_variant",
    justification = "Ensure find_api_result returns Some(ApiResult::ErrorCode) for code 1.",
    directions = "Call `options::complex::find_api_result` through the generated binding and assert find_api_result returns Some(ApiResult::ErrorCode) for code 1.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_find_error_with_data_variant",
    justification = "Ensure find_api_result returns Some(ApiResult::ErrorWithData) for code 2.",
    directions = "Call `options::complex::find_api_result` through the generated binding and assert find_api_result returns Some(ApiResult::ErrorWithData) for code 2.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for the ErrorWithData find_api_result branch in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_return_none_for_unknown_code",
    justification = "Ensure find_api_result returns None when the code does not map to an ApiResult variant.",
    directions = "Call `options::complex::find_api_result` through the generated binding and assert find_api_result returns None when the code does not map to an ApiResult variant.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin has no assertion for find_api_result in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[export]
pub fn find_api_result(code: i32) -> Option<ApiResult> {
    match code {
        0 => Some(ApiResult::Success),
        1 => Some(ApiResult::ErrorCode(-1)),
        2 => Some(ApiResult::ErrorWithData {
            code: -1,
            detail: -2,
        }),
        _ => None,
    }
}

/// Round-trips a vector of optional i32s. Exercises `Vec<Option<T>>` —
/// the encoded-array path where every element carries its own 1-byte
/// Option tag. Without this fixture each backend's Option support
/// would be provable at the function-signature level only, not in
/// composition with Vec.
#[demo_bench_macros::demo_case(
    "options.complex.vec_optional_i32.should_roundtrip_mixed_presence",
    justification = "Ensure a Vec<Option<i32>> carrying mixed Some and None elements crosses the wire and returns unchanged.",
    directions = "Call `options::complex::echo_vec_optional_i32` through the generated binding and assert a Vec<Option<i32>> carrying mixed Some and None elements crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_optional_i32.should_roundtrip_empty",
    justification = "Ensure an empty Vec<Option<i32>> crosses the wire and returns empty.",
    directions = "Call `options::complex::echo_vec_optional_i32` through the generated binding and assert an empty Vec<Option<i32>> crosses the wire and returns empty.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_optional_i32.should_roundtrip_all_none",
    justification = "Ensure a Vec<Option<i32>> carrying only None elements crosses the wire and preserves each absent slot.",
    directions = "Call `options::complex::echo_vec_optional_i32` through the generated binding and assert a Vec<Option<i32>> carrying only None elements crosses the wire and preserves each absent slot.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java has no assertion for all-None Vec<Option<i32>> in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently handle Option<T>. Include this case when optional values are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for all-None Vec<Option<i32>> in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
pub fn echo_vec_optional_i32(v: Vec<Option<i32>>) -> Vec<Option<i32>> {
    v
}
