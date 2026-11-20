use boltffi::*;

use crate::FixturePoint;

#[data]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FixtureRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl FixtureRect {
    pub fn origin() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }
}

/// Ordered by its derived `Ord`: tier, then score.
#[data(ord)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FixtureRank {
    pub tier: u8,
    pub score: i32,
}

#[export]
pub fn rect_area(rect: FixtureRect) -> f64 {
    rect.width * rect.height
}

#[export]
pub fn rect_x(rect: &FixtureRect) -> f64 {
    rect.x
}

#[export]
pub fn scale_rect_in_place(rect: &mut FixtureRect, factor: f64) {
    rect.width *= factor;
    rect.height *= factor;
}

#[export]
pub fn make_rect(x: f64, y: f64, width: f64, height: f64) -> FixtureRect {
    FixtureRect {
        x,
        y,
        width,
        height,
    }
}

#[data(impl)]
impl FixturePoint {
    pub fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    pub fn new_at(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to_origin(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn scale(&mut self, factor: f64) {
        self.x *= factor;
        self.y *= factor;
    }

    pub fn midpoint_to(a: FixturePoint, b: FixturePoint) -> FixturePoint {
        FixturePoint {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        }
    }
}

/// Deliberately one character.
///
/// `#[data]` locates its own declaration by byte offset, and an offset that is
/// off by one still lands inside any longer name. Every other declaration in
/// this crate is long enough to hide that, so this one is the tripwire: it
/// fails to compile if the offset leaves the identifier.
#[data]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct R {
    pub value: u32,
}
