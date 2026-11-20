boltffi::scaffolding!();


#[boltffi::export]
pub fn add(left: i32, right: i32) -> i32 {
    left + right
}
/// Writes `value` with the `itoa` crate from crates.io.
///
/// The Ruby extension calls no Rust function yet. The function only forces
/// the install build to build a crates.io dependency.
pub fn write_integer(value: i64) -> String {
    let mut buffer = itoa::Buffer::new();
    buffer.format(value).to_owned()
}
