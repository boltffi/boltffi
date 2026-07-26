use std::{path::PathBuf, process::Command};

fn check_fixture(name: &str) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("Cargo.toml");
    let target_dir = std::env::temp_dir().join(format!(
        "boltffi-{name}-interned-string-pool-{}",
        std::process::id()
    ));
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir).expect("remove stale fixture target directory");
    }
    std::fs::create_dir(&target_dir).expect("create fixture target directory");

    let output = Command::new(env!("CARGO"))
        .args(["check", "--locked", "--manifest-path"])
        .arg(&fixture)
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("fixture cargo check starts");
    let cleanup = std::fs::remove_dir_all(&target_dir);

    assert!(
        output.status.success(),
        "{name} fixture failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    cleanup.expect("remove fixture target directory");
}

#[test]
fn core_only_interned_string_pool_fixture_compiles() {
    check_fixture("core_only_interned_string_pool");
}

#[test]
fn renamed_facade_interned_string_pool_fixture_compiles() {
    check_fixture("renamed_interned_string_pool");
}

#[test]
fn renamed_core_only_interned_string_pool_fixture_compiles() {
    check_fixture("renamed_core_only_interned_string_pool");
}
