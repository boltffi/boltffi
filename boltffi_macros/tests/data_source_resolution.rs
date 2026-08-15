use std::path::PathBuf;
use std::process::Command;

#[test]
fn external_module_data_resolves_imported_types_across_feature_sets() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/data_source_resolution/Cargo.toml");
    let target = std::env::temp_dir().join(format!(
        "boltffi-data-source-resolution-{}",
        std::process::id()
    ));
    if target.exists() {
        std::fs::remove_dir_all(&target).expect("remove stale fixture target directory");
    }
    std::fs::create_dir(&target).expect("create fixture target directory");

    let check = |configuration: &str, arguments: &[&str]| {
        let output = Command::new(env!("CARGO"))
            .args(["check", "--locked"])
            .args(arguments)
            .arg("--manifest-path")
            .arg(&fixture)
            .env("CARGO_TARGET_DIR", &target)
            .output()
            .expect("fixture cargo check starts");

        assert!(
            output.status.success(),
            "{configuration} data source resolution fixture failed\nstdout\n{}\nstderr\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    check("default", &[]);
    check("experimental", &["--all-features"]);

    let cleanup = std::fs::remove_dir_all(&target);
    cleanup.expect("remove fixture target directory");
}
