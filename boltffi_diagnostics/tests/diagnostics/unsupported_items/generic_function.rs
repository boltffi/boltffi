boltffi::scaffolding!();

#[boltffi::export]
pub fn first<T>(items: Vec<T>) -> Option<T> {
    items.into_iter().next()
}

fn main() {}
