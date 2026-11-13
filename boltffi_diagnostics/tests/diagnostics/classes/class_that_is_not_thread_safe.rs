boltffi::scaffolding!();

use std::cell::Cell;

#[boltffi::export]
pub struct Counter {
    count: Cell<u32>,
}

#[boltffi::export]
impl Counter {
    pub fn new() -> Self {
        Self {
            count: Cell::new(0),
        }
    }

    pub fn count(&self) -> u32 {
        self.count.get()
    }
}

fn main() {}
