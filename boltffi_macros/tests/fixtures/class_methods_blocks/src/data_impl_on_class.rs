use boltffi::data;

use crate::Counter;

#[data(impl)]
impl Counter {
    pub fn peek(&self) -> i32 {
        self.get()
    }
}
