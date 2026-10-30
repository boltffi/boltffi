boltffi::scaffolding!();
#[boltffi::data]
pub struct P { pub c: Box<dyn Fn(i32)> }
fn main() {}
