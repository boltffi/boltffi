boltffi::scaffolding!();
#[boltffi::export]
pub trait T { fn f(&self, g: Option<Box<dyn Fn(i32)>>); }
fn main() {}
