boltffi::scaffolding!();
#[boltffi::export]
pub fn f<'a>(s: &'a str) -> &'a str { s }
fn main() {}
