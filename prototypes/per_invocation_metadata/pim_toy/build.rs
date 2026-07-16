use std::env;
use std::fs;
use std::path::PathBuf;

const GENERATED: &str = r#"#[::pim_macros::data]
pub struct BuildScriptRecord {
    pub id: u64,
    pub anchor: crate::geometry::Point,
}
"#;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("generated.rs"), GENERATED).expect("OUT_DIR is writable");
    println!("cargo::rerun-if-changed=build.rs");
}
