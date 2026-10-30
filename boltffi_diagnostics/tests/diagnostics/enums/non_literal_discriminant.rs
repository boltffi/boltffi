boltffi::scaffolding!();
const B: i32 = 1;
#[boltffi::data]
#[derive(Clone, Copy)]
pub enum E { A = B + 1 }
fn main() {}
