use boltffi::export;

pub struct Orphan;

#[export]
impl Orphan {
    pub fn value(&self) -> u32 {
        0
    }
}
