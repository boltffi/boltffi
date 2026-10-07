//! The wasm32 smoke test. Two facts differ from native, and this test pins both: wasm-ld
//! drops a dependency's unreferenced records (only its exported-symbol neighbours survive),
//! and an `include!(OUT_DIR)` export hashes a per-target-dir path into its symbol.

use std::path::{Path, PathBuf};
use std::process::Command;

use pim_reader::Resolved;

#[test]
fn wasm_records_survive_with_the_two_recorded_deviations() {
    if !wasm_target_installed() {
        eprintln!("skipping: the wasm32-unknown-unknown target is not installed");
        return;
    }

    build(&["build", "-p", "pim_toy"]);
    let wasm_dir = target().join("wasm");
    build(&[
        "build",
        "-p",
        "pim_toy",
        "--target",
        "wasm32-unknown-unknown",
        "--target-dir",
        wasm_dir.to_str().expect("the target dir is utf-8"),
    ]);

    let native = resolve(&target().join(format!(
        "{}pim_toy{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )));
    let wasm = resolve(&wasm_dir.join("wasm32-unknown-unknown/debug/pim_toy.wasm"));

    let own_items = |resolved: &Resolved| {
        resolved
            .items
            .iter()
            .filter(|item| !item.module.starts_with("pim_dep"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        own_items(&native),
        wasm.items,
        "the cdylib crate's own records survive the wasm link identically"
    );
    assert!(
        native
            .items
            .iter()
            .any(|item| item.module.starts_with("pim_dep")),
        "the native artifact keeps the dependency's records"
    );
    assert!(
        !wasm.items.iter().any(|item| item.module.starts_with("pim_dep")),
        "wasm-ld drops the dependency's unreferenced records — the inventory failure \
         native linkers avoid; wasm aggregation must read the rlibs or tether the records"
    );
    assert!(
        wasm.scaffolding
            .iter()
            .any(|scaffolding| scaffolding.module == "pim_dep"),
        "the dependency's scaffolding record survives by riding its exported free function"
    );

    for native_function in &native.functions {
        let wasm_function = wasm
            .functions
            .iter()
            .find(|function| function.canonical_id == native_function.canonical_id)
            .unwrap_or_else(|| panic!("`{}` reaches the wasm artifact", native_function.canonical_id));
        if native_function.canonical_id == "pim_toy::api::build_script_sum" {
            assert_ne!(
                native_function.symbol, wasm_function.symbol,
                "an `include!(OUT_DIR)` export hashes the per-target-dir path: two builds \
                 of the same source disagree on its symbol"
            );
        } else {
            assert_eq!(
                native_function.symbol, wasm_function.symbol,
                "`{}` keeps the same symbol on both targets",
                native_function.canonical_id
            );
        }
    }

    assert_eq!(native.classes, wasm.classes, "the class protocol is identical");
    assert_eq!(native.callbacks, wasm.callbacks, "the callback vtable order is identical");
    assert_eq!(native.streams, wasm.streams, "the stream protocol is identical");
}

fn resolve(path: &Path) -> Resolved {
    let records = pim_reader::read_artifact(path)
        .unwrap_or_else(|error| panic!("{} holds records: {error}", path.display()));
    pim_reader::resolve(&records).expect("records resolve")
}

fn wasm_target_installed() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line == "wasm32-unknown-unknown")
        })
}

fn build(arguments: &[&str]) {
    let status = Command::new(env!("CARGO"))
        .args(arguments)
        .current_dir(workspace())
        .status()
        .expect("cargo runs");
    assert!(status.success(), "pim_toy builds: cargo {arguments:?}");
}

fn target() -> PathBuf {
    std::env::current_exe()
        .expect("the test binary has a path")
        .parent()
        .and_then(Path::parent)
        .expect("the test binary lives in <target>/<profile>/deps")
        .to_path_buf()
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("pim_reader sits inside the prototype workspace")
        .to_path_buf()
}
