boltffi::scaffolding!();
#[boltffi::export]
pub fn f() -> Result<i32, Box<dyn std::error::Error>> { Ok(1) }
fn main() {}
