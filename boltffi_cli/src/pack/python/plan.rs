use std::collections::HashSet;
use std::path::{Component, PathBuf};

use boltffi_bindgen::metadata::{ResolvedFeatures, resolve_cargo_features};

use crate::build::{CargoBuildProfile, resolve_build_profile};
use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::pack::resolve_build_cargo_args;
use crate::target::NativeHostPlatform;

use super::layout::PythonPackageLayout;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PythonInterpreterSelection {
    command: String,
}

impl PythonInterpreterSelection {
    pub fn new(command: impl Into<String>) -> Result<Self> {
        let command = command.into();
        let normalized_command = command.trim();

        if normalized_command.is_empty() {
            return Err(CliError::CommandFailed {
                command: "python interpreter selection must not be empty".to_string(),
                status: None,
            });
        }

        Ok(Self {
            command: normalized_command.to_string(),
        })
    }

    pub fn command(&self) -> &str {
        &self.command
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonCargoContext {
    pub release: bool,
    pub build_profile: CargoBuildProfile,
    pub artifact_name: String,
    pub cargo_manifest_path: PathBuf,
    pub manifest_path: PathBuf,
    pub library_source_path: PathBuf,
    pub package_selector: Option<String>,
    pub target_directory: PathBuf,
    pub cargo_command_args: Vec<String>,
    pub toolchain_selector: Option<String>,
    pub features: ResolvedFeatures,
}

impl PythonCargoContext {
    pub fn artifact_directory(&self) -> PathBuf {
        self.target_directory
            .join(self.build_profile.output_directory_name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonPackagingPlan {
    pub distribution_name: String,
    pub module_name: String,
    pub package_version: Option<String>,
    pub host_platform: NativeHostPlatform,
    pub interpreters: Vec<PythonInterpreterSelection>,
    pub layout: PythonPackageLayout,
    pub cargo_context: PythonCargoContext,
}

impl PythonPackagingPlan {
    pub fn from_config(
        config: &Config,
        release: bool,
        cli_cargo_args: &[String],
        cli_python_interpreters: &[String],
    ) -> Result<Self> {
        let host_platform = NativeHostPlatform::current().ok_or_else(|| CliError::CommandFailed {
            command:
                "python packaging is only supported on darwin-arm64, darwin-x86_64, linux-x86_64, linux-aarch64, and windows-x86_64 hosts".to_string(),
            status: None,
        })?;
        let module_name = config.python_module_name();
        let output_root = absolutize_configured_path(config.python_output())?;
        let wheel_directory = absolutize_configured_path(config.python_wheel_output())?;
        let layout = if wheel_directory == output_root.join("wheelhouse") {
            PythonPackageLayout::new(output_root, &module_name)
        } else {
            PythonPackageLayout::with_wheel_directory(output_root, wheel_directory, &module_name)
        };
        layout.validate_wheel_directory_safety()?;
        let build_cargo_args = resolve_build_cargo_args(config, cli_cargo_args);
        let build_profile = resolve_build_profile(release, &build_cargo_args);
        let cargo = Cargo::current(&build_cargo_args)?;

        if let Some(target_selector) = cargo.target_selector() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "pack python targets the current host interpreter; remove cargo --target '{}'",
                    target_selector
                ),
                status: None,
            });
        }

        if let Some(configured_build_target) = cargo.configured_build_target() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "pack python targets the current host interpreter; remove cargo build.target '{}'",
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

        if !library_target.builds_cdylib() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "python packaging requires a cdylib target for '{}'",
                    cargo_manifest_path.display()
                ),
                status: None,
            });
        }
        let cargo_command_args = cargo.probe_command_arguments();
        let features = resolve_cargo_features(
            &package.manifest_path,
            cargo_command_args.clone(),
            cargo.toolchain_selector(),
        )
        .map_err(|error| CliError::CommandFailed {
            command: format!("resolve active Cargo features: {error}"),
            status: None,
        })?;

        Ok(Self {
            distribution_name: config.package.name.clone(),
            module_name,
            package_version: config.package_version(),
            host_platform,
            interpreters: Self::resolve_interpreters(config, cli_python_interpreters)?,
            layout,
            cargo_context: PythonCargoContext {
                release,
                build_profile,
                artifact_name: library_target.name.clone(),
                cargo_manifest_path,
                manifest_path: package.manifest_path.clone(),
                library_source_path: library_target.src_path.clone(),
                package_selector,
                target_directory: metadata.target_directory,
                cargo_command_args,
                toolchain_selector: cargo.toolchain_selector().map(str::to_owned),
                features,
            },
        })
    }

    pub fn built_shared_library_path(&self) -> PathBuf {
        self.cargo_context.artifact_directory().join(
            self.host_platform
                .shared_library_filename(&self.cargo_context.artifact_name),
        )
    }

    pub fn packaged_shared_library_path(&self) -> PathBuf {
        self.layout
            .packaged_shared_library_path(self.host_platform, &self.cargo_context.artifact_name)
    }

    fn resolve_interpreters(
        config: &Config,
        cli_python_interpreters: &[String],
    ) -> Result<Vec<PythonInterpreterSelection>> {
        let configured_commands = (!cli_python_interpreters.is_empty())
            .then_some(cli_python_interpreters)
            .or_else(|| config.python_wheel_interpreters())
            .unwrap_or(&[]);

        if configured_commands.is_empty() {
            return Ok(Vec::new());
        }

        let mut seen_commands = HashSet::new();

        configured_commands
            .iter()
            .map(|command| PythonInterpreterSelection::new(command.clone()))
            .filter_map(|selection_result| match selection_result {
                Ok(selection) => seen_commands
                    .insert(selection.command().to_string())
                    .then_some(Ok(selection)),
                Err(error) => Some(Err(error)),
            })
            .collect()
    }
}

