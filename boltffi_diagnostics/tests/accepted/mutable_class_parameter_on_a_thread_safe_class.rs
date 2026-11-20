boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
pub fn f(c: &mut Counter) {
    c.n += 1;
}
fn main() {}
