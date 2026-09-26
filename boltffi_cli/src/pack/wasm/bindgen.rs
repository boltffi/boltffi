use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use askama::Template;
use semver::Version;

use crate::cli::{CliError, Result};
use crate::config::Config;

use super::{Error, javascript::JavaScriptModules, module::Module};

pub struct Bindgen {
    executable: PathBuf,
}

#[derive(Template)]
#[template(path = "wasm/imports.ts", escape = "none")]
struct ImportsModule<'module> {
    runtime_package: &'module str,
    glue_module: &'module str,
    modules: BTreeMap<&'module str, BTreeSet<&'module str>>,
}

impl Bindgen {
    pub fn resolve(config: &Config, version: &Version) -> Result<Self> {
        let executable = config
            .targets
            .wasm
            .wasm_bindgen_cli
            .clone()
            .or_else(|| std::env::var_os("BOLTFFI_WASM_BINDGEN").map(PathBuf::from))
            .or_else(|| which::which("wasm-bindgen").ok())
            .ok_or_else(|| Error::MissingTool {
                version: version.clone(),
            })?;
        let output = Command::new(&executable)
            .arg("--version")
            .output()
            .map_err(|_| Error::MissingTool {
                version: version.clone(),
            })?;
        let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let mut words = actual.split_whitespace();
        let tool_name = words.next();
        let reported_version = words
            .next()
            .and_then(|version| Version::parse(version).ok());
        if !output.status.success()
            || tool_name != Some("wasm-bindgen")
            || reported_version.as_ref() != Some(version)
        {
            return Err(Error::ToolVersion {
                path: executable,
                expected: version.clone(),
                actual,
            }
            .into());
        }
        Ok(Self { executable })
    }

    pub fn process(&self, input: &Path, output: &Path, module_name: &str) -> Result<()> {
        let result = Command::new(&self.executable)
            .arg(input)
            .args([
                "--target",
                "bundler",
                "--no-typescript",
                "--keep-lld-exports",
                "--out-name",
            ])
            .arg(module_name)
            .arg("--out-dir")
            .arg(output)
            .output()
            .map_err(|source| CliError::CommandFailed {
                command: format!("{}: {source}", self.executable.display()),
                status: None,
            })?;
        if !result.status.success() {
            return Err(CliError::CommandFailed {
                command: format!("wasm-bindgen: {}", String::from_utf8_lossy(&result.stderr)),
                status: result.status.code(),
            });
        }
        let unused_loader = output.join(format!("{module_name}.js"));
        fs::remove_file(&unused_loader).map_err(|source| CliError::WriteFailed {
            path: unused_loader,
            source,
        })?;
        Ok(())
    }

    pub fn write_imports(
        module: &Module<'_>,
        directory: &Path,
        module_name: &str,
        runtime_package: &str,
    ) -> Result<()> {
        let glue_module = format!("./{module_name}_bg.js");
        let mut javascript = JavaScriptModules::new(directory)?;
        javascript.validate(&glue_module)?;
        let mut imports = BTreeMap::<_, BTreeSet<_>>::new();
        module
            .imports
            .iter()
            .filter(|import| import.module != "env")
            .for_each(|import| {
                imports
                    .entry(import.module)
                    .or_default()
                    .insert(import.name);
            });
        imports.iter().try_for_each(|(module, names)| {
            if *module != glue_module && !module.starts_with("./snippets/") {
                return Err(Error::Import {
                    module: (*module).to_owned(),
                    name: names.first().expect("an imported module has a name").to_string(),
                    reason: "only packaged local JavaScript modules are supported; npm module imports require a package dependency resolver".to_owned(),
                }.into());
            }
            javascript.validate(module)
        })?;
        let imports = ImportsModule {
            runtime_package,
            glue_module: &glue_module,
            modules: imports,
        }
        .render()
        .map_err(Error::from)?;
        let path = directory.join(format!("{module_name}_imports.ts"));
        fs::write(&path, imports).map_err(|source| CliError::WriteFailed { path, source })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use semver::Version;

    use super::Bindgen;
    use crate::config::Config;

    #[test]
    fn matching_cli_versions_allow_git_build_annotations() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("wasm-bindgen");
        let mut config: Config = toml::from_str("[package]\nname = 'fixture'\n").unwrap();
        config.targets.wasm.wasm_bindgen_cli = Some(executable.clone());
        [
            ("wasm-bindgen 0.2.129", 0, true),
            ("wasm-bindgen 0.2.129 (012345678)", 0, true),
            ("wasm-bindgen 0.2.128 (012345678)", 0, false),
            ("wasm-bindgen 0.2.1290", 0, false),
            ("wasm-bindgen 0.2.129-alpha.1", 0, false),
            ("different-tool 0.2.129", 0, false),
            ("wasm-bindgen invalid", 0, false),
            ("wasm-bindgen 0.2.129", 1, false),
        ]
        .into_iter()
        .for_each(|(version, exit_code, accepted)| {
            fs::write(
                &executable,
                format!("#!/bin/sh\nprintf '%s\\n' '{version}'\nexit {exit_code}\n"),
            )
            .unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
            let result = Bindgen::resolve(&config, &Version::new(0, 2, 129));
            assert_eq!(result.is_ok(), accepted, "{version} with exit {exit_code}");
        });
    }
}
