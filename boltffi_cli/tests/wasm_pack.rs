use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
#[ignore = "requires the Wasm target, tsc, and prepared examples/platforms/wasm dependencies"]
fn pure_packages_preserve_cargo_artifacts_and_survive_failed_repacking() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = workspace.join("boltffi_cli/tests/fixtures/wasm_wake_link");
    let package = tempfile::Builder::new()
        .prefix(".wasm-pack-test-")
        .tempdir_in(workspace.join("examples/platforms/wasm"))
        .unwrap();
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let artifact = target.join("wasm32-unknown-unknown/debug/wasm_wake_link_fixture.wasm");
    let sources = package.path().join("sources");
    let output = package.path().join("dist");
    let config = package.path().join("overlay.toml");
    let mut configuration = serde_json::json!({
        "targets": { "wasm": {
            "profile": "debug",
            "artifact_path": artifact,
            "optimize": { "enabled": false },
            "typescript": { "output": sources, "module_name": "fixture" },
            "npm": { "output": output, "package_name": "fixture" }
        } }
    });
    fs::write(&config, toml::to_string(&configuration).unwrap()).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_boltffi"))
        .args(["build", "wasm"])
        .current_dir(&fixture)
        .env("CARGO_TARGET_DIR", target)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let raw = fs::read(&artifact).unwrap();
    let mut pack = Command::new(env!("CARGO_BIN_EXE_boltffi"));
    pack.arg("--overlay")
        .arg(&config)
        .args(["pack", "wasm", "--no-build"])
        .current_dir(&fixture)
        .env("CARGO_TARGET_DIR", target)
        .env(
            "BOLTFFI_WASM_BINDGEN",
            package.path().join("no-cli-installed"),
        );
    let first = pack.output().unwrap();
    assert!(
        first.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(fs::read(&artifact).unwrap(), raw);
    assert!(output.join("fixture_imports.js").exists());
    assert!(!output.join("fixture_bg.js").exists());
    let node = Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            r#"
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as synchronous from './fixture_node.js';
import * as asynchronous from './fixture.js';
await asynchronous.default(readFileSync('./fixture_bg.wasm'));
await Promise.all([synchronous, asynchronous].map(async (module) => {
    assert.equal(module.u64Bytes, 8);
    assert.equal(await module.answer(), 42);
    assert.equal(module.invoke(value => value + 1, 41), 42);
}));
"#,
        ])
        .current_dir(&output)
        .output()
        .unwrap();
    assert!(
        node.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&node.stdout),
        String::from_utf8_lossy(&node.stderr)
    );
    let packaged_wasm = fs::read(output.join("fixture_bg.wasm")).unwrap();
    fs::remove_file(sources.join("fixture_imports.ts")).unwrap();
    fs::remove_file(sources.join("fixture_node.ts")).unwrap();
    configuration["targets"]["wasm"]["npm"]["targets"] = serde_json::json!(["web"]);
    configuration["targets"]["wasm"]["typescript"]["source_map"] = serde_json::json!(false);
    configuration["targets"]["wasm"]["npm"]["generate_package_json"] = serde_json::json!(false);
    configuration["targets"]["wasm"]["npm"]["generate_readme"] = serde_json::json!(false);
    fs::write(output.join("README.md"), "a custom readme").unwrap();
    let package_json = fs::read(output.join("package.json")).unwrap();
    fs::write(&config, toml::to_string(&configuration).unwrap()).unwrap();
    pack.args(["--regenerate", "false"]);
    let repeated = pack.output().unwrap();
    assert!(
        repeated.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&repeated.stdout),
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert_eq!(fs::read(&artifact).unwrap(), raw);
    assert_eq!(
        fs::read(output.join("fixture_bg.wasm")).unwrap(),
        packaged_wasm
    );
    assert!(!output.join("fixture_node.js").exists());
    assert!(!output.join("fixture.js.map").exists());
    assert_eq!(
        fs::read_to_string(output.join("README.md")).unwrap(),
        "a custom readme"
    );
    assert_eq!(fs::read(output.join("package.json")).unwrap(), package_json);
    let previous: Vec<_> = fs::read_dir(&output)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    [
        ("fixture_node.ts", "export const broken: number = 'wrong';"),
        ("fixture.ts", "export const broken: number = 'wrong';"),
        (
            "fixture.ts",
            "import * as runtime from '@boltffi/runtime'; export const imports = { missing: runtime['__missing_wasm_import'] };",
        ),
    ]
    .into_iter()
    .for_each(|(name, contents)| {
        let source = sources.join(name);
        fs::write(&source, contents).unwrap();
        let failed = pack.output().unwrap();
        assert!(
            !failed.status.success(),
            "pack accepted invalid TypeScript: {contents}"
        );
        assert!(String::from_utf8_lossy(&failed.stderr).contains("tsc failed"));
        previous
            .iter()
            .for_each(|(path, bytes)| assert_eq!(&fs::read(path).unwrap(), bytes));
        assert_eq!(fs::read(&artifact).unwrap(), raw);
        fs::remove_file(source).unwrap();
    });
}
