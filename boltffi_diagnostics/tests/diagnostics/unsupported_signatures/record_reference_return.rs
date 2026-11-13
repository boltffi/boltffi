boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: i32,
}
static P: Point = Point { x: 0 };
#[boltffi::export]
pub fn f() -> &'static Point {
    &P
}
fn main() {}
