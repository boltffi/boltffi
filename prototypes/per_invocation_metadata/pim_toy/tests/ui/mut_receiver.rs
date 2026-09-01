pim_macros::scaffolding!();

pub struct Counter {
    value: u64,
}

#[pim_macros::export]
impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn bump(&mut self) -> u64 {
        self.value += 1;
        self.value
    }
}

fn main() {}
