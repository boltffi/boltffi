#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Scale {
    pub factor: u32,
}

#[boltffi::data(impl)]
impl Scale {
    pub const UNIT: u32 = 1;

    pub fn doubled(&self) -> u32 {
        self.factor * 2
    }
}

pub struct Balance {
    total: u32,
}

#[boltffi::export]
impl Balance {
    pub const LIMIT: u32 = 9;

    pub fn new() -> Self {
        Self { total: 0 }
    }

    pub fn weigh(&self, scale: Scale) -> u32 {
        self.total + scale.factor
    }
}
