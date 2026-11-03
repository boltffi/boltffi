//! An export that names its types through aliases.
//!
//! A type alias reaches no lane, so a build expands this export only from a record store.
//! A gem install builds a copy of this crate in a new folder, whose build directory has no
//! store, so the copy has to carry one.

boltffi::scaffolding!();

use boltffi::{data, export};

#[data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

type Coordinate = Point;
type Route = Vec<Point>;

#[export]
pub fn first_point(start: Coordinate, route: Route) -> Coordinate {
    route.first().copied().unwrap_or(start)
}
