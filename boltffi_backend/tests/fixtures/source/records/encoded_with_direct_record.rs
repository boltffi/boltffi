#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub active: bool,
}

#[data]
pub struct Segment {
    pub label: String,
    pub start: Point,
    pub end: Point,
}

#[export]
pub fn echo_segment(segment: Segment) -> Segment {
    segment
}
