boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: i32,
}
#[boltffi::export]
pub fn f(a: std::sync::Arc<Point>) {}
fn main() {}
