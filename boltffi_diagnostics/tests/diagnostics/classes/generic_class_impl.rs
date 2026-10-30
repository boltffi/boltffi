boltffi::scaffolding!();

pub struct Holder<T> {
    value: T,
}

#[boltffi::export]
impl<T> Holder<T> {
    pub fn replace(&self, value: T) -> T {
        let _ = &self.value;
        value
    }
}

fn main() {}
