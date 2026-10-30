boltffi::scaffolding!();

#[boltffi::export]
pub struct Counter {
    count: u32,
}

#[boltffi::export]
impl Counter {
    pub fn new() -> Self {
        Self { count: 0 }
    }
}

#[boltffi::data(impl)]
impl Counter {
    pub fn count(&self) -> u32 {
        self.count
    }
}

fn main() {}
