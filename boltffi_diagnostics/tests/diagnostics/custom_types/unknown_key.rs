boltffi::scaffolding!();
pub struct R;
boltffi::custom_type! { pub L, remote = R, repr = String, nope = 1, into_ffi = |s: &R| String::new(), try_from_ffi = |s: String| Ok(R), }
fn main() {}
