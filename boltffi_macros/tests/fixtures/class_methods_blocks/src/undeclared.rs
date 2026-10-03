use boltffi::export;

pub struct Orphan;

#[export(methods)]
impl Orphan {
    pub fn value(&self) -> u32 {
        0
    }
}
