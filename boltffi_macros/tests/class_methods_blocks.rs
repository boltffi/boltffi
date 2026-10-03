use std::path::PathBuf;
use std::process::{Command, Output};

fn check(configuration: &str, arguments: &[&str]) -> Output {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/class_methods_blocks/Cargo.toml");
    let target = std::env::temp_dir().join(format!(
        "boltffi-class-methods-blocks-{configuration}-{}",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO"))
        .args(["check", "--locked"])
        .args(arguments)
        .arg("--manifest-path")
        .arg(&fixture)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("fixture cargo check starts");
    std::fs::remove_dir_all(&target).expect("remove fixture target directory");
    output
}

#[test]
fn methods_blocks_extend_the_class_one_block_declares() {
    let output = check("declared", &[]);

    assert!(
        output.status.success(),
        "class methods blocks fixture failed\nstdout\n{}\nstderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_methods_block_without_a_declaring_block_names_the_missing_class() {
    let output = check("undeclared", &["--features", "undeclared"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a methods block compiled without its class\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("`Orphan` is not an exported class")
            && stderr.contains("no `#[export] impl Orphan` block declares this class"),
        "undeclared class fixture failed for another reason\nstderr\n{stderr}"
    );
    assert!(
        !stderr.contains("no canonical id"),
        "an undeclared class was reported as a missing data type\nstderr\n{stderr}"
    );
}

#[test]
fn a_methods_block_on_a_data_type_names_the_data_marker() {
    let output = check("methods-on-data", &["--features", "methods_on_data"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a class methods block compiled on a data type\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("`Step` is not one; use `#[data(impl)]` on a `#[data]` type"),
        "methods on data fixture failed for another reason\nstderr\n{stderr}"
    );
}

#[test]
fn a_data_methods_block_on_a_class_names_the_export_marker() {
    let output = check("data-impl-on-class", &["--features", "data_impl_on_class"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a data methods block compiled on a class\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("`Counter` is an exported class; use `#[export(methods)]`"),
        "data impl on class fixture failed for another reason\nstderr\n{stderr}"
    );
}
