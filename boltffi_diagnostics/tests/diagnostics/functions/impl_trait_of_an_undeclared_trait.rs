boltffi::scaffolding!();
pub trait D {}
#[boltffi::export]
pub fn f(d: impl D) {}
fn main() {}
