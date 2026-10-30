boltffi::scaffolding!();
pub struct R;
boltffi::custom_type! { pub L, remote = R, repr = String, into_ffi = |s: &R| String::new(), try_from_ffi = |s: String| R, }
fn main() {}
