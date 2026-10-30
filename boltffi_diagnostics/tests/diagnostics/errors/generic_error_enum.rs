boltffi::scaffolding!();
#[boltffi::error]
#[derive(Clone, Debug)]
pub enum E<T> { A(T) }
fn main() {}
