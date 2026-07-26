use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::cli::{CliError, Result};
use crate::target::{BuiltLibrary, Platform, RustTarget};

use super::Cargo;

#[derive(Debug, Deserialize)]
pub(crate) struct CargoMetadata {
    pub(crate) packages: Vec<CargoMetadataPackage>,
    pub(crate) target_directory: PathBuf,
    #[serde(default)]
    pub(crate) workspace_root: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoMetadataPackage {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) targets: Vec<CargoMetadataPackageTarget>,
    #[serde(default)]
    pub(crate) features: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CargoMetadataPackageTarget {
    pub(crate) name: String,
    pub(crate) crate_types: Vec<CargoCrateType>,
    pub(crate) src_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub(crate) enum CargoCrateType {
    StaticLib,
    Cdylib,
    Dylib,
    Rlib,
    Bin,
    ProcMacro,
    Other(String),
}

impl CargoMetadata {
    pub(crate) fn load(cargo: &Cargo) -> Result<Self> {
        let mut command = Command::new("cargo");
        command.current_dir(cargo.working_directory());
        if let Some(toolchain_selector) = cargo.toolchain_selector() {
            command.arg(toolchain_selector);
        }
        let output = command
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .args(cargo.metadata_arguments())
            .output()
            .map_err(|source| CliError::CommandFailed {
                command: format!("cargo metadata: {source}"),
                status: None,
            })?;

        if !output.status.success() {
            return Err(CliError::CommandFailed {
                command: "cargo metadata --format-version 1 --no-deps".to_string(),
                status: output.status.code(),
            });
        }

        Self::parse(&output.stdout)
    }

    pub(crate) fn find_package(
        &self,
        manifest_path: &Path,
        package_selector: Option<&str>,
    ) -> Result<&CargoMetadataPackage> {
        if let Some(package_selector) = package_selector {
            return self
                .packages
                .iter()
                .find(|package| package.matches_selector(package_selector))
                .ok_or_else(|| CliError::CommandFailed {
                    command: format!(
                        "could not find selected cargo package '{}' in cargo metadata",
                        package_selector
                    ),
                    status: None,
                });
        }

        self.packages
            .iter()
            .find(|package| package.manifest_path == manifest_path)
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "could not find current package manifest '{}' in cargo metadata",
                    manifest_path.display()
                ),
                status: None,
            })
    }

    #[cfg(test)]
    pub(crate) fn target_directory_from_bytes(metadata: &[u8]) -> Result<PathBuf> {
        Ok(Self::parse(metadata)?.target_directory)
    }

    fn parse(metadata: &[u8]) -> Result<Self> {
        serde_json::from_slice::<Self>(metadata).map_err(|source| CliError::CommandFailed {
            command: format!("parse cargo metadata: {source}"),
            status: None,
        })
    }

    pub(crate) fn resolve_built_libraries_for_targets(
        &self,
        cargo_manifest_path: &Path,
        profile_directory_name: &str,
        preferred_artifact_name: &str,
        package_selector: Option<&str>,
        package_targets: &[RustTarget],
    ) -> Result<Vec<BuiltLibrary>> {
        let package = self.find_package(cargo_manifest_path, package_selector)?;
        let artifact_name = package
            .resolve_library_artifact_name(preferred_artifact_name, cargo_manifest_path)?
            .to_string();
        let library_target = package.resolve_library_target(&artifact_name, cargo_manifest_path)?;

        let mut libraries = vec![];

        for t in package_targets {
            for crate_type in &library_target.crate_types {
                let prefix = if crate_type.is_staticlib() {
                    "lib"
                } else {
                    match t.platform() {
                        Platform::Wasm => "",
                        Platform::Android
                        | Platform::Ios
                        | Platform::IosSimulator
                        | Platform::Linux
                        | Platform::MacOs => "lib",
                    }
                };
                let suffix = if crate_type.is_cdylib() {
                    match t.platform() {
                        Platform::Wasm => "wasm",
                        Platform::Android | Platform::Linux => "so",
                        Platform::MacOs | Platform::Ios | Platform::IosSimulator => "dylib",
                    }
                } else if crate_type.is_staticlib() {
                    "a"
                } else {
                    continue;
                };

                let library_filename = format!("{}{}.{}", prefix, artifact_name, suffix);

                let library_filepath = self
                    .target_directory
                    .join(t.triple())
                    .join(profile_directory_name)
                    .join(library_filename);

                libraries.push(BuiltLibrary {
                    target: *t,
                    path: library_filepath,
                })
            }
        }

        Ok(libraries)
    }
}

impl CargoMetadataPackage {
    pub(crate) fn has_target(&self, target_name: &str) -> bool {
        self.targets.iter().any(|target| target.name == target_name)
    }

    pub(crate) fn resolve_library_artifact_name(
        &self,
        preferred_artifact_name: &str,
        manifest_path: &Path,
    ) -> Result<&str> {
        self.resolve_library_target(preferred_artifact_name, manifest_path)
            .map(|target| target.name.as_str())
    }

