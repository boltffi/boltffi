boltffi::scaffolding!();
#[boltffi::export]
pub fn f(a: Box<i32>) -> i32 {
    *a
}
fn main() {}
