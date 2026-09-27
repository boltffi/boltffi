use std::path::{Path, PathBuf};

use serde::Serialize;

use super::metadata::{CargoCrateType, CargoMetadata};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CargoMetadataFixture {
    target_directory: PathBuf,
    workspace_root: Option<PathBuf>,
    packages: Vec<CargoPackageFixture>,
}

impl CargoMetadataFixture {
    pub(crate) fn new(target_directory: impl AsRef<Path>) -> Self {
        Self {
            target_directory: target_directory.as_ref().to_path_buf(),
            workspace_root: None,
            packages: Vec::new(),
        }
    }

    pub(crate) fn workspace_root(mut self, workspace_root: impl AsRef<Path>) -> Self {
        self.workspace_root = Some(workspace_root.as_ref().to_path_buf());
        self
    }

    pub(crate) fn package(mut self, package: CargoPackageFixture) -> Self {
        self.packages.push(package);
        self
    }

    pub(crate) fn json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("cargo metadata fixture")
    }

    pub(crate) fn metadata(&self) -> CargoMetadata {
        serde_json::from_slice(&self.json_bytes()).expect("cargo metadata fixture")
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CargoPackageFixture {
    id: String,
    name: String,
    version: String,
    manifest_path: PathBuf,
    targets: Vec<CargoTargetFixture>,
}

impl CargoPackageFixture {
    pub(crate) fn manifest_package(
        name: impl Into<String>,
        manifest_path: impl AsRef<Path>,
        version: impl Into<String>,
    ) -> Self {
        let package_name = name.into();
        let version = version.into();
        let manifest_path = manifest_path.as_ref().to_path_buf();
        let package_id =
            CargoPackageIdFixture::manifest_package(&manifest_path, version.clone()).render();

        Self {
            id: package_id,
            name: package_name,
            version,
            manifest_path,
            targets: Vec::new(),
        }
    }

    pub(crate) fn workspace_package(
        name: impl Into<String>,
        manifest_path: impl AsRef<Path>,
        version: impl Into<String>,
    ) -> Self {
        let package_name = name.into();
        let version = version.into();
        let manifest_path = manifest_path.as_ref().to_path_buf();
        let package_id = CargoPackageIdFixture::workspace_package(
            &manifest_path,
            &package_name,
            version.clone(),
        )
        .render();

        Self {
            id: package_id,
            name: package_name,
            version,
            manifest_path,
            targets: Vec::new(),
        }
    }

    pub(crate) fn target(mut self, target: CargoTargetFixture) -> Self {
        self.targets.push(target);
        self
    }
}

#[derive(Debug, Clone)]
enum CargoPackageIdFixture {
    Manifest {
        manifest_directory: PathBuf,
        version: String,
    },
    Workspace {
        workspace_root: PathBuf,
        package_name: String,
        version: String,
    },
}

impl CargoPackageIdFixture {
    fn manifest_package(manifest_path: &Path, version: String) -> Self {
        Self::Manifest {
            manifest_directory: manifest_directory(manifest_path),
            version,
        }
    }

    fn workspace_package(manifest_path: &Path, package_name: &str, version: String) -> Self {
        Self::Workspace {
            workspace_root: manifest_directory(manifest_path),
            package_name: package_name.to_string(),
            version,
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Manifest {
                manifest_directory,
                version,
            } => format!("path+file://{}#{}", manifest_directory.display(), version),
            Self::Workspace {
                workspace_root,
                package_name,
                version,
            } => format!(
                "path+file://{}#{}@{}",
                workspace_root.display(),
                package_name,
                version
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CargoTargetFixture {
    name: String,
    crate_types: Vec<String>,
    src_path: PathBuf,
}

impl CargoTargetFixture {
    pub(crate) fn library(
        name: impl Into<String>,
        crate_types: impl IntoIterator<Item = CargoCrateType>,
    ) -> Self {
        Self {
            name: name.into(),
            crate_types: crate_types.into_iter().map(Into::into).collect(),
            src_path: PathBuf::from("src/lib.rs"),
        }
    }

    /// An `[[example]]`, which cargo lets a package build as a cdylib too.
    pub(crate) fn example(
        name: impl Into<String>,
        crate_types: impl IntoIterator<Item = CargoCrateType>,
    ) -> Self {
        let target = Self::library(name, crate_types);
        let src_path = PathBuf::from(format!("examples/{}.rs", target.name));
        Self { src_path, ..target }
    }

    pub(crate) fn bin(name: impl Into<String>) -> Self {
        Self::library(name, [CargoCrateType::Bin])
    }
}

fn manifest_directory(manifest_path: &Path) -> PathBuf {
    manifest_path
        .parent()
        .expect("cargo metadata fixture manifest path should have a parent")
        .to_path_buf()
}
