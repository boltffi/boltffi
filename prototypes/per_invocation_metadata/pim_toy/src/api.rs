//! Exported functions: every wrapper is emitted adjacent to its item, so types resolve
//! under exactly the tokens the signature wrote.

use pim_macros::{data, export};

use crate::geometry::Point as GeoPoint;
use crate::geometry::Shape;

#[data]
#[repr(C)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[data]
pub struct MathError {
    pub code: u32,
    pub message: String,
}

#[export]
pub fn add_vec2(a: Vec2, b: Vec2) -> Vec2 {
    Vec2 {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}

#[export]
pub fn describe_shape(shape: Shape) -> String {
    format!(
        "Shape #{} at ({}, {}) with {} points",
        shape.id,
        shape.origin.x,
        shape.origin.y,
        shape.points.len()
    )
}

#[export]
pub fn nudge(point: GeoPoint, by: f64) -> GeoPoint {
    GeoPoint {
        x: point.x + by,
        y: point.y + by,
    }
}

#[export]
pub fn from_dep(point: pim_dep::DepPoint) -> f64 {
    point.x
}

#[export]
pub fn checked_div(dividend: f64, divisor: f64) -> Result<f64, MathError> {
    if divisor == 0.0 {
        return Err(MathError {
            code: 1,
            message: "division by zero".to_owned(),
        });
    }
    Ok(dividend / divisor)
}

macro_rules! make_scaler {
    ($name:ident, $factor:literal) => {
        #[export]
        pub fn $name(value: f64) -> f64 {
            value * $factor
        }
    };
}

make_scaler!(double_it, 2.0);
make_scaler!(triple_it, 3.0);

include!(concat!(env!("OUT_DIR"), "/generated_api.rs"));
