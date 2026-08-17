//! A wasm build has to link once a crate exports anything async.
//!
//! `boltffi_core` declares `__boltffi_wake` and `__boltffi_stream_wake` as
//! `extern "C"`. The host supplies them at instantiation, so they have to be
//! declared as wasm imports; left bare, the linker looks for a definition and
//! does not find one.
//!
//! This is only observable on a toolchain that does not pass
//! `--allow-undefined` on a wasm cdylib link, which rustc stopped doing
//! between 1.95 and 1.97. `rust-toolchain.toml` pins 1.95.0, so running this
//! the ordinary way proves nothing; CI runs it again under `stable`, where the
//! flag is gone and an undeclared import is a hard error.
//!
//! It is also why `examples/demo` never caught this. It exports async and
//! streams and is packed for wasm in CI, but at the pinned toolchain.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wasm_wake_link")
}

#[test]
fn a_crate_exporting_an_async_function_links_for_wasm() {
    let output = Command::new(env!("CARGO_BIN_EXE_boltffi"))
        .args(["build", "wasm"])
        .current_dir(fixture())
        // Keeps the fixture's artifacts out of the repository, and off the
        // package lock this test already holds.
        .env("CARGO_TARGET_DIR", env!("CARGO_TARGET_TMPDIR"))
        .output()
        .expect("the boltffi binary runs");

    assert!(
        output.status.success(),
        "`boltffi build wasm` failed for a crate whose only export is an async \
         function.\n\n\
         If the linker names `__boltffi_wake` or `__boltffi_stream_wake`, their \
         declarations in `boltffi_core` are missing \
         `#[link(wasm_import_module = \"env\")]`, the attribute the generated \
         callback imports already carry. `build` does not forward the linker's \
         message; run `boltffi -vv pack wasm` in {} to read it.\n\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        fixture().display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
