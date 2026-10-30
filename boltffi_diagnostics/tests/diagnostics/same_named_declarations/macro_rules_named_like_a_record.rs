boltffi::scaffolding!();

#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[macro_export]
macro_rules! Point {
    ($x:expr, $y:expr) => {
        $crate::Point { x: $x, y: $y }
    };
}

fn main() {}
