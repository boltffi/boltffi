use boltffi::export;

use crate::Step;

#[export]
impl Step {
    pub fn doubled(&self) -> i32 {
        self.by * 2
    }
}
