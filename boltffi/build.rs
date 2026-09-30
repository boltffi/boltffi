/// Turns the `BOLTFFI_SOURCE_RECORDS` environment variable, which bindgen's metadata
/// build sets, into the cfg under which `__source_record!` keeps its records.
fn main() {
    println!("cargo:rerun-if-env-changed=BOLTFFI_SOURCE_RECORDS");
    println!("cargo:rustc-check-cfg=cfg(boltffi_source_records)");
    if std::env::var_os("BOLTFFI_SOURCE_RECORDS").is_some() {
        println!("cargo:rustc-cfg=boltffi_source_records");
    }
}
