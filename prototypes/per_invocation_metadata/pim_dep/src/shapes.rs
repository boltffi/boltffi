use pim_macros::data;

#[data]
pub struct DepPoint {
    pub x: f64,
}

#[data]
pub struct DepLine {
    pub from: DepPoint,
    pub to: DepPoint,
}
