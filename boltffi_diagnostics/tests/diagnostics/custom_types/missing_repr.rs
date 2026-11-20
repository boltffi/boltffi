boltffi::scaffolding!();
boltffi::custom_type! { pub L, remote = String, into_ffi = |s: &String| s.clone(), try_from_ffi = |s: String| Ok(s), }
fn main() {}
