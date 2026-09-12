use pim_macros::data;

#[data]
pub struct Point {
    pub magnitude: f64,
}

#[data]
pub struct Body {
    pub position: Point,
}
