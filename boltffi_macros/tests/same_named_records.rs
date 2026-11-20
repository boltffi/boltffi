use std::path::PathBuf;
use std::process::Command;

#[test]
fn same_named_records_fail_to_compile_even_with_equal_content() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/same_named_records/Cargo.toml");
    let target =
        std::env::temp_dir().join(format!("boltffi-same-named-records-{}", std::process::id()));

    let output = Command::new(env!("CARGO"))
        .args(["check", "--locked"])
        .arg("--manifest-path")
        .arg(&fixture)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("fixture cargo check starts");
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::remove_dir_all(&target).expect("remove fixture target directory");

    assert!(
        !output.status.success(),
        "same-named records fixture compiled\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("the name `__boltffi_declared_Point` is defined multiple times"),
        "same-named records fixture failed for another reason\nstderr\n{stderr}"
    );
}
