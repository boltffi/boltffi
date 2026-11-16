boltffi::scaffolding!();
#[boltffi::export]
#[boltffi::default(1)]
pub fn f(a: i32) -> i32 {
    a
}
fn main() {}
