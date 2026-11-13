//! Exports that name their types through aliases.
//!
//! A type alias reaches no lane, so a build expands these exports only from the record
//! store `boltffi generate` keeps. A fresh checkout has none, so `boltffi build` has to
//! prepare it before it compiles.

boltffi::scaffolding!();

use boltffi::{data, export};

#[data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

type Coordinate = Point;
type Meters = f64;
type Route = Vec<Point>;

#[data]
#[derive(Clone)]
pub struct Leg {
    pub length: Meters,
    pub route: Route,
}

#[export]
pub fn walk(start: Coordinate, route: Route) -> Leg {
    let length = route.iter().map(|point| point.x - start.x).sum();
    Leg { length, route }
}
