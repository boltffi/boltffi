#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[boltffi::export]
pub fn distance(from: Point, to: Point) -> f64 {
    (to.x - from.x).hypot(to.y - from.y)
}

fn main() {}
