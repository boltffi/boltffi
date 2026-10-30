boltffi::scaffolding!();
const D: i32 = 1;
#[boltffi::export]
pub fn f(#[boltffi::default(D)] a: i32) -> i32 { a }
fn main() {}
