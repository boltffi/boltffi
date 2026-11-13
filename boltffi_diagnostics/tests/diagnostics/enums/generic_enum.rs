boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone, Copy)]
pub enum E<T> { A(T) }
fn main() {}
