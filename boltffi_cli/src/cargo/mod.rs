mod args;
pub(crate) mod config;
#[cfg(test)]
pub(crate) mod fixture;
mod metadata;

use std::path::{Path, PathBuf};

use crate::cli::{CliError, Result};
use crate::config::Config;

use self::args::CargoArguments;

#[cfg(test)]
pub(crate) use self::metadata::CargoCrateType;
pub(crate) use self::metadata::CargoMetadata;

#[derive(Debug, Clone)]
pub(crate) struct Cargo {
    working_directory: PathBuf,
    arguments: CargoArguments,
}

impl Cargo {
    pub(crate) fn current(cargo_args: &[String]) -> Result<Self> {
        std::env::current_dir()
            .map(|working_directory| Self::in_working_directory(working_directory, cargo_args))
            .map_err(|source| CliError::CommandFailed {
                command: format!("current_dir: {source}"),
                status: None,
            })
    }

    pub(crate) fn in_working_directory(working_directory: PathBuf, cargo_args: &[String]) -> Self {
        Self {
            working_directory,
            arguments: CargoArguments::new(cargo_args),
        }
    }

    pub(crate) fn metadata(&self) -> Result<CargoMetadata> {
        CargoMetadata::load(self)
    }

    pub(crate) fn manifest_path(&self) -> Result<PathBuf> {
        self.arguments.manifest_path(&self.working_directory)
    }

    pub(crate) fn toolchain_selector(&self) -> Option<&str> {
        self.arguments.toolchain_selector()
    }

    pub(crate) fn package_selector(&self) -> Option<&str> {
        self.arguments.package_selector()
    }

    pub(crate) fn target_selector(&self) -> Option<&str> {
        self.arguments.target_selector()
    }

    pub(crate) fn probe_command_arguments(&self) -> Vec<String> {
        self.arguments.probe_command_arguments()
    }

    pub(crate) fn configured_build_target(&self) -> Option<String> {
        self.arguments
            .configured_build_target(&self.working_directory)
    }

    pub(crate) fn effective_package_selector(
        &self,
        config: &Config,
        metadata: &CargoMetadata,
        manifest_path: &Path,
    ) -> Option<String> {
        if let Some(package_selector) = self.package_selector() {
            return Some(package_selector.to_string());
        }

        let manifest_selects_package = self.arguments.has_explicit_manifest_path()
            && metadata
                .packages
                .iter()
                .any(|package| package.manifest_path == manifest_path);

        (!manifest_selects_package)
            .then(|| self.infer_package_selector(config, metadata, manifest_path))
            .flatten()
    }

    pub(super) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(super) fn metadata_arguments(&self) -> Vec<String> {
        self.arguments.metadata_arguments()
    }

    #[cfg(test)]
    pub(crate) fn command_arguments(&self) -> &[String] {
        self.arguments.command_arguments()
    }

    #[cfg(test)]
    pub(crate) fn metadata_passthrough_arguments(&self) -> Vec<String> {
        self.arguments.metadata_arguments()
    }

    #[cfg(test)]
    pub(crate) fn command_arguments_without_package_selector(&self) -> Vec<String> {
        self.arguments.command_arguments_without_package_selector()
    }

    #[cfg(test)]
    pub(crate) fn command_arguments_without_manifest_path_selector(&self) -> Vec<String> {
        self.arguments
            .command_arguments_without_manifest_path_selector()
    }

    pub(crate) fn command_arguments_without_target_selector(&self) -> Vec<String> {
        self.arguments.command_arguments_without_target_selector()
    }

    fn infer_package_selector(
        &self,
        config: &Config,
        metadata: &CargoMetadata,
        manifest_path: &Path,
    ) -> Option<String> {
        metadata
            .packages
            .iter()
            .find(|package| package.manifest_path == manifest_path)
            .map(|package| package.name.clone())
            .or_else(|| {
                let mut matching_packages = metadata
                    .packages
                    .iter()
                    .filter(|package| package.name == config.package.name);
                let package = matching_packages.next()?;
                matching_packages
                    .next()
                    .is_none()
                    .then(|| package.name.clone())
            })
            .or_else(|| {
                let crate_artifact_name = config.crate_artifact_name();
                let mut matching_packages = metadata
                    .packages
                    .iter()
                    .filter(|package| package.has_target(&crate_artifact_name));
                let package = matching_packages.next()?;
                matching_packages
                    .next()
                    .is_none()
                    .then(|| package.name.clone())
            })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Cargo;
    use crate::cargo::fixture::{CargoMetadataFixture, CargoPackageFixture, CargoTargetFixture};
    use crate::cargo::metadata::CargoCrateType;
    use crate::config::{CargoConfig, Config, PackageConfig, TargetsConfig};

    fn cargo(arguments: &[&str]) -> Cargo {
        Cargo::in_working_directory(
            std::env::current_dir().unwrap_or_default(),
            &arguments
                .iter()
                .map(|argument| argument.to_string())
                .collect::<Vec<_>>(),
        )
    }

    fn config(crate_name: Option<&str>) -> Config {
        Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "workspace-member".to_string(),
                crate_name: crate_name.map(str::to_string),
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig::default(),
        }
    }

