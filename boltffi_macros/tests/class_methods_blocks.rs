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
fn methods_blocks_extend_the_class_its_struct_declares() {
    let output = check("declared", &[]);

    assert!(
        output.status.success(),
        "class methods blocks fixture failed\nstdout\n{}\nstderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_methods_block_without_an_exported_struct_names_the_missing_class() {
    let output = check("undeclared", &["--features", "undeclared"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a methods block compiled without its class\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("`Orphan` is not an exported class")
            && stderr.contains("declare the class with `#[export]` on the struct or enum `Orphan`"),
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
        stderr.contains("`Counter` is an exported class; use `#[export] impl`"),
        "data impl on class fixture failed for another reason\nstderr\n{stderr}"
    );
}

#[test]
fn thread_safety_on_a_methods_block_points_at_the_declaring_type() {
    let output = check(
        "misplaced-thread-safety",
        &["--features", "misplaced_thread_safety"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a methods block took a thread-safety argument\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains(
            "`#[export] impl` takes no arguments; write `#[export(single_threaded)]` on \
             `Cursor`, which declares the class"
        ) && stderr.contains(
            "`#[export] impl` takes no arguments; write `#[export(single_threaded)]` on \
             `Gear`, which declares the class"
        ),
        "misplaced thread safety fixture failed for another reason\nstderr\n{stderr}"
    );
}

#[test]
fn a_class_that_is_not_sync_is_reported_at_its_struct() {
    let output = check("unsync", &["--features", "unsync"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a class holding a Cell compiled as thread safe\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("`Cell<u32>` cannot be shared between threads safely")
            && stderr.contains("src/unsync.rs:5:1"),
        "unsync fixture failed for another reason\nstderr\n{stderr}"
    );
}

#[test]
fn an_argument_on_a_methods_block_is_moved_only_when_the_struct_takes_it() {
    let output = check("impl-arguments", &["--features", "impl_arguments"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a methods block took an argument\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains(
            "`#[export] impl` takes no arguments; every `#[export] impl` adds methods to the \
             class that `#[export]` on `Cursor` declares"
        ) && stderr
            .matches("`#[export] impl` takes no arguments")
            .count()
            == 2,
        "impl arguments fixture failed for another reason\nstderr\n{stderr}"
    );
    assert!(
        !stderr.contains("write `#[export(sideways)]`")
            && !stderr.contains("write `#[export(methods)]`"),
        "an argument the struct does not take was suggested for it\nstderr\n{stderr}"
    );
}

#[test]
fn a_type_that_is_both_data_and_a_class_is_refused_once_either_way() {
    let output = check("data-and-export", &["--features", "data_and_export"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let struct_refusal = "a struct is either a `#[data]` record or an `#[export]` class, not both";
    let enum_refusal = "an enum is either a `#[data]` enum or an `#[export]` class, not both";
    let error_refusal = "an enum is either an `#[error]` enum or an `#[export]` class, not both";

    assert!(
        !output.status.success(),
        "a type compiled as both data and a class\nstderr\n{stderr}"
    );
    assert_eq!(
        (
            stderr.matches(struct_refusal).count(),
            stderr.matches(enum_refusal).count(),
            stderr.matches(error_refusal).count()
        ),
        (2, 2, 2),
        "data and export fixture failed for another reason\nstderr\n{stderr}"
    );
    assert!(
        !stderr.contains("defined multiple times")
            && !stderr.contains("conflicting implementations"),
        "the refusal came with the raw duplicate-declaration errors\nstderr\n{stderr}"
    );
}

#[test]
fn a_methods_block_on_a_custom_type_says_custom_types_take_none() {
    let output = check("custom-methods", &["--features", "custom_methods"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a class methods block compiled on a custom type\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains("`Email` is a custom type, and a custom type takes no methods blocks")
            && !stderr.contains("use `#[data(impl)]`"),
        "custom methods fixture failed for another reason\nstderr\n{stderr}"
    );
}

#[test]
fn a_mutable_receiver_on_a_thread_safe_class_points_at_the_declaring_type() {
    let output = check("mutable-receiver", &["--features", "mutable_receiver"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a &mut self method compiled on a thread-safe class\nstderr\n{stderr}"
    );
    assert!(
        stderr.contains(
            "a `&mut self` method needs `#[export(single_threaded)]` on its class's struct or enum"
        ),
        "mutable receiver fixture failed for another reason\nstderr\n{stderr}"
    );
}
