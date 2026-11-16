boltffi::scaffolding!();
#[boltffi::export(single_thread)]
pub struct Counter { n: u32 }
#[boltffi::export]
impl Counter { pub fn inc(&mut self) { self.n += 1; } }
fn main() {}
