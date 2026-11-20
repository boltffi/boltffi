mod a {
    boltffi::scaffolding!();
}
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: i32,
}
#[boltffi::export]
pub fn f() -> Point {
    Point { x: 0 }
}
fn main() {}