    pub(crate) fn resolve_library_target(
        &self,
        preferred_artifact_name: &str,
        manifest_path: &Path,
    ) -> Result<&CargoMetadataPackageTarget> {
        self.targets
            .iter()
            .find(|target| target.builds_ffi() && target.name == preferred_artifact_name)
            .map_or_else(|| self.resolve_ffi_library_target(None, manifest_path), Ok)
    }

    pub(crate) fn resolve_ffi_library_target(
        &self,
        preferred_artifact_name: Option<&str>,
        manifest_path: &Path,
    ) -> Result<&CargoMetadataPackageTarget> {
        let ffi_targets = self
            .targets
            .iter()
            .filter(|target| target.builds_ffi())
            .collect::<Vec<_>>();

        match preferred_artifact_name {
            Some(preferred_artifact_name) => {
                if let Some(target) = ffi_targets
                    .iter()
                    .copied()
                    .find(|target| target.name == preferred_artifact_name)
                {
                    return Ok(target);
                }
            }
            None if ffi_targets.len() == 1 => return Ok(ffi_targets[0]),
            None => {}
        }

        Err(CliError::CommandFailed {
            command: format!(
                "could not find library target '{}' in cargo metadata for '{}'",
                preferred_artifact_name.unwrap_or("<unique ffi library>"),
                manifest_path.display()
            ),
            status: None,
        })
    }

    fn matches_selector(&self, package_selector: &str) -> bool {
        self.name == package_selector
            || self.id == package_selector
            || self.matches_package_spec(package_selector)
    }

    fn matches_package_spec(&self, package_selector: &str) -> bool {
        let Some((name, version)) = package_selector.rsplit_once('@') else {
            return false;
        };

        self.name == name && self.version().is_some_and(|value| value == version)
    }

    fn version(&self) -> Option<&str> {
        let fragment = self.id.rsplit('#').next()?;
        let (_, version) = fragment.rsplit_once('@')?;
        Some(version)
    }
}

impl CargoMetadataPackageTarget {
    pub(crate) fn builds_ffi(&self) -> bool {
        self.builds_staticlib() || self.builds_cdylib()
    }

    pub(crate) fn builds_staticlib(&self) -> bool {
        self.crate_types.iter().any(CargoCrateType::is_staticlib)
    }

    pub(crate) fn builds_cdylib(&self) -> bool {
        self.crate_types.iter().any(CargoCrateType::is_cdylib)
    }
}

impl CargoCrateType {
    pub(crate) fn is_staticlib(&self) -> bool {
        matches!(self, Self::StaticLib)
    }

    pub(crate) fn is_cdylib(&self) -> bool {
        matches!(self, Self::Cdylib)
    }
}

impl From<String> for CargoCrateType {
    fn from(crate_type: String) -> Self {
        match crate_type.as_str() {
            "staticlib" => Self::StaticLib,
            "cdylib" => Self::Cdylib,
            "dylib" => Self::Dylib,
            "rlib" => Self::Rlib,
            "bin" => Self::Bin,
            "proc-macro" => Self::ProcMacro,
            _ => Self::Other(crate_type),
        }
    }
}

impl From<&str> for CargoCrateType {
    fn from(crate_type: &str) -> Self {
        Self::from(crate_type.to_string())
    }
}

