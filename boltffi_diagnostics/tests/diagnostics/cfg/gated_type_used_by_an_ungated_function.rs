boltffi::scaffolding!();
#[cfg(any())]
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct P { pub x: i32 }
#[boltffi::export]
pub fn f(a: P) {}
fn main() {}
