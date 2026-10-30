boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter { n: u32 }
#[boltffi::data]
#[derive(Clone)]
pub struct P { pub c: std::sync::Arc<Counter> }
fn main() {}
