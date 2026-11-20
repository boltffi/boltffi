boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone)]
pub struct T { pub c: Option<Box<T>> }
fn main() {}
