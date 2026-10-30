boltffi::scaffolding!();
#[boltffi::export]
pub trait T { fn f<A>(&self, a: A); }
fn main() {}
