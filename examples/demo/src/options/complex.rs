use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::enums::c_style::Status;
use crate::records::blittable::Point;
use crate::results::ApiResult;

#[demo_bench_macros::demo_case(
    "options.complex.string.should_roundtrip_some",
    description = "An Option<String> carrying Some crosses the wire as UTF-8 and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_roundtrip_none",
    description = "An Option<String> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn echo_optional_string(v: Option<String>) -> Option<String> {
    v
}

#[demo_bench_macros::demo_case(
    "options.complex.string.should_report_some",
    description = "is_some_string returns true when an Option<String> is Some.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_report_none",
    description = "is_some_string returns false when an Option<String> is None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn is_some_string(v: Option<String>) -> bool {
    v.is_some()
}

#[demo_bench_macros::demo_case(
    "options.complex.point.should_roundtrip_some",
    description = "An Option<Point> carrying Some crosses the wire and returns the same Point.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.point.should_roundtrip_none",
    description = "An Option<Point> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn echo_optional_point(v: Option<Point>) -> Option<Point> {
    v
}

/// Returns a Point if both coordinates are valid, None otherwise.
#[demo_bench_macros::demo_case(
    "options.complex.point.should_make_some",
    description = "make_some_point returns Some containing a Point built from coordinates.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn make_some_point(x: f64, y: f64) -> Option<Point> {
    Some(Point { x, y })
}

#[demo_bench_macros::demo_case(
    "options.complex.point.should_make_none",
    description = "make_none_point returns None for Option<Point>.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn make_none_point() -> Option<Point> {
    None
}

#[demo_bench_macros::demo_case(
    "options.complex.status.should_roundtrip_some",
    description = "An Option<Status> carrying Some crosses the wire and returns the same enum value.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.status.should_roundtrip_none",
    description = "An Option<Status> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn echo_optional_status(v: Option<Status>) -> Option<Status> {
    v
}

#[demo_bench_macros::demo_case(
    "options.complex.vec.should_roundtrip_some",
    description = "An Option<Vec<i32>> carrying Some crosses the wire and returns the same vector.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_roundtrip_none",
    description = "An Option<Vec<i32>> carrying None crosses the wire and returns None.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_roundtrip_empty_some",
    description = "An Option<Vec<i32>> carrying Some(empty vector) remains distinct from None.",
    exclude(
        apple,
        reason = "The Apple option demo does not currently cover Some(empty Vec) for echo_optional_vec."
    ),
    exclude(
        java,
        reason = "The Java option demo does not currently cover Some(empty Vec) for echo_optional_vec."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover Some(empty Vec) for echo_optional_vec."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover Some(empty Vec) for echo_optional_vec."
    )
)]
#[export]
pub fn echo_optional_vec(v: Option<Vec<i32>>) -> Option<Vec<i32>> {
    v
}

#[demo_bench_macros::demo_case(
    "options.complex.vec.should_report_length_for_some",
    description = "optional_vec_length returns Some(length) when an Option<Vec<i32>> contains a vector.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_return_none_for_absent_length",
    description = "optional_vec_length returns None when the vector option is absent.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[export]
pub fn optional_vec_length(v: Option<Vec<i32>>) -> Option<u32> {
    v.map(|vec| vec.len() as u32)
}

#[benchmark_candidate(function, uniffi, wasm_bindgen)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_find_name_for_positive_id",
    description = "find_name returns Some generated string when the id is positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_name."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_name."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_name."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.string.should_return_none_for_non_positive_id",
    description = "find_name returns None when the id is not positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_name."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_name."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_name."
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
    description = "find_numbers returns Some vector of i32 values when count is positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_numbers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_numbers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_numbers."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec.should_return_none_for_non_positive_number_count",
    description = "find_numbers returns None when count is not positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_numbers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_numbers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_numbers."
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
    description = "find_names returns Some vector of generated strings when count is positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_names."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_names."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_names."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_string.should_return_none_for_non_positive_name_count",
    description = "find_names returns None when count is not positive.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_names."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_names."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover find_names."
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
    description = "find_api_result returns Some(ApiResult::Success) for code 0.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_api_result."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_api_result."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_find_error_code_variant",
    description = "find_api_result returns Some(ApiResult::ErrorCode) for code 1.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_api_result."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_api_result."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_find_error_with_data_variant",
    description = "find_api_result returns Some(ApiResult::ErrorWithData) for code 2.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_api_result."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_api_result."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover the ErrorWithData find_api_result branch."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.api_result.should_return_none_for_unknown_code",
    description = "find_api_result returns None when the code does not map to an ApiResult variant.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover find_api_result."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin option demo does not currently cover find_api_result."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
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
    description = "A Vec<Option<i32>> carrying mixed Some and None elements crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_optional_i32.should_roundtrip_empty",
    description = "An empty Vec<Option<i32>> crosses the wire and returns empty.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    )
)]
#[demo_bench_macros::demo_case(
    "options.complex.vec_optional_i32.should_roundtrip_all_none",
    description = "A Vec<Option<i32>> carrying only None elements crosses the wire and preserves each absent slot.",
    exclude(
        java,
        reason = "The Java option demo does not currently cover all-None Vec<Option<i32>>."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Option surfaces."
    ),
    exclude(
        wasm,
        reason = "The wasm option demo does not currently cover all-None Vec<Option<i32>>."
    )
)]
#[export]
pub fn echo_vec_optional_i32(v: Vec<Option<i32>>) -> Vec<Option<i32>> {
    v
}
