boltffi::scaffolding!();
#[boltffi::export]
pub fn f(#[boltffi::default("x")] a: i32) -> i32 {
    a
}
fn main() {}
