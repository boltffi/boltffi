use boltffi::export;

#[export]
pub struct Gauge {
    level: u32,
}

#[export]
impl Gauge {
    pub fn raise(&mut self) {
        self.level += 1;
    }
}
