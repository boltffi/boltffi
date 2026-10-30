boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
impl Counter {
    pub fn me(&self) -> &Self {
        self
    }
}
fn main() {}
