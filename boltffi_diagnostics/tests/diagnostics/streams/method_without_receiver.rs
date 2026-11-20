boltffi::scaffolding!();

use std::sync::Arc;

use boltffi::EventSubscription;

#[boltffi::export]
pub struct Counter {
    n: u32,
}

#[boltffi::export]
impl Counter {
    #[ffi_stream(item = i32)]
    pub fn events() -> Arc<EventSubscription<i32>> {
        unimplemented!()
    }
}

fn main() {}