    fn metadata_fixture() -> CargoMetadataFixture {
        CargoMetadataFixture::new("/tmp/boltffi-target")
    }

    #[test]
    fn splits_toolchain_selector_from_cargo_args() {
        let cargo = cargo(&["--features", "demo", "+nightly", "--locked"]);

        assert_eq!(cargo.toolchain_selector(), Some("+nightly"));
        assert_eq!(
            cargo.command_arguments(),
            vec![
                "--features".to_string(),
                "demo".to_string(),
                "--locked".to_string()
            ]
        );
    }

    #[test]
    fn keeps_metadata_relevant_cargo_args() {
        let cargo = cargo(&[
            "+nightly",
            "--target-dir",
            "out/target",
            "--config=build.target-dir=\"other-target\"",
            "--locked",
            "--features",
            "demo",
            "--manifest-path",
            "examples/demo/Cargo.toml",
            "-Zunstable-options",
        ]);
        let metadata_args = cargo.metadata_passthrough_arguments();

        assert_eq!(cargo.toolchain_selector(), Some("+nightly"));
        assert_eq!(
            metadata_args,
            vec![
                "--target-dir".to_string(),
                "out/target".to_string(),
                "--config=build.target-dir=\"other-target\"".to_string(),
                "--locked".to_string(),
                "--manifest-path".to_string(),
                "examples/demo/Cargo.toml".to_string(),
                "-Zunstable-options".to_string(),
            ]
        );
    }

    #[test]
    fn canonicalizes_manifest_path_from_split_cargo_args() {
        let expected = std::env::current_dir()
            .expect("current dir")
            .join("Cargo.toml")
            .canonicalize()
            .expect("canonical manifest path");

        let manifest_path = cargo(&["--manifest-path", "Cargo.toml"])
            .manifest_path()
            .expect("manifest path");

        assert_eq!(manifest_path, expected);
    }

    #[test]
    fn canonicalizes_manifest_path_from_equals_cargo_arg() {
        let expected = std::env::current_dir()
            .expect("current dir")
            .join("Cargo.toml")
            .canonicalize()
            .expect("canonical manifest path");

        let manifest_path = cargo(&["--manifest-path=Cargo.toml"])
            .manifest_path()
            .expect("manifest path");

        assert_eq!(manifest_path, expected);
    }

    #[test]
    fn canonicalizes_implicit_manifest_path() {
        let expected = std::env::current_dir()
            .expect("current dir")
            .join("Cargo.toml")
            .canonicalize()
            .expect("canonical manifest path");

        let manifest_path = cargo(&[]).manifest_path().expect("manifest path");

        assert_eq!(manifest_path, expected);
    }

    #[test]
    fn extracts_last_package_selector_from_cargo_args() {
        let package_selector = cargo(&[
            "--manifest-path",
            "Cargo.toml",
            "-p",
            "first",
            "--package=second",
        ])
        .package_selector()
        .map(str::to_owned);

        assert_eq!(package_selector.as_deref(), Some("second"));
    }

    #[test]
    fn extracts_package_spec_selector_from_cargo_args() {
        let package_selector = cargo(&["--locked", "-p", "workspace-member@1.2.3"])
            .package_selector()
            .map(str::to_owned);

        assert_eq!(package_selector.as_deref(), Some("workspace-member@1.2.3"));
    }

