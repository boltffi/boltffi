boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct P<'a> { pub x: &'a str }
fn main() {}
