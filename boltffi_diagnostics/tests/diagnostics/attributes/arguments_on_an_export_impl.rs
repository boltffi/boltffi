boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter { n: u32 }
#[boltffi::export(single_threaded)]
impl Counter { pub fn new() -> Self { Self { n: 0 } } }
fn main() {}
