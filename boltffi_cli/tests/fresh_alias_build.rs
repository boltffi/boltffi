//! The first build of a fresh checkout, before any `boltffi generate`, still expands
//! exports that name their types through aliases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use boltffi_binding::RecordStore;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fresh_alias_build")
}

#[test]
fn a_fresh_checkout_builds_exports_that_name_types_through_aliases() {
    let build_directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fresh_alias_build");
    let record_stores = build_directory.join(RecordStore::DIRECTORY);
    if record_stores.exists() {
        fs::remove_dir_all(&record_stores).expect("start from a checkout without a record store");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_boltffi"))
        .args(["build", "wasm"])
        .current_dir(fixture())
        .env("CARGO_TARGET_DIR", &build_directory)
        .env("CARGO_BUILD_BUILD_DIR", &build_directory)
        .output()
        .expect("the boltffi binary runs");

    assert!(
        output.status.success(),
        "`boltffi build wasm` failed before any `boltffi generate`. An export naming \
         `Coordinate` or `Route` fails with `cannot find macro` when the build runs \
         without the record store, so the build has to prepare it first.\n\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        fs::read_dir(&record_stores).is_ok_and(|mut stores| stores.next().is_some()),
        "the build keeps the record store"
    );
}
