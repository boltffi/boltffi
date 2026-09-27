#![cfg(unix)]

use std::env::{
    self,
    consts::{ARCH, OS},
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct WasmProject {
    directory: TempDir,
}

impl WasmProject {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let project = Self { directory };
        project.artifact("0.2.129");
        project.install(
            &project.directory.path().join("bin/wasm-bindgen"),
            "0.2.100",
            "unrelated CLI on PATH",
        );
        fs::write(project.directory.path().join("Cargo.toml"), "[package]\nname = 'fixture'\nversion = '0.1.0'\n[lib]\npath = 'lib.rs'\ncrate-type = ['cdylib']\n").unwrap();
        fs::write(project.directory.path().join("lib.rs"), "").unwrap();
        fs::write(
            project.directory.path().join("boltffi.toml"),
            r#"[package]
name = "fixture"
[targets.wasm]
artifact_path = "fixture.wasm"
[targets.wasm.npm]
package_name = "fixture"
"#,
        )
        .unwrap();
        project
    }

    fn artifact(&self, version: &str) {
        let header = format!(r#"{{"version":"{version}"}}"#);
        let metadata = (header.len() as u32)
            .to_le_bytes()
            .into_iter()
            .chain(header.bytes())
            .chain(0u32.to_le_bytes())
            .map(|byte| format!("\\{byte:02x}"))
            .collect::<String>();
        let module = wat::parse_str(format!(
            r#"(module (@custom "__wasm_bindgen_unstable" "{metadata}"))"#
        ))
        .unwrap();
        fs::write(self.directory.path().join("fixture.wasm"), module).unwrap();
    }

    fn cached(&self, version: &str) -> PathBuf {
        self.directory
            .path()
            .join("cache/wasm-bindgen")
            .join(format!("{version}-{OS}-{ARCH}/wasm-bindgen"))
    }

    fn install(&self, executable: &Path, version: &str, label: &str) {
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(
            executable,
            format!(
                r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    printf '%s\n' 'wasm-bindgen {version}'
    exit 0
fi
printf '%s\n' '{label}' >&2
exit 42
"#
            ),
        )
        .unwrap();
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boltffi"));
        command
            .args(["pack", "wasm", "--no-build", "--regenerate", "false"])
            .current_dir(self.directory.path())
            .env(
                "PATH",
                env::join_paths(
                    std::iter::once(self.directory.path().join("bin"))
                        .chain(env::split_paths(&env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            )
            .env("BOLTFFI_CACHE_DIR", self.directory.path().join("cache"))
            .env("CARGO_HOME", self.directory.path().join("cargo-home"))
            .env("BOLTFFI_TEST_CARGO", env!("CARGO"))
            .env_remove("BOLTFFI_WASM_BINDGEN")
            .env_remove("CARGO_NET_OFFLINE");
        command
    }

    fn assert_error(&self, output: Output, expected: &str) {
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
    }
}

#[test]
fn projects_with_different_versions_use_separate_cached_tools_offline() {
    let project = WasmProject::new();
    let unrelated_cli = project.directory.path().join("bin/wasm-bindgen");
    project.install(&unrelated_cli, "0.2.100", "wrong tool on PATH");
    ["0.2.122", "0.2.129"].into_iter().for_each(|version| {
        project.install(
            &project.cached(version),
            version,
            &format!("selected {version}"),
        );
        project.artifact(version);
        project.assert_error(
            project
                .command()
                .env("CARGO_NET_OFFLINE", "true")
                .output()
                .unwrap(),
            &format!("selected {version}"),
        );
    });
    assert!(
        fs::read_to_string(unrelated_cli)
            .unwrap()
            .contains("0.2.100")
    );
}

#[test]
fn an_explicit_override_is_never_silently_replaced() {
    let project = WasmProject::new();
    project.install(&project.cached("0.2.129"), "0.2.129", "cached CLI");
    let overridden = project.directory.path().join("custom-cli");
    project.install(&overridden, "0.2.122", "mismatched CLI");
    project.assert_error(
        project
            .command()
            .env("BOLTFFI_WASM_BINDGEN", &overridden)
            .output()
            .unwrap(),
        "the Wasm artifact requires 0.2.129",
    );
    fs::remove_file(&overridden).unwrap();
    project.assert_error(
        project
            .command()
            .env("BOLTFFI_WASM_BINDGEN", &overridden)
            .output()
            .unwrap(),
        "could not run wasm-bindgen",
    );
}

#[test]
fn cargo_install_preserves_offline_settings_and_reports_failures() {
    let project = WasmProject::new();
    let installer = project.directory.path().join("bin/cargo");
    fs::write(
        &installer,
        r#"#!/bin/sh
if [ "$1" != "install" ]; then
    exec "$BOLTFFI_TEST_CARGO" "$@"
fi
printf '%s\n' "$@" > cargo-install-arguments
printf '%s\n' "${RUSTFLAGS-unset}" >> cargo-install-arguments
printf '%s\n' 'installation refused by fixture' >&2
exit 17
"#,
    )
    .unwrap();
    fs::set_permissions(&installer, fs::Permissions::from_mode(0o755)).unwrap();
    let assert_offline = |mut command: Command| {
        project.assert_error(command.output().unwrap(), "installation refused by fixture");
        let arguments =
            fs::read_to_string(project.directory.path().join("cargo-install-arguments")).unwrap();
        assert!(
            arguments
                .lines()
                .any(|argument| argument == "net.offline=true"),
            "{arguments}"
        );
        assert!(
            arguments.lines().any(|argument| argument == "--locked"),
            "{arguments}"
        );
        assert!(!project.cached("0.2.129").exists());
        assert_eq!(
            fs::read_dir(project.cached("0.2.129").parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    };
    ["--offline", "--frozen", "--config=net.offline=true"]
        .into_iter()
        .for_each(|argument| {
            let mut command = project.command();
            command.arg(format!("--cargo-arg={argument}"));
            assert_offline(command);
        });
    let mut command = project.command();
    command.env("CARGO_NET_OFFLINE", "true");
    assert_offline(command);
    let cargo_config = project.directory.path().join(".cargo/config.toml");
    fs::create_dir(cargo_config.parent().unwrap()).unwrap();
    fs::write(&cargo_config, "[net]\noffline = true\n").unwrap();
    assert_offline(project.command());
    fs::write(&cargo_config, "[net]\noffline = false\n").unwrap();
    let legacy_config = cargo_config.with_extension("");
    fs::write(&legacy_config, "[net]\noffline = true\n").unwrap();
    assert_offline(project.command());
    fs::remove_file(legacy_config).unwrap();
    fs::write(&cargo_config, "[net]\noffline = true\n").unwrap();
    let failure = project
        .command()
        .arg("--cargo-arg=--config=net.offline=false")
        .env("RUSTFLAGS", "--cfg=host_install_fixture")
        .output()
        .unwrap();
    project.assert_error(
        failure,
        "cargo install wasm-bindgen-cli --version =0.2.129 --locked",
    );
    let arguments =
        fs::read_to_string(project.directory.path().join("cargo-install-arguments")).unwrap();
    assert!(
        arguments
            .lines()
            .any(|argument| argument == "net.offline=false"),
        "{arguments}"
    );
    assert!(
        arguments
            .lines()
            .any(|argument| argument == "--cfg=host_install_fixture"),
        "{arguments}"
    );
}

#[test]
fn a_cold_offline_install_fails_without_publishing_a_tool() {
    let project = WasmProject::new();
    let output = project
        .command()
        .arg("--cargo-arg=--offline")
        .output()
        .unwrap();
    project.assert_error(output, "could not build wasm-bindgen 0.2.129");
    assert!(!project.cached("0.2.129").exists());
    assert_eq!(
        fs::read_dir(project.cached("0.2.129").parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn concurrent_packs_publish_one_complete_install_and_retry_failures() {
    let project = WasmProject::new();
    let installer = project.directory.path().join("bin/cargo");
    project.install(
        &project.directory.path().join("built-cli"),
        "0.2.129",
        "installed CLI",
    );
    fs::write(
        &installer,
        r#"#!/bin/sh
if [ "$1" != "install" ]; then
    exec "$BOLTFFI_TEST_CARGO" "$@"
fi
printf 'attempt\n' >> install-attempts
while [ "$#" -gt 0 ]; do
    if [ "$1" = "--root" ]; then
        shift
        install_root="$1"
    fi
    shift
done
mkdir -p "$install_root/bin"
if [ -f refuse-install ]; then
    printf 'partial binary' > "$install_root/bin/wasm-bindgen"
    printf 'interrupted installation\n' >&2
    exit 17
fi
cp built-cli "$install_root/bin/wasm-bindgen"
"#,
    )
    .unwrap();
    fs::set_permissions(&installer, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(project.directory.path().join("refuse-install"), "").unwrap();
    project.assert_error(
        project.command().output().unwrap(),
        "interrupted installation",
    );
    assert!(!project.cached("0.2.129").exists());
    fs::remove_file(project.directory.path().join("refuse-install")).unwrap();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| project.command().output().unwrap());
        let second = scope.spawn(|| project.command().output().unwrap());
        project.assert_error(first.join().unwrap(), "installed CLI");
        project.assert_error(second.join().unwrap(), "installed CLI");
    });
    let attempts = fs::read_to_string(project.directory.path().join("install-attempts")).unwrap();
    assert_eq!(attempts.lines().count(), 2);
    assert_eq!(
        fs::read(project.cached("0.2.129")).unwrap(),
        fs::read(project.directory.path().join("built-cli")).unwrap()
    );
    assert_eq!(
        fs::read_dir(project.cached("0.2.129").parent().unwrap())
            .unwrap()
            .count(),
        2
    );
}
