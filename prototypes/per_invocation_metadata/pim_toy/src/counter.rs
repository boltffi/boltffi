//! A class: constructed behind a `u64` handle, driven through `&self` methods.

use std::sync::atomic::{AtomicU64, Ordering};

use pim_macros::export;

pub struct Counter {
    label: String,
    value: AtomicU64,
}

#[export]
impl Counter {
    pub fn new(label: String, start: u64) -> Self {
        Self {
            label,
            value: AtomicU64::new(start),
        }
    }

    pub fn add(&self, amount: u64) -> u64 {
        self.value.fetch_add(amount, Ordering::Relaxed) + amount
    }

    pub fn label(&self) -> String {
        self.label.clone()
    }
}
