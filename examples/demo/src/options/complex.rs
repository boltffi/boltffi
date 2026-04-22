use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::enums::c_style::Status;
use crate::records::blittable::Point;
use crate::results::ApiResult;

#[export]
pub fn echo_optional_string(v: Option<String>) -> Option<String> {
    v
}

#[export]
pub fn is_some_string(v: Option<String>) -> bool {
    v.is_some()
}

#[export]
pub fn echo_optional_point(v: Option<Point>) -> Option<Point> {
    v
}

/// Returns a Point if both coordinates are valid, None otherwise.
#[export]
pub fn make_some_point(x: f64, y: f64) -> Option<Point> {
    Some(Point { x, y })
}

#[export]
pub fn make_none_point() -> Option<Point> {
    None
}

#[export]
pub fn echo_optional_status(v: Option<Status>) -> Option<Status> {
    v
}

#[export]
pub fn echo_optional_vec(v: Option<Vec<i32>>) -> Option<Vec<i32>> {
    v
}

#[export]
pub fn optional_vec_length(v: Option<Vec<i32>>) -> Option<u32> {
    v.map(|vec| vec.len() as u32)
}

#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn find_name(id: i32) -> Option<String> {
    if id > 0 {
        Some(format!("Name_{}", id))
    } else {
        None
    }
}

#[export]
#[benchmark_candidate(function, uniffi)]
pub fn find_numbers(count: i32) -> Option<Vec<i32>> {
    if count > 0 {
        Some((0..count).collect())
    } else {
        None
    }
}

#[export]
#[benchmark_candidate(function, uniffi)]
pub fn find_names(count: i32) -> Option<Vec<String>> {
    if count > 0 {
        Some((0..count).map(|index| format!("Name_{}", index)).collect())
    } else {
        None
    }
}

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
#[export]
pub fn echo_vec_optional_i32(v: Vec<Option<i32>>) -> Vec<Option<i32>> {
    v
}
