use boltffi::*;

use crate::records::blittable::Point;

pub struct MathUtils {
    precision: u32,
}

#[export]
impl MathUtils {
    pub const DEFAULT_PRECISION: u32 = 2;

    pub fn new(precision: u32) -> Self {
        Self { precision }
    }

    pub fn round(&self, value: f64) -> f64 {
        let factor = 10_f64.powi(self.precision as i32);
        (value * factor).round() / factor
    }

    pub fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
        value.max(min).min(max)
    }

    pub fn distance_between(a: Point, b: Point) -> f64 {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn midpoint(a: Point, b: Point) -> Point {
        Point {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        }
    }

    pub fn parse_int(input: String) -> Result<i32, String> {
        input.parse::<i32>().map_err(|err| err.to_string())
    }

    pub fn safe_sqrt(value: f64) -> Option<f64> {
        if value >= 0.0 {
            Some(value.sqrt())
        } else {
            None
        }
    }
}
