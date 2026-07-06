use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BindingMetadataSurface,
};

use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::config::Config;

const BINDING_EXPANSION_CFG: &str = "boltffi_binding_expansion";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingExpansion {
    manifest_path: PathBuf,
    source_path: PathBuf,
    cargo_args: Vec<String>,
}

impl BindingExpansion {
    pub fn resolve(config: &Config, build_cargo_args: &[String]) -> Result<Self> {
        let cargo = Cargo::current(build_cargo_args)?;
        let metadata = cargo.metadata()?;
        let cargo_manifest_path = cargo.manifest_path()?;
        let package_selector =
            cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
        let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
        let library_target =
            package.resolve_library_target(&config.crate_artifact_name(), &cargo_manifest_path)?;

        Ok(Self {
            manifest_path: package.manifest_path.clone(),
            source_path: library_target.src_path.clone(),
            cargo_args: cargo.probe_command_arguments(),
        })
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.manifest_path.clone()
    }

    pub fn cargo_args(&self) -> Vec<String> {
        self.cargo_args.clone()
    }

    pub fn env(&self) -> Result<Vec<(OsString, OsString)>> {
        let root = self
            .manifest_path
            .parent()
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "manifest path '{}' has no parent directory",
                    self.manifest_path.display()
                ),
                status: None,
            })?;

        Ok(vec![
            (BINDING_EXPANSION_BUILD_ENV.into(), "1".into()),
            (
                BINDING_EXPANSION_ROOT_ENV.into(),
                root.as_os_str().to_owned(),
            ),
            (
                BINDING_EXPANSION_SOURCE_ENV.into(),
                self.source_path.as_os_str().to_owned(),
            ),
            (
                BINDING_EXPANSION_SURFACE_ENV.into(),
                BindingMetadataSurface::Native.as_str().into(),
            ),
            ExpansionRustflags::from_env().into_env(),
        ])
    }
}

enum ExpansionRustflags {
    Encoded(OsString),
    Plain(OsString),
}

impl ExpansionRustflags {
    fn from_env() -> Self {
        std::env::var_os("CARGO_ENCODED_RUSTFLAGS")
            .map(Self::encoded)
            .unwrap_or_else(|| Self::plain(std::env::var_os("RUSTFLAGS")))
    }

    fn encoded(existing: OsString) -> Self {
        Self::Encoded(Self::append_encoded(existing))
    }

    fn plain(existing: Option<OsString>) -> Self {
        Self::Plain(match existing.filter(|value| !value.is_empty()) {
            Some(mut value) => {
                value.push(" --cfg ");
                value.push(BINDING_EXPANSION_CFG);
                value
            }
            None => OsString::from(format!("--cfg {BINDING_EXPANSION_CFG}")),
        })
    }

    fn append_encoded(mut existing: OsString) -> OsString {
        if !existing.is_empty() {
            existing.push(OsStr::new("\u{1f}"));
        }
        existing.push(OsStr::new("--cfg"));
        existing.push(OsStr::new("\u{1f}"));
        existing.push(OsStr::new(BINDING_EXPANSION_CFG));
        existing
    }

    fn into_env(self) -> (OsString, OsString) {
        match self {
            Self::Encoded(value) => ("CARGO_ENCODED_RUSTFLAGS".into(), value),
            Self::Plain(value) => ("RUSTFLAGS".into(), value),
        }
    }
}
