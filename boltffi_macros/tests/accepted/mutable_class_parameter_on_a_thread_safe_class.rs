boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
pub fn f(c: &mut Counter) {}
fn main() {}
