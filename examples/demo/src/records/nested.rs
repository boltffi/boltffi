use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::records::blittable::Point;

#[data]
#[benchmark_candidate(record, uniffi)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Line {
    pub start: Point,
    pub end: Point,
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.nested.line.should_roundtrip_nested_points",
    description = "A Line record containing two Point records crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover nested records."
    )
)]
pub fn echo_line(l: Line) -> Line {
    l
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.nested.line.should_make_from_coordinates",
    description = "make_line builds a Line with nested Point fields from four coordinate values.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover nested records."
    )
)]
pub fn make_line(x1: f64, y1: f64, x2: f64, y2: f64) -> Line {
    Line {
        start: Point { x: x1, y: y1 },
        end: Point { x: x2, y: y2 },
    }
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.nested.line.should_compute_length",
    description = "line_length receives a Line record and returns the distance between its nested Points.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover nested records."
    )
)]
pub fn line_length(l: Line) -> f64 {
    let dx = l.end.x - l.start.x;
    let dy = l.end.y - l.start.y;
    (dx * dx + dy * dy).sqrt()
}

#[data]
#[benchmark_candidate(record, uniffi)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Dimensions {
    pub width: f64,
    pub height: f64,
}

#[data]
#[benchmark_candidate(record, uniffi)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Rect {
    pub origin: Point,
    pub dimensions: Dimensions,
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.nested.rect.should_roundtrip_nested_records",
    description = "A Rect record containing Point and Dimensions records crosses the wire and returns unchanged.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Rect records."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover nested records."
    )
)]
pub fn echo_rect(r: Rect) -> Rect {
    r
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.nested.rect.should_compute_area",
    description = "rect_area receives a Rect record and multiplies its nested width and height fields.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Rect records."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover nested records."
    )
)]
pub fn rect_area(r: Rect) -> f64 {
    r.dimensions.width * r.dimensions.height
}
