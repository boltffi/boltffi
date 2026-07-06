/// A point on a two-dimensional plane.
#[repr(C)]
#[data]
pub struct Point {
    /// The horizontal coordinate.
    pub x: f64,
    /// The vertical coordinate.
    pub y: f64,
}

/// A mode selected by the caller.
#[repr(u8)]
#[data]
pub enum Mode {
    /// Favor fast work.
    Fast = 1,
    /// Favor careful work.
    Slow = 2,
}

#[data(impl)]
impl Point {
    /// Returns the origin point.
    pub fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Returns the horizontal distance from another point.
    pub fn distance(&self, other: Point) -> f64 {
        other.x - self.x
    }
}

#[data(impl)]
impl Mode {
    /// Returns the default mode.
    pub fn default() -> Self {
        Self::Fast
    }

    /// Returns the wire code for this mode.
    pub fn code(&self) -> u8 {
        0
    }
}
