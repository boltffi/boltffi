boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter {
    n: u32,
}
#[boltffi::export]
impl Counter {
    pub fn v(&self) -> Vec<Self> {
        vec![]
    }
}
fn main() {}
