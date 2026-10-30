boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
impl Counter {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self { n: 0 })
    }
}
fn main() {}
