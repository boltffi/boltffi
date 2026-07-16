use std::env;
use std::fs;
use std::path::PathBuf;

const GENERATED: &str = r#"#[::pim_macros::data]
pub struct BuildScriptRecord {
    pub id: u64,
    pub anchor: crate::geometry::Point,
}
"#;

const GENERATED_API: &str = r#"#[::pim_macros::export]
pub fn build_script_sum(values: Vec<f64>) -> f64 {
    values.iter().sum()
}
"#;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("generated.rs"), GENERATED).expect("OUT_DIR is writable");
    fs::write(out.join("generated_api.rs"), GENERATED_API).expect("OUT_DIR is writable");
    println!("cargo::rerun-if-changed=build.rs");
}
