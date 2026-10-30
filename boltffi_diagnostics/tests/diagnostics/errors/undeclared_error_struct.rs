boltffi::scaffolding!();
pub struct E;
#[boltffi::export]
pub fn f() -> Result<i32, E> { Ok(1) }
fn main() {}
