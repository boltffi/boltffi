use boltffi::export;

use crate::Step;

#[export(methods)]
impl Step {
    pub fn doubled(&self) -> i32 {
        self.by * 2
    }
}
