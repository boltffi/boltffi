boltffi::scaffolding!();

#[boltffi::export]
pub struct Url {
    address: String,
}

#[boltffi::export]
impl Url {
    pub fn new(address: String) -> Self {
        Self { address }
    }

    pub fn address(&self) -> String {
        self.address.clone()
    }
}

fn main() {}
