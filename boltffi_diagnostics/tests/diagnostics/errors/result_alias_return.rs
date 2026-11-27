boltffi::scaffolding!();
pub type Result<T> = std::result::Result<T, String>;
#[boltffi::export]
pub fn f() -> Result<i32> {
    Ok(1)
}
fn main() {}
