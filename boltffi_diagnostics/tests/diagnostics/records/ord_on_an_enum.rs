boltffi::scaffolding!();
#[boltffi::data(ord)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Level { Low, High }
fn main() {}
