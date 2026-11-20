boltffi::scaffolding!();

#[boltffi::export]
pub struct Counter {
    count: u32,
}

#[boltffi::export]
impl Default for Counter {
    fn default() -> Self {
        Self { count: 0 }
    }
}

fn main() {}
