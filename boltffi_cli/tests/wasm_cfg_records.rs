use std::path::Path;
use std::process::Command;

#[test]
fn typescript_bindings_follow_the_wasm_build() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wasm_cfg_records");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wasm_cfg_records");
    let output = tempfile::tempdir().expect("output directory");
    let generate = Command::new(env!("CARGO_BIN_EXE_boltffi"))
        .args(["generate", "typescript", "--experimental", "-o"])
        .arg(output.path())
        .current_dir(&fixture)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("the boltffi binary runs");
    assert!(
        generate.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&generate.stdout),
        String::from_utf8_lossy(&generate.stderr)
    );

    let bindings = std::fs::read_dir(output.path())
        .expect("generated bindings")
        .map(|entry| std::fs::read_to_string(entry.expect("entry").path()).unwrap_or_default())
        .collect::<String>();
    assert!(
        bindings.contains("wasmOnly"),
        "the wasm-only field is missing from the TypeScript bindings"
    );
    assert!(
        !bindings.contains("nativeOnly"),
        "the host-only field leaked into the TypeScript bindings"
    );
}
