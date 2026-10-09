//! `boltffi::wasm_getrandom_backend!` gives a wasm module entropy through one
//! host import, `env.__boltffi_getrandom`, which `@boltffi/runtime` supplies.
//!
//! The runtime satisfies nothing else, so the module must link with that
//! import declared, and must not pick up any other: in particular none of
//! `wasm-bindgen`'s, which is what `getrandom`'s own `wasm_js` backend would
//! bring and what a BoltFFI package cannot provide.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wasm_getrandom")
}

/// The module and field of every import in a wasm module.
fn imports(module: &[u8]) -> Vec<(String, String)> {
    assert_eq!(&module[..4], b"\0asm", "not a wasm module");

    fn uleb(bytes: &[u8], cursor: &mut usize) -> usize {
        let (mut value, mut shift) = (0usize, 0);
        loop {
            let byte = bytes[*cursor];
            *cursor += 1;
            value |= usize::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                return value;
            }
        }
    }
    fn name(bytes: &[u8], cursor: &mut usize) -> String {
        let len = uleb(bytes, cursor);
        let name = String::from_utf8(bytes[*cursor..*cursor + len].to_vec()).unwrap();
        *cursor += len;
        name
    }

    const IMPORT_SECTION: u8 = 2;
    let mut cursor = 8;
    while cursor < module.len() {
        let id = module[cursor];
        cursor += 1;
        let len = uleb(module, &mut cursor);
        if id != IMPORT_SECTION {
            cursor += len;
            continue;
        }
        let count = uleb(module, &mut cursor);
        return (0..count)
            .map(|_| {
                let import = (name(module, &mut cursor), name(module, &mut cursor));
                // Only functions are expected; anything else fails the
                // assertion below before a misparse could matter.
                let kind = module[cursor];
                cursor += 1;
                if kind == 0 {
                    uleb(module, &mut cursor);
                }
                import
            })
            .collect();
    }
    Vec::new()
}

#[test]
fn the_getrandom_backend_imports_only_the_runtime_entropy_source() {
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wasm_getrandom");
    // `RUSTFLAGS` outranks a fixture's `.cargo/config.toml`, and CI sets it,
    // so the backend is selected here, on top of whatever it already holds.
    let mut rustflags = std::env::var_os("RUSTFLAGS").unwrap_or_default();
    rustflags.push(OsString::from(" --cfg getrandom_backend=\"custom\""));

    let output = Command::new(env!("CARGO_BIN_EXE_boltffi"))
        .args(["build", "wasm"])
        .current_dir(fixture())
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("RUSTFLAGS", rustflags)
        .output()
        .expect("the boltffi binary runs");
    assert!(
        output.status.success(),
        "`boltffi build wasm` failed for a crate using `wasm_getrandom_backend!`. \
         If the linker names `__boltffi_getrandom`, its declaration in \
         `boltffi_core` has lost `#[link(wasm_import_module = \"env\")]`; if it \
         names `__getrandom_v03_custom`, the macro no longer defines it.\n\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let module =
        std::fs::read(target_dir.join("wasm32-unknown-unknown/debug/wasm_getrandom_fixture.wasm"))
            .expect("the build wrote the module");
    assert_eq!(
        imports(&module),
        [("env".to_owned(), "__boltffi_getrandom".to_owned())],
    );
}
