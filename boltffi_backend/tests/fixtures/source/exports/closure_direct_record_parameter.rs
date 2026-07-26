#[repr(C)]
#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn apply_point(callback: impl Fn(Point) -> Point, value: Point) -> Point {
    callback(value)
}
