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

    pub fn increment(&mut self) {
        self.count += 1;
    }
}

fn main() {}
