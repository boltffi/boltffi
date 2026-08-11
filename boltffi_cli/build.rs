//! Builds `runtime/typescript` from source and embeds the fresh output
//! into the `boltffi` binary, so `pack dart-web` can vendor it with zero
//! npm/Node dependency on the consuming side. A hand-copied snapshot
//! drifts out of sync with `runtime/typescript/src` silently -- building
//! from source at compile time means it can't be stale.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

include!("src/pack/dart_web/runtime_sources_codegen.rs");

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime_dir = manifest_dir.join("../runtime/typescript");
    let src_dir = runtime_dir.join("src");
    let dist_dir = runtime_dir.join("dist");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by cargo"));

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        runtime_dir.join("package.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        runtime_dir.join("tsconfig.json").display()
    );

    if !src_dir.exists() {
        // A packaged crate (`cargo install`/crates.io) never carries a
        // sibling `runtime/typescript` -- Cargo's own packaging can't
        // include paths outside the crate root. Degrade instead of
        // panicking so `boltffi_cli` stays buildable there; `pack
        // dart-web` fails with a clear error at runtime instead, in the
        // rare case someone still asks for it from a build like this.
        println!(
            "cargo:warning=boltffi_cli build.rs: `runtime/typescript/src` not found at {} \
             (not a full boltffi monorepo checkout) -- pack dart-web's vendored runtime \
             will be empty in this build",
            src_dir.display()
        );
        let generated_path = out_dir.join("dart_web_runtime_sources.rs");
        fs::write(&generated_path, render_runtime_sources_source(&[]))
            .expect("write generated runtime sources file");
        return;
    }

    build_runtime(&runtime_dir);

    let embedded_dir = out_dir.join("dart_web_runtime");
    fs::create_dir_all(&embedded_dir).expect("create embedded runtime output directory");

    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&dist_dir)
        .unwrap_or_else(|error| {
            panic!(
                "boltffi_cli build.rs: reading {} after building runtime/typescript failed: {error}",
                dist_dir.display()
            )
        })
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "js"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("dist entry has a file name")
                .to_string_lossy()
                .into_owned();
            (name, path)
        })
        .collect();

    if entries.is_empty() {
        panic!(
            "boltffi_cli build.rs: `runtime/typescript` built successfully but {} contains no .js files",
            dist_dir.display()
        );
    }
    entries.sort();

    let codegen_entries: Vec<(String, String)> = entries
        .iter()
        .map(|(name, source_path)| {
            let embedded_path = embedded_dir.join(name);
            fs::copy(source_path, &embedded_path).unwrap_or_else(|error| {
                panic!(
                    "boltffi_cli build.rs: copying {} to {} failed: {error}",
                    source_path.display(),
                    embedded_path.display()
                )
            });
            (
                name.clone(),
                embedded_path.display().to_string().replace('\\', "/"),
            )
        })
        .collect();
    let generated = render_runtime_sources_source(&codegen_entries);

    let generated_path = out_dir.join("dart_web_runtime_sources.rs");
    fs::write(&generated_path, generated).expect("write generated runtime sources file");
}

fn build_runtime(runtime_dir: &Path) {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

    if !runtime_dir.join("node_modules").exists() {
        run(Command::new(npm).arg("install").current_dir(runtime_dir));
    }

    run(Command::new(npm)
        .args(["run", "build"])
        .current_dir(runtime_dir));
}

fn run(command: &mut Command) {
    let status = command.status().unwrap_or_else(|error| {
        panic!(
            "boltffi_cli build.rs: failed to run `{:?}` — is Node.js/npm installed and on PATH? ({error})",
            command
        )
    });
    if !status.success() {
        panic!("boltffi_cli build.rs: `{:?}` exited with {status}", command);
    }
}
