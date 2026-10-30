boltffi::scaffolding!();
#[boltffi::export]
pub fn f() -> impl Iterator<Item = i32> {
    0..1
}
fn main() {}
