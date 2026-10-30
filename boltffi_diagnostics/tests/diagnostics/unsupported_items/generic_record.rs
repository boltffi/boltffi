boltffi::scaffolding!();

#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Pair<T> {
    pub left: T,
    pub right: T,
}

fn main() {}
