use pim_macros::data;

#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[data]
pub struct Shape {
    pub origin: Point,
    pub points: Vec<Point>,
    pub id: u64,
}
