boltffi::scaffolding!();

pub struct Counter {
    count: u32,
}

#[boltffi::export]
impl Counter {
    pub fn new() -> Self {
        Self { count: 0 }
    }
}

fn main() {}
