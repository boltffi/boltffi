boltffi::scaffolding!();
#[boltffi::export]
pub fn f(g: fn(i32) -> i32) -> i32 {
    g(1)
}
fn main() {}
