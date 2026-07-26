#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn point_sum(point: &Point) -> f64 {
    point.x + point.y
}
