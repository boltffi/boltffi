boltffi::scaffolding!();
#[boltffi::export]
pub struct Counter { n: u32 }
#[boltffi::export]
impl Counter { #[ffi_stream(item = i32, mode = "nope")]
 pub fn s(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() } }
fn main() {}
