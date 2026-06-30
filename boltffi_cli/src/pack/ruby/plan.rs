use std::path::PathBuf;

use crate::build::resolve_build_profile;
use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::commands::pack::PackRubyOptions;
use crate::config::Config;
use crate::config::targets::ruby::RubyGemPlatform;
use crate::pack::resolve_build_cargo_args;

use super::platform::RubyPackTarget;

#[derive(Debug, Clone)]
pub struct RubyPackagingPlan {
    pub targets: Vec<RubyPackTarget>,
    pub source_directory: PathBuf,
    pub crate_name: String,
    pub gem_name: String,
    pub version: String,
    pub cargo_manifest_path: PathBuf,
    pub library_source_path: PathBuf,
    pub package_selector: Option<String>,
    pub cargo_zigbuild: Option<String>,
    pub release: bool,
    pub cargo_command_args: Vec<String>,
    pub toolchain_selector: Option<String>,
}

impl RubyPackagingPlan {
    pub fn from_config(config: &Config, options: &PackRubyOptions) -> Result<Self> {
        let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
        let _build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);
        let cargo = Cargo::current(&build_cargo_args)?;

        if let Some(target_selector) = cargo.target_selector() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "pack ruby controls cargo-zigbuild targets; remove cargo --target '{}'",
                    target_selector
                ),
                status: None,
            });
        }

        if let Some(configured_build_target) = cargo.configured_build_target() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "pack ruby controls cargo-zigbuild targets; remove cargo build.target '{}'",
                    configured_build_target
                ),
                status: None,
            });
        }

        let metadata = cargo.metadata()?;
        let cargo_manifest_path = cargo.manifest_path()?;
        let package_selector =
            cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
        let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
        let library_target =
            package.resolve_library_target(&config.crate_artifact_name(), &cargo_manifest_path)?;

        if !library_target.builds_staticlib() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "pack ruby uses cargo-zigbuild and requires [lib] crate-type for '{}' to include \"staticlib\"",
                    package.manifest_path.display()
                ),
                status: None,
            });
        }

        let source_directory = package
            .manifest_path
            .parent()
            .ok_or_else(|| CliError::CommandFailed {
                command:
                    "could not resolve selected Cargo package source directory for Ruby packaging"
                        .to_string(),
                status: None,
            })?
            .to_path_buf();
        let crate_name = library_target.name.clone();
        let gem_name = config
            .ruby_gem_name()
            .unwrap_or_else(|| config.library_name().replace('_', "-"));
        let version = config
            .package
            .version
            .clone()
            .unwrap_or_else(|| package.version.clone());
        let glibc_version = config.ruby_pack_glibc_version().map(String::from);
        let cargo_zigbuild = config.ruby_pack_cargo_zigbuild().map(String::from);

        let platforms = if !options.targets.is_empty() {
            let mut resolved = Vec::new();
            for target in &options.targets {
                let platform: RubyGemPlatform =
                    target
                        .parse()
                        .map_err(|error: String| CliError::CommandFailed {
                            command: format!("invalid Ruby pack target: {error}"),
                            status: None,
                        })?;
                resolved.push(platform);
            }
            resolved
        } else if let Some(targets) = config.ruby_pack_targets() {
            targets.to_vec()
        } else {
            vec![RubyGemPlatform::Current]
        };

        let targets = platforms
            .into_iter()
            .map(|platform| RubyPackTarget::from_platform(platform, glibc_version.as_deref()))
            .collect();

        Ok(Self {
            targets,
            source_directory,
            crate_name,
            gem_name,
            version,
            cargo_manifest_path,
            library_source_path: library_target.src_path.clone(),
            package_selector,
            cargo_zigbuild,
            release: options.execution.release,
            cargo_command_args: cargo.probe_command_arguments(),
            toolchain_selector: cargo.toolchain_selector().map(str::to_string),
        })
    }
}