    #[test]
    fn extracts_last_target_selector_from_cargo_args() {
        let target_selector = cargo(&[
            "--target",
            "aarch64-apple-darwin",
            "--target=x86_64-unknown-linux-gnu",
        ])
        .target_selector()
        .map(str::to_owned);

        assert_eq!(target_selector.as_deref(), Some("x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn strips_package_selectors_from_probe_cargo_args() {
        let cargo_args = cargo(&[
            "+nightly",
            "--package",
            "member-a",
            "-pmember-b",
            "-p",
            "member-c@1.2.3",
            "--features",
            "demo",
            "--package=member-d",
            "--release",
        ])
        .command_arguments_without_package_selector();

        assert_eq!(
            cargo_args,
            vec![
                "+nightly".to_string(),
                "--features".to_string(),
                "demo".to_string(),
                "--release".to_string(),
            ]
        );
    }

    #[test]
    fn strips_manifest_path_from_probe_cargo_args() {
        let cargo_args = cargo(&[
            "--locked",
            "--manifest-path",
            "workspace/Cargo.toml",
            "--manifest-path=member/Cargo.toml",
            "--frozen",
        ])
        .command_arguments_without_manifest_path_selector();

        assert_eq!(
            cargo_args,
            vec!["--locked".to_string(), "--frozen".to_string()]
        );
    }

    #[test]
    fn strips_target_selectors_from_probe_cargo_args() {
        let cargo_args = cargo(&[
            "+nightly",
            "--target",
            "aarch64-apple-darwin",
            "--features",
            "demo",
            "--target=x86_64-unknown-linux-gnu",
            "--release",
        ])
        .command_arguments_without_target_selector();

        assert_eq!(
            cargo_args,
            vec![
                "+nightly".to_string(),
                "--features".to_string(),
                "demo".to_string(),
                "--release".to_string(),
            ]
        );
    }

    #[test]
    fn probe_command_arguments_strip_rustup_toolchain_selectors() {
        let cargo_args = cargo(&[
            "--package",
            "member-a",
            "+nightly",
            "--manifest-path",
            "workspace/Cargo.toml",
            "--target",
            "aarch64-apple-darwin",
            "--features",
            "demo",
            "--release",
        ])
        .probe_command_arguments();

        assert_eq!(
            cargo_args,
            vec![
                "--features".to_string(),
                "demo".to_string(),
                "--release".to_string(),
            ]
        );
    }

    #[test]
    fn falls_back_to_current_manifest_package_for_effective_package_selector() {
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package(
                    "workspace-member",
                    "/tmp/workspace/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "workspace_member",
                    [CargoCrateType::Cdylib],
                )),
            )
            .metadata();

        let package_selector = cargo(&[]).effective_package_selector(
            &config(None),
            &metadata,
            Path::new("/tmp/workspace/Cargo.toml"),
        );

        assert_eq!(package_selector.as_deref(), Some("workspace-member"));
    }

    #[test]
    fn falls_back_to_cargo_package_name_when_crate_name_differs() {
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package(
                    "workspace-member",
                    "/tmp/workspace/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "ffi_member",
                    [CargoCrateType::Cdylib],
                )),
            )
            .metadata();

        let package_selector = cargo(&[]).effective_package_selector(
            &config(Some("ffi_member")),
            &metadata,
            Path::new("/tmp/workspace/Cargo.toml"),
        );

        assert_eq!(package_selector.as_deref(), Some("workspace-member"));
    }

    #[test]
    fn returns_none_for_effective_package_selector_when_manifest_path_selects_package() {
        let metadata = metadata_fixture()
            .package(CargoPackageFixture::manifest_package(
                "workspace-member",
                "/tmp/workspace/member/Cargo.toml",
                "0.1.0",
            ))
            .metadata();

        let package_selector = cargo(&["--manifest-path", "member/Cargo.toml"])
            .effective_package_selector(
                &config(None),
                &metadata,
                Path::new("/tmp/workspace/member/Cargo.toml"),
            );

        assert_eq!(package_selector, None);
    }

    #[test]
    fn falls_back_to_package_name_for_virtual_workspace_manifest_path() {
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package(
                    "workspace-member",
                    "/tmp/workspace/member/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "workspace_member",
                    [CargoCrateType::Cdylib],
                )),
            )
            .metadata();

        let package_selector = cargo(&["--manifest-path", "/tmp/workspace/Cargo.toml"])
            .effective_package_selector(
                &config(None),
                &metadata,
                Path::new("/tmp/workspace/Cargo.toml"),
            );

        assert_eq!(package_selector.as_deref(), Some("workspace-member"));
    }

    #[test]
    fn falls_back_to_package_name_when_crate_name_differs_for_virtual_workspace_manifest_path() {
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package(
                    "workspace-member",
                    "/tmp/workspace/member/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "ffi_member",
                    [CargoCrateType::Cdylib],
                )),
            )
            .metadata();

        let package_selector = cargo(&["--manifest-path", "/tmp/workspace/Cargo.toml"])
            .effective_package_selector(
                &config(Some("ffi_member")),
                &metadata,
                Path::new("/tmp/workspace/Cargo.toml"),
            );

        assert_eq!(package_selector.as_deref(), Some("workspace-member"));
    }

    #[test]
    fn prefers_explicit_package_selector_over_config_package_name() {
        let package_selector = cargo(&["--package=selected-member"]).effective_package_selector(
            &config(None),
            &metadata_fixture().metadata(),
            Path::new("/tmp/workspace/Cargo.toml"),
        );

        assert_eq!(package_selector.as_deref(), Some("selected-member"));
    }

    #[test]
    fn prefers_configured_package_name_over_unique_library_target_match() {
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package(
                    "other-member",
                    "/tmp/workspace/other/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "ffi_member",
                    [CargoCrateType::Cdylib],
                )),
            )
            .package(
                CargoPackageFixture::manifest_package(
                    "workspace-member",
                    "/tmp/workspace/member/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "workspace_member_lib",
                    [CargoCrateType::Cdylib],
                )),
            )
            .metadata();

        let package_selector = cargo(&["--manifest-path", "/tmp/workspace/Cargo.toml"])
            .effective_package_selector(
                &config(Some("ffi_member")),
                &metadata,
                Path::new("/tmp/workspace/Cargo.toml"),
            );

        assert_eq!(package_selector.as_deref(), Some("workspace-member"));
    }
}
