use std::cell::Cell;

use boltffi::export;

#[export]
pub struct Tally {
    count: Cell<u32>,
}

#[export]
impl Tally {
    pub fn count(&self) -> u32 {
        self.count.get()
    }
}
