boltffi::scaffolding!();
#[boltffi::export]
pub trait T { fn f(&self) -> &str; }
fn main() {}
