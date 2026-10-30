boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
impl Counter {
    pub fn a(self: std::sync::Arc<Self>) -> u32 {
        0
    }
}
fn main() {}
