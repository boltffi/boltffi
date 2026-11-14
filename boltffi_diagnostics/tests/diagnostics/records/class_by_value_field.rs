boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter { n: u32 }
#[boltffi::data]
pub struct P { pub c: Counter }
fn main() {}
