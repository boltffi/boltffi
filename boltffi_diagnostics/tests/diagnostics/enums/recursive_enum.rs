boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone)]
pub enum E { L, N(Box<E>) }
fn main() {}
