boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point { pub x: i32 }
#[boltffi::data]
impl Point { pub fn x(&self) -> i32 { self.x } }
fn main() {}
