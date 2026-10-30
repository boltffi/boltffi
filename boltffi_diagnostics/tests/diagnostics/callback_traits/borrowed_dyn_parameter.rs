boltffi::scaffolding!();
#[boltffi::export]
pub trait T { fn f(&self); }
#[boltffi::export]
pub fn g(t: &dyn T) { t.f() }
fn main() {}
