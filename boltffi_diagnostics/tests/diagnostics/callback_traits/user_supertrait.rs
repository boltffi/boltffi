boltffi::scaffolding!();
pub trait B {}
#[boltffi::export]
pub trait T: B { fn f(&self); }
fn main() {}
