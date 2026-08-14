// Stages the generated Dart callback-dispatch shim (`dart_shims.rs`) for
// `include!()` into this crate's own source. `boltffi pack dart` points
// `BOLTFFI_DART_SHIM_RS` at the shim file it just generated; outside that
// flow (env var unset) this writes an empty stub.
fn main() {
    if std::env::var_os("CARGO_FEATURE_DART").is_none() {
        return;
    }

    println!("cargo:rerun-if-env-changed=BOLTFFI_DART_SHIM_RS");

    let requested_path = std::env::var_os("BOLTFFI_DART_SHIM_RS").map(std::path::PathBuf::from);

    let contents = match &requested_path {
        None => "// no qualifying Dart callback shims generated\n".to_string(),
        Some(path) => {
            // The env var being set means the CLI promised a real shim file
            // at this path; if it's missing, that's a broken handoff, not
            // "no shims to generate" -- fail loudly instead of silently
            // producing a cdylib the generated Dart bindings can't call into.
            if !path.exists() {
                panic!(
                    "BOLTFFI_DART_SHIM_RS is set to {} but that file does not exist. \
                     This crate was built expecting a generated Dart callback shim that \
                     isn't there -- regenerate Dart bindings (`boltffi pack dart` without \
                     `--regenerate false`) before building, or unset BOLTFFI_DART_SHIM_RS \
                     if this crate genuinely has no Dart callback shims to build.",
                    path.display()
                );
            }
            println!("cargo:rerun-if-changed={}", path.display());
            std::fs::read_to_string(path).unwrap_or_else(|error| {
                panic!(
                    "BOLTFFI_DART_SHIM_RS is set to {} but it could not be read: {error}",
                    path.display()
                )
            })
        }
    };

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let dest = std::path::Path::new(&out_dir).join("dart_shims.rs");
    std::fs::write(&dest, contents).expect("write staged dart_shims.rs into OUT_DIR");
}
