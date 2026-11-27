boltffi::scaffolding!();
#[boltffi::export]
pub fn f(#[boltffi::default(300)] a: u8) -> u8 {
    a
}
fn main() {}
