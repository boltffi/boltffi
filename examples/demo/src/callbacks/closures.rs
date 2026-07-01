use boltffi::*;

use crate::enums::c_style::Status;
use crate::records::blittable::Point;
use crate::results::error_enums::MathError;

/// Applies a closure to a single i32 value and returns the result.
#[export]
pub fn apply_closure(f: impl Fn(i32) -> i32, value: i32) -> i32 {
    f(value)
}

#[export]
pub fn apply_binary_closure(f: impl Fn(i32, i32) -> i32, a: i32, b: i32) -> i32 {
    f(a, b)
}

#[export]
pub fn apply_void_closure(f: impl Fn(i32), value: i32) {
    f(value)
}

#[export]
pub fn apply_nullary_closure(f: impl Fn() -> i32) -> i32 {
    f()
}

#[export]
pub fn apply_point_closure(f: impl Fn(Point) -> Point, p: Point) -> Point {
    f(p)
}

#[export]
pub fn apply_string_closure(f: impl Fn(String) -> String, s: String) -> String {
    f(s)
}

#[export]
pub fn apply_bool_closure(f: impl Fn(bool) -> bool, v: bool) -> bool {
    f(v)
}

#[export]
pub fn apply_f64_closure(f: impl Fn(f64) -> f64, v: f64) -> f64 {
    f(v)
}

#[export]
pub fn map_vec_with_closure(f: impl Fn(i32) -> i32, values: Vec<i32>) -> Vec<i32> {
    values.into_iter().map(|v| f(v)).collect()
}

#[export]
pub fn filter_vec_with_closure(f: impl Fn(i32) -> bool, values: Vec<i32>) -> Vec<i32> {
    values.into_iter().filter(|&v| f(v)).collect()
}

#[export]
pub fn apply_offset_closure(
    f: impl Fn(isize, usize) -> isize,
    value: isize,
    delta: usize,
) -> isize {
    f(value, delta)
}

#[export]
pub fn apply_status_closure(f: impl Fn(Status) -> Status, status: Status) -> Status {
    f(status)
}

#[export]
pub fn apply_optional_point_closure(
    f: impl Fn(Option<Point>) -> Option<Point>,
    point: Option<Point>,
) -> Option<Point> {
    f(point)
}

#[export]
pub fn apply_result_closure(
    f: impl Fn(i32) -> Result<i32, MathError>,
    value: i32,
) -> Result<i32, MathError> {
    f(value)
}
