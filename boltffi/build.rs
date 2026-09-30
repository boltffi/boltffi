/// Turns `BOLTFFI_SOURCE_RECORDS=1`, which bindgen's metadata build sets, into the
/// cfg under which `__source_record!` keeps its records. Any other value leaves it off.
fn main() {
    println!("cargo:rerun-if-env-changed=BOLTFFI_SOURCE_RECORDS");
    println!("cargo:rustc-check-cfg=cfg(boltffi_source_records)");
    if std::env::var_os("BOLTFFI_SOURCE_RECORDS").is_some_and(|value| value == "1") {
        println!("cargo:rustc-cfg=boltffi_source_records");
    }
}
