use std::path::{Path, PathBuf};

use crate::cli::Result;
use crate::config::Config;

use super::metadata::{CargoMetadataPackage, CargoMetadataPackageTarget};
use super::{Cargo, CargoMetadata};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedLibrary {
    package_id: String,
    package_name: String,
    cargo_manifest_path: PathBuf,
    package_manifest_path: PathBuf,
    source_path: PathBuf,
    artifact_name: String,
    builds_staticlib: bool,
    builds_cdylib: bool,
}

impl SelectedLibrary {
    pub fn resolve(
        config: &Config,
        cargo: &Cargo,
        metadata: &CargoMetadata,
        cargo_manifest_path: &Path,
        preferred_artifact: Option<&str>,
    ) -> Result<Self> {
        let package_selector = cargo.effective_package_selector_with_artifact(
            config,
            metadata,
            cargo_manifest_path,
            preferred_artifact,
        );
        let package = metadata.find_package(cargo_manifest_path, package_selector.as_deref())?;
        let target = package.resolve_ffi_library_target(preferred_artifact, cargo_manifest_path)?;
        Ok(Self::from_target(cargo_manifest_path, package, target))
    }

    pub fn resolve_preferred(
        config: &Config,
        cargo: &Cargo,
        metadata: &CargoMetadata,
        cargo_manifest_path: &Path,
        preferred_artifact: &str,
    ) -> Result<Self> {
        let package_selector = cargo.effective_package_selector_with_artifact(
            config,
            metadata,
            cargo_manifest_path,
            Some(preferred_artifact),
        );
        let package = metadata.find_package(cargo_manifest_path, package_selector.as_deref())?;
        let target = package.resolve_library_target(preferred_artifact, cargo_manifest_path)?;
        Ok(Self::from_target(cargo_manifest_path, package, target))
    }

    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    pub fn cargo_manifest_path(&self) -> &Path {
        &self.cargo_manifest_path
    }

    pub fn manifest_path(&self) -> &Path {
        &self.package_manifest_path
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn artifact_name(&self) -> &str {
        &self.artifact_name
    }

    pub const fn builds_staticlib(&self) -> bool {
        self.builds_staticlib
    }

    pub const fn builds_cdylib(&self) -> bool {
        self.builds_cdylib
    }

    fn from_target(
        cargo_manifest_path: &Path,
        package: &CargoMetadataPackage,
        target: &CargoMetadataPackageTarget,
    ) -> Self {
        Self {
            package_id: package.id.clone(),
            package_name: package.name.clone(),
            cargo_manifest_path: cargo_manifest_path.to_path_buf(),
            package_manifest_path: package.manifest_path.clone(),
            source_path: target.src_path.clone(),
            artifact_name: target.name.clone(),
            builds_staticlib: target.builds_staticlib(),
            builds_cdylib: target.builds_cdylib(),
        }
    }
}

#[cfg(test)]
impl SelectedLibrary {
    pub fn fixture(
        package_name: impl Into<String>,
        package_manifest_path: impl AsRef<Path>,
        artifact_name: impl Into<String>,
    ) -> Self {
        let package_name = package_name.into();
        let package_manifest_path = package_manifest_path.as_ref().to_path_buf();
        let source_path = package_manifest_path
            .parent()
            .expect("selected library fixture manifest must have a parent")
            .join("src/lib.rs");
        Self {
            package_id: format!(
                "path+file://{}#{}@0.1.0",
                package_manifest_path.display(),
                package_name
            ),
            package_name,
            cargo_manifest_path: package_manifest_path.clone(),
            package_manifest_path,
            source_path,
            artifact_name: artifact_name.into(),
            builds_staticlib: true,
            builds_cdylib: true,
        }
    }

    pub fn fixture_cargo_manifest(mut self, cargo_manifest_path: impl AsRef<Path>) -> Self {
        self.cargo_manifest_path = cargo_manifest_path.as_ref().to_path_buf();
        self
    }

    pub const fn fixture_outputs(mut self, builds_staticlib: bool, builds_cdylib: bool) -> Self {
        self.builds_staticlib = builds_staticlib;
        self.builds_cdylib = builds_cdylib;
        self
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::SelectedLibrary;
    use crate::cargo::fixture::{CargoMetadataFixture, CargoPackageFixture, CargoTargetFixture};
    use crate::cargo::{Cargo, CargoCrateType};
    use crate::config::{CargoConfig, Config, PackageConfig, TargetsConfig};

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

    #[test]
    fn selects_exact_configured_workspace_library() {
        let cargo_manifest_path = Path::new("/tmp/workspace/Cargo.toml");
        let selected_manifest_path = Path::new("/tmp/workspace/ffi/Cargo.toml");
        let metadata = CargoMetadataFixture::new("/tmp/target")
            .package(
                CargoPackageFixture::workspace_package(
                    "ffi-package",
                    selected_manifest_path,
                    "1.2.3",
                )
                .target(CargoTargetFixture::library(
                    "ffi_artifact",
                    [CargoCrateType::StaticLib, CargoCrateType::Cdylib],
                )),
            )
            .package(
                CargoPackageFixture::workspace_package(
                    "other-package",
                    "/tmp/workspace/other/Cargo.toml",
                    "1.2.3",
                )
                .target(CargoTargetFixture::library(
                    "other_artifact",
                    [CargoCrateType::StaticLib],
                )),
            )
            .metadata();
        let cargo = Cargo::in_working_directory(
            "/tmp/workspace".into(),
            &[
                "--manifest-path".to_string(),
                cargo_manifest_path.display().to_string(),
            ],
        );

        let library = SelectedLibrary::resolve(
            &config(Some("ffi-artifact")),
            &cargo,
            &metadata,
            cargo_manifest_path,
            Some("ffi_artifact"),
        )
        .expect("configured library selection");

        assert_eq!(library.package_name(), "ffi-package");
        assert!(library.package_id().contains("#ffi-package@1.2.3"));
        assert_eq!(library.cargo_manifest_path(), cargo_manifest_path);
        assert_eq!(library.manifest_path(), selected_manifest_path);
        assert_eq!(library.artifact_name(), "ffi_artifact");
        assert!(library.builds_staticlib());
        assert!(library.builds_cdylib());
    }

    #[test]
    fn legacy_preference_falls_back_to_the_unique_ffi_library() {
        let cargo_manifest_path = Path::new("/tmp/workspace/member/Cargo.toml");
        let metadata = CargoMetadataFixture::new("/tmp/target")
            .package(
                CargoPackageFixture::manifest_package(
                    "workspace-member",
                    cargo_manifest_path,
                    "1.2.3",
                )
                .target(CargoTargetFixture::library(
                    "actual_artifact",
                    [CargoCrateType::StaticLib, CargoCrateType::Cdylib],
                ))
                .target(CargoTargetFixture::bin("workspace_member_cli")),
            )
            .metadata();
        let cargo = Cargo::in_working_directory("/tmp/workspace/member".into(), &[]);

        let library = SelectedLibrary::resolve_preferred(
            &config(None),
            &cargo,
            &metadata,
            cargo_manifest_path,
            "configured_name",
        )
        .expect("unique legacy library fallback");

        assert_eq!(library.cargo_manifest_path(), cargo_manifest_path);
        assert_eq!(library.artifact_name(), "actual_artifact");
        assert!(library.builds_staticlib());
        assert!(library.builds_cdylib());
    }
}
