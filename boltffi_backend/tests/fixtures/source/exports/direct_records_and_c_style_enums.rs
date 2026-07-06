#[repr(C)]
#[data]
/// A two-dimensional point.
pub struct Point {
    /// The horizontal coordinate.
    pub x: i32,
    /// The vertical coordinate.
    pub y: i32,
}

#[repr(u8)]
#[data]
/// How fast the operation should run.
pub enum Mode {
    /// Prioritize speed.
    Fast = 1,
    /// Prioritize patience.
    Slow = 2,
}

#[export]
/// Returns the point unchanged.
pub fn echo_point(point: Point) -> Point {
    point
}

#[export]
/// Returns the mode unchanged.
pub fn echo_mode(mode: Mode) -> Mode {
    mode
}
