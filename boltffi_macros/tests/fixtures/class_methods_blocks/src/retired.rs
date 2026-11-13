#![deny(deprecated)]

use boltffi::{data, export};

#[export]
#[deprecated(note = "use Counter")]
pub struct Tally {
    count: std::sync::atomic::AtomicU32,
}

#[export]
#[allow(deprecated)]
impl Tally {
    pub fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

#[export]
#[allow(deprecated)]
impl Tally {
    pub fn count(&self) -> u32 {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[data]
#[deprecated(note = "use Step")]
#[derive(Clone, Copy)]
pub struct Stride {
    pub by: i32,
}
