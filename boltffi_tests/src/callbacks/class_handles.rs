use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use boltffi::export;

pub struct CallbackOnlyHandle {
    drops: Arc<AtomicUsize>,
}

impl CallbackOnlyHandle {
    pub fn with_drops(drops: Arc<AtomicUsize>) -> Self {
        Self { drops }
    }
}

#[export]
impl CallbackOnlyHandle {
    pub fn value(&self) -> u32 {
        42
    }
}

impl Drop for CallbackOnlyHandle {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[export]
pub trait ClassHandleReceiver {
    fn attach(&self, r#handle: CallbackOnlyHandle, callback: u32);
}
