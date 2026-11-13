boltffi::scaffolding!();
#[boltffi::export]
pub fn f() -> std::io::Result<i32> { Ok(1) }
fn main() {}
