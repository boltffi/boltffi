pim_macros::scaffolding!();

#[pim_macros::data]
pub struct Point {
    pub x: f64,
}

pub type Points = Vec<Point>;

#[pim_macros::data]
pub struct Route {
    pub points: Points,
}

fn main() {}
