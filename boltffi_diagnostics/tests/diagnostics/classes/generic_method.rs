boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
impl Counter {
    pub fn g<T>(&self, t: T) {}
}
fn main() {}