fn absolutize_configured_path(path: PathBuf) -> Result<PathBuf> {
    let absolute_path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|current_directory| current_directory.join(path))
            .map_err(|source| CliError::CommandFailed {
                command: format!("current_dir: {source}"),
                status: None,
            })?
    };

    Ok(normalize_path(absolute_path))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.components()
        .fold(PathBuf::new(), |mut normalized_path, component| {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if normalized_path.file_name().is_some() {
                        normalized_path.pop();
                    } else if !normalized_path.has_root() {
                        normalized_path.push(component.as_os_str());
                    }
                }
                _ => normalized_path.push(component.as_os_str()),
            }

            normalized_path
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use boltffi_bindgen::metadata::ResolvedFeatures;

    use super::{
        PythonCargoContext, PythonInterpreterSelection, PythonPackageLayout, PythonPackagingPlan,
    };
    use crate::build::CargoBuildProfile;
    use crate::cli::CliError;
    use crate::config::{
        CargoConfig, Config, PackageConfig, PythonConfig, PythonWheelConfig, TargetsConfig,
    };
    use crate::target::NativeHostPlatform;

    fn config() -> Config {
        Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "demo-package".to_string(),
                crate_name: Some("demo-ffi".to_string()),
                version: Some("0.1.0".to_string()),
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                python: PythonConfig {
                    output: PathBuf::from("dist/python"),
                    module_name: Some("demo_module".to_string()),
                    wheel: PythonWheelConfig {
                        output: Some(PathBuf::from("dist/wheels")),
                        interpreters: Some(vec![
                            "python3.12".to_string(),
                            "python3.13".to_string(),
                        ]),
                    },
                    enabled: true,
                },
                ..TargetsConfig::default()
            },
        }
    }

    #[test]
    fn rejects_explicit_cargo_target_for_python_packaging() {
        let error = PythonPackagingPlan::from_config(
            &config(),
            false,
            &["--target".to_string(), "aarch64-apple-darwin".to_string()],
            &[],
        )
        .expect_err("expected explicit cargo target rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("remove cargo --target 'aarch64-apple-darwin'")
        ));
    }

    #[test]
    fn rejects_configured_cargo_build_target_for_python_packaging() {
        let error = PythonPackagingPlan::from_config(
            &config(),
            false,
            &["--config=build.target='aarch64-apple-darwin'".to_string()],
            &[],
        )
        .expect_err("expected configured cargo build target rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("remove cargo build.target 'aarch64-apple-darwin'")
        ));
    }

    #[test]
    fn resolves_built_and_packaged_shared_library_paths() {
        let host_platform = NativeHostPlatform::current().expect("supported current host");
        let plan = PythonPackagingPlan {
            distribution_name: "demo-package".to_string(),
            module_name: "demo_ffi".to_string(),
            package_version: Some("0.1.0".to_string()),
            host_platform,
            interpreters: vec![
                PythonInterpreterSelection::new("python3.13")
                    .expect("python interpreter selection"),
            ],
            layout: PythonPackageLayout::new("dist/python", "demo_ffi"),
            cargo_context: PythonCargoContext {
                release: true,
                build_profile: CargoBuildProfile::Release,
                artifact_name: "demo_ffi".to_string(),
                cargo_manifest_path: PathBuf::from("/tmp/workspace/Cargo.toml"),
                manifest_path: PathBuf::from("/tmp/workspace/member/Cargo.toml"),
                library_source_path: PathBuf::from("/tmp/workspace/member/src/lib.rs"),
                package_selector: Some("workspace-member".to_string()),
                target_directory: PathBuf::from("/tmp/boltffi-target"),
                cargo_command_args: Vec::new(),
                toolchain_selector: None,
                features: ResolvedFeatures::default(),
            },
        };

        assert_eq!(
            plan.built_shared_library_path(),
            PathBuf::from("/tmp/boltffi-target/release")
                .join(host_platform.shared_library_filename("demo_ffi"))
        );
        assert_eq!(
            plan.packaged_shared_library_path(),
            PathBuf::from("dist/python/demo_ffi")
                .join(host_platform.shared_library_filename("demo_ffi"))
        );
    }

    #[test]
    fn retains_selected_package_manifest_for_python_generation() {
        let host_platform = NativeHostPlatform::current().expect("supported current host");
        let plan = PythonPackagingPlan {
            distribution_name: "demo-package".to_string(),
            module_name: "demo_ffi".to_string(),
            package_version: Some("0.1.0".to_string()),
            host_platform,
            interpreters: vec![],
            layout: PythonPackageLayout::new("dist/python", "demo_ffi"),
            cargo_context: PythonCargoContext {
                release: false,
                build_profile: CargoBuildProfile::Debug,
                artifact_name: "workspace_member_ffi".to_string(),
                cargo_manifest_path: PathBuf::from("/tmp/workspace/Cargo.toml"),
                manifest_path: PathBuf::from("/tmp/workspace/member/Cargo.toml"),
                library_source_path: PathBuf::from("/tmp/workspace/member/src/lib.rs"),
                package_selector: Some("workspace-member".to_string()),
                target_directory: PathBuf::from("/tmp/boltffi-target"),
                cargo_command_args: Vec::new(),
                toolchain_selector: None,
                features: ResolvedFeatures::default(),
            },
        };

        assert_eq!(
            plan.cargo_context.manifest_path,
            PathBuf::from("/tmp/workspace/member/Cargo.toml")
        );
    }

    #[test]
    fn prefers_cli_python_interpreters_over_configured_packaging_matrix() {
        let config = config();
        let interpreters = PythonPackagingPlan::resolve_interpreters(
            &config,
            &["python3.11".to_string(), "python3.12".to_string()],
        )
        .expect("python interpreter resolution");

        assert_eq!(
            interpreters
                .iter()
                .map(|interpreter| interpreter.command())
                .collect::<Vec<_>>(),
            vec!["python3.11", "python3.12"]
        );
        assert_eq!(config.python_module_name(), "demo_module");
        assert_eq!(config.python_wheel_output(), PathBuf::from("dist/wheels"));
    }

    #[test]
    fn rejects_wheel_directory_that_overlaps_python_output_after_normalization() {
        let mut config = config();
        config.targets.python.wheel.output = Some(PathBuf::from("dist/python/../python"));

        let error = PythonPackagingPlan::from_config(&config, false, &[], &[])
            .expect_err("expected unsafe wheel output rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("targets.python.wheel.output")
        ));
    }
}
