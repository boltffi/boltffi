boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: i32,
}
#[boltffi::data(impl)]
impl Default for Point {
    fn default() -> Self {
        Self { x: 0 }
    }
}
fn main() {}
