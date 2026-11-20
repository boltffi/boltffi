boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter { n: u32 }
#[boltffi::export]
impl Counter { #[ffi_stream]
 pub fn s(&self) -> u32 { 0 } }
fn main() {}
