//! A Ruby gem from a crate whose exports name types through aliases builds at install.
//!
//! The install builds the crate copy in a new folder with a new build directory. That
//! directory holds no record store, so the copy carries a portable store, and the build
//! names it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use boltffi_binding::RecordStore;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ruby_alias_pack")
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the command runs")
}

fn text(output: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

/// Copies `from` into `to`, file by file.
fn copy_folder(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.expect("walk the crate copy");
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn a_crate_copy_with_alias_exports_builds_in_a_new_folder_with_the_portable_store() {
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ruby_alias_pack");
    if work.exists() {
        fs::remove_dir_all(&work).expect("start from an empty work folder");
    }
    fs::create_dir_all(&work).unwrap();
    let gem = work.join("gem");
    let overlay = work.join("overlay.toml");
    fs::write(
        &overlay,
        format!("[targets.ruby]\noutput = {:?}\n", gem.display().to_string()),
    )
    .unwrap();

    let lockfile = run(Command::new(env!("CARGO"))
        .args(["generate-lockfile", "--manifest-path"])
        .arg(fixture().join("Cargo.toml")));
    assert!(lockfile.status.success(), "{}", text(&lockfile));

    let pack = run(Command::new(env!("CARGO_BIN_EXE_boltffi"))
        .arg("--overlay")
        .arg(&overlay)
        .args([
            "pack",
            "ruby",
            "--experimental",
            "--allow-external-path-dependencies",
        ])
        .current_dir(fixture())
        .env("CARGO_TARGET_DIR", work.join("pack-target"))
        .env_remove("CARGO_BUILD_BUILD_DIR"));
    assert!(pack.status.success(), "{}", text(&pack));

    // The install unpacks the gem into a folder of its own.
    let installed = work.join("installed/crate");
    copy_folder(&gem.join("ext/ruby_alias_pack_fixture/crate"), &installed);
    let store = installed.join(RecordStore::PORTABLE_FILE);
    assert!(store.is_file(), "the crate copy carries the portable store");

    let build = |with_store: bool| {
        let mut command = Command::new(env!("CARGO"));
        command
            .args(["build", "--locked", "--lib", "--manifest-path"])
            .arg(installed.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(work.join("install-target"))
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_BUILD_DIR")
            .env_remove(RecordStore::PORTABLE_ENV);
        if with_store {
            command.env(RecordStore::PORTABLE_ENV, &store);
        }
        run(&mut command)
    };

    let without_store = build(false);
    assert!(
        !without_store.status.success()
            && String::from_utf8_lossy(&without_store.stderr).contains("cannot find macro"),
        "the copy did not fail on the alias without the store, so this test no longer \
         proves that the store matters\n{}",
        text(&without_store)
    );
    let with_store = build(true);
    assert!(
        with_store.status.success(),
        "the crate copy failed to build with the portable store\n{}",
        text(&with_store)
    );
}