impl From<CargoCrateType> for String {
    fn from(crate_type: CargoCrateType) -> Self {
        match crate_type {
            CargoCrateType::StaticLib => "staticlib".to_string(),
            CargoCrateType::Cdylib => "cdylib".to_string(),
            CargoCrateType::Dylib => "dylib".to_string(),
            CargoCrateType::Rlib => "rlib".to_string(),
            CargoCrateType::Bin => "bin".to_string(),
            CargoCrateType::ProcMacro => "proc-macro".to_string(),
            CargoCrateType::Other(crate_type) => crate_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::CargoMetadata;
    use crate::{
        cargo::{
            CargoCrateType,
            fixture::{CargoMetadataFixture, CargoPackageFixture, CargoTargetFixture},
        },
        target::RustTarget,
    };

    fn metadata_fixture() -> CargoMetadataFixture {
        CargoMetadataFixture::new("/tmp/boltffi-target")
    }

    #[test]
    fn parses_target_directory_from_cargo_metadata() {
        let target_directory =
            CargoMetadata::target_directory_from_bytes(&metadata_fixture().json_bytes())
                .expect("expected target directory");

        assert_eq!(target_directory, PathBuf::from("/tmp/boltffi-target"));
    }

    #[test]
    fn parses_workspace_root_from_cargo_metadata() {
        let metadata = metadata_fixture()
            .workspace_root("/tmp/workspace")
            .metadata();

        assert_eq!(
            metadata.workspace_root,
            Some(PathBuf::from("/tmp/workspace"))
        );
    }

    #[test]
    fn finds_current_cargo_metadata_package_by_manifest_path() {
        let metadata = metadata_fixture()
            .package(CargoPackageFixture::manifest_package(
                "workspace-a",
                "/tmp/workspace/a/Cargo.toml",
                "0.1.0",
            ))
            .package(CargoPackageFixture::manifest_package(
                "workspace-b",
                "/tmp/workspace/b/Cargo.toml",
                "0.1.0",
            ))
            .metadata();

        let package = metadata
            .find_package(Path::new("/tmp/workspace/b/Cargo.toml"), None)
            .expect("package lookup");

        assert_eq!(package.id, "path+file:///tmp/workspace/b#0.1.0");
    }

    #[test]
    fn finds_selected_cargo_metadata_package_by_package_name() {
        let metadata = metadata_fixture()
            .package(CargoPackageFixture::workspace_package(
                "workspace-a",
                "/tmp/workspace/Cargo.toml",
                "0.1.0",
            ))
            .package(CargoPackageFixture::workspace_package(
                "workspace-b",
                "/tmp/workspace/Cargo.toml",
                "0.1.0",
            ))
            .metadata();

        let package = metadata
            .find_package(Path::new("/tmp/workspace/Cargo.toml"), Some("workspace-b"))
            .expect("package lookup");

        assert_eq!(package.id, "path+file:///tmp/workspace#workspace-b@0.1.0");
    }

    #[test]
    fn finds_selected_cargo_metadata_package_by_package_spec() {
        let metadata = metadata_fixture()
            .package(CargoPackageFixture::workspace_package(
                "workspace-a",
                "/tmp/workspace/Cargo.toml",
                "0.1.0",
            ))
            .package(CargoPackageFixture::workspace_package(
                "workspace-b",
                "/tmp/workspace/Cargo.toml",
                "1.2.3",
            ))
            .metadata();

        let package = metadata
            .find_package(
                Path::new("/tmp/workspace/Cargo.toml"),
                Some("workspace-b@1.2.3"),
            )
            .expect("package lookup");

        assert_eq!(package.id, "path+file:///tmp/workspace#workspace-b@1.2.3");
    }

    #[test]
    fn explicit_ffi_artifact_selection_never_falls_back_to_the_only_target() {
        let manifest_path = Path::new("/tmp/workspace/Cargo.toml");
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package("workspace", manifest_path, "0.1.0").target(
                    CargoTargetFixture::library("actual_artifact", [CargoCrateType::StaticLib]),
                ),
            )
            .metadata();
        let package = metadata
            .find_package(manifest_path, None)
            .expect("package lookup");

        assert!(
            package
                .resolve_ffi_library_target(Some("configured_artifact"), manifest_path)
                .is_err()
        );
        assert_eq!(
            package
                .resolve_ffi_library_target(None, manifest_path)
                .expect("unique target inference")
                .name,
            "actual_artifact"
        );
    }

    #[test]
    fn metadata_resolves_built_libraries_for_targets() {
        let metadata = metadata_fixture()
            .package(
                CargoPackageFixture::manifest_package(
                    "my-workspace",
                    "/tmp/my-workspace/Cargo.toml",
                    "0.1.0",
                )
                .target(CargoTargetFixture::library(
                    "my_workspace",
                    [
                        CargoCrateType::Cdylib,
                        CargoCrateType::StaticLib,
                        CargoCrateType::Bin,
                    ],
                )),
            )
            .metadata();

        let targets = [
            RustTarget::IOS_ARM64,
            RustTarget::WASM32_UNKNOWN_UNKNOWN,
            RustTarget::LINUX_X86_64,
        ];
        let expected_library_paths_for_targets = [
            [
                format!(
                    "/tmp/boltffi-target/{}/release/libmy_workspace.dylib",
                    targets[0].triple()
                ),
                format!(
                    "/tmp/boltffi-target/{}/release/libmy_workspace.a",
                    targets[0].triple()
                ),
            ],
            [
                format!(
                    "/tmp/boltffi-target/{}/release/my_workspace.wasm",
                    targets[1].triple()
                ),
                format!(
                    "/tmp/boltffi-target/{}/release/libmy_workspace.a",
                    targets[1].triple()
                ),
            ],
            [
                format!(
                    "/tmp/boltffi-target/{}/release/libmy_workspace.so",
                    targets[2].triple()
                ),
                format!(
                    "/tmp/boltffi-target/{}/release/libmy_workspace.a",
                    targets[2].triple()
                ),
            ],
        ];

        let libraries = metadata
            .resolve_built_libraries_for_targets(
                Path::new("/tmp/my-workspace/Cargo.toml"),
                "release",
                "my_workspace",
                None,
                &targets,
            )
            .unwrap();

        let matches_any_expected_path = |i: usize, p: &Path| {
            expected_library_paths_for_targets[i]
                .iter()
                .any(|ex| ex == p)
        };

        for l in libraries {
            for (i, target) in targets.into_iter().enumerate() {
                if l.target == target {
                    assert!(matches_any_expected_path(i, &l.path));
                }
            }
        }
    }
}
