boltffi::scaffolding!();

pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[boltffi::export]
pub fn origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

fn main() {}
