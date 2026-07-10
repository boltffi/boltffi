mod exports;
mod features;

pub use features::{CargoFeatureSelection, ResolvedFeatures};

use crate::naming;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageId(String);

#[derive(Clone, Debug)]
pub struct ExportedPackage {
    id: PackageId,
    import_name: String,
    manifest_dir: PathBuf,
    source_file: PathBuf,
    module_name: String,
}

pub type LocalPackage = ExportedPackage;

pub struct PackageGraph {
    packages: HashMap<PackageId, CargoPackage>,
    dependencies: HashMap<PackageId, Vec<CargoDependency>>,
    root_id: PackageId,
    root_features: ResolvedFeatures,
}

#[derive(Debug)]
pub struct LoadError {
    message: String,
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: Option<CargoResolve>,
}

#[derive(Clone, Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    source: Option<String>,
    targets: Vec<CargoTarget>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

#[derive(Deserialize)]
struct CargoResolve {
    nodes: Vec<CargoNode>,
}

#[derive(Deserialize)]
struct CargoNode {
    id: String,
    deps: Vec<CargoNodeDependency>,
}

#[derive(Deserialize)]
struct CargoNodeDependency {
    name: String,
    pkg: String,
    #[serde(default)]
    dep_kinds: Vec<CargoDependencyKind>,
}

#[derive(Deserialize)]
struct CargoDependencyKind {
    kind: Option<String>,
}

impl PackageId {
    fn new(value: String) -> Self {
        Self(value)
    }
}

impl PackageGraph {
    pub fn load(manifest_dir: &Path) -> Result<Option<Self>, LoadError> {
        Self::load_manifest(manifest_dir, None, &CargoFeatureSelection::default())
    }

    pub fn load_for_module(
        manifest_dir: &Path,
        root_module_name: &str,
    ) -> Result<Option<Self>, LoadError> {
        Self::load_manifest(
            manifest_dir,
            Some(root_module_name),
            &CargoFeatureSelection::default(),
        )
    }

    pub fn load_with_features(
        manifest_dir: &Path,
        root_module_name: &str,
        features: &CargoFeatureSelection,
    ) -> Result<Option<Self>, LoadError> {
        Self::load_manifest(manifest_dir, Some(root_module_name), features)
    }

    pub fn root_id(&self) -> &PackageId {
        &self.root_id
    }

    pub fn root_features(&self) -> &ResolvedFeatures {
        &self.root_features
    }

    pub fn package(&self, id: &PackageId) -> Option<LocalPackage> {
        let package = self.packages.get(id)?;
        package.root(id.clone())
    }

    pub fn direct_local_dependencies(&self, id: &PackageId) -> Vec<LocalPackage> {
        self.dependencies
            .get(id)
            .into_iter()
            .flat_map(|dependencies| dependencies.iter())
            .filter_map(|dependency| {
                let package = self.packages.get(&dependency.package_id)?;
                package.is_local().then_some(())?;
                package.local(
                    dependency.package_id.clone(),
                    dependency.import_name.clone(),
                )
            })
            .collect()
    }

    pub fn exported_dependencies(&self, id: &PackageId) -> Vec<ExportedPackage> {
        self.direct_local_dependencies(id)
            .into_iter()
            .filter(exports::PackageExports::has_exports)
            .collect()
    }

    pub fn reachable_exported_dependencies(&self, id: &PackageId) -> Vec<ExportedPackage> {
        self.collect_reachable_exported_dependencies(id, &mut HashSet::new())
    }

    fn load_manifest(
        manifest_dir: &Path,
        root_module_name: Option<&str>,
        features: &CargoFeatureSelection,
    ) -> Result<Option<Self>, LoadError> {
        let manifest_path = manifest_dir.join("Cargo.toml");
        if !manifest_path.exists() {
            return Ok(None);
        }

        let output = MetadataCommand::new(&manifest_path, features).output()?;

        if !output.status.success() {
            return Err(LoadError::new(format!(
                "cargo metadata failed with status {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
            .map_err(|error| LoadError::new(format!("failed to parse cargo metadata: {error}")))?;
        let root_id = Self::resolve_root_id(&metadata.packages, &manifest_path, root_module_name)?;
        Ok(Some(Self::from_metadata(metadata, root_id, features)))
    }

    fn from_metadata(
        metadata: CargoMetadata,
        root_id: PackageId,
        feature_selection: &CargoFeatureSelection,
    ) -> Self {
        let root_features = metadata
            .packages
            .iter()
            .find(|package| package.id == root_id.0)
            .map(|package| feature_selection.resolve_for_package(&package.name, &package.features))
            .unwrap_or_default();
        let packages = metadata
            .packages
            .into_iter()
            .map(|package| (PackageId::new(package.id.clone()), package))
            .collect::<HashMap<_, _>>();
        let dependencies = metadata
            .resolve
            .map(|resolve| {
                resolve
                    .nodes
                    .into_iter()
                    .map(|node| {
                        (
                            PackageId::new(node.id),
                            node.deps
                                .into_iter()
                                .filter(CargoNodeDependency::is_normal)
                                .map(CargoDependency::from)
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        Self {
            packages,
            dependencies,
            root_id,
            root_features,
        }
    }

    fn resolve_root_id(
        packages: &[CargoPackage],
        manifest_path: &Path,
        root_module_name: Option<&str>,
    ) -> Result<PackageId, LoadError> {
        let canonical_manifest = manifest_path.canonicalize().map_err(|error| {
            LoadError::new(format!(
                "failed to canonicalize {}: {error}",
                manifest_path.display()
            ))
        })?;

        packages
            .iter()
            .find(|package| {
                package
                    .manifest_path
                    .canonicalize()
                    .is_ok_and(|path| path == canonical_manifest)
            })
            .or_else(|| {
                root_module_name.and_then(|module_name| {
                    packages.iter().find(|package| {
                        package.library_target_name().as_deref() == Some(module_name)
                    })
                })
            })
            .map(|package| PackageId::new(package.id.clone()))
            .ok_or_else(|| {
                LoadError::new(format!(
                    "cargo metadata did not include package for {}",
                    manifest_path.display()
                ))
            })
    }

    fn collect_reachable_exported_dependencies(
        &self,
        id: &PackageId,
        visited: &mut HashSet<PackageId>,
    ) -> Vec<ExportedPackage> {
        self.exported_dependencies(id)
            .into_iter()
            .flat_map(|package| {
                if !visited.insert(package.id.clone()) {
                    return Vec::new();
                }
                self.collect_reachable_exported_dependencies(package.id(), visited)
                    .into_iter()
                    .chain(std::iter::once(package))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

impl ExportedPackage {
    pub fn id(&self) -> &PackageId {
        &self.id
    }

    pub fn root_path(&self) -> Vec<String> {
        vec![self.import_name.clone()]
    }

    pub fn manifest_dir(&self) -> &Path {
        &self.manifest_dir
    }

    pub fn source_file(&self) -> &Path {
        &self.source_file
    }

    pub fn source_root(&self) -> &Path {
        self.source_file
            .parent()
            .unwrap_or(self.manifest_dir.as_path())
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }
}

impl LoadError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LoadError {}

struct MetadataCommand<'a> {
    manifest_path: &'a Path,
    features: &'a CargoFeatureSelection,
}

impl<'a> MetadataCommand<'a> {
    fn new(manifest_path: &'a Path, features: &'a CargoFeatureSelection) -> Self {
        Self {
            manifest_path,
            features,
        }
    }

    fn output(self) -> Result<Output, LoadError> {
        let mut command = Command::new(Self::cargo_executable());
        command
            .args(["metadata", "--format-version", "1", "--manifest-path"])
            .arg(self.manifest_path)
            .args(Self::mode_args())
            .args(self.features.cargo_arguments());
        command
            .output()
            .map_err(|error| LoadError::new(format!("cargo metadata failed: {error}")))
    }

    fn cargo_executable() -> OsString {
        env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
    }

    fn mode_args() -> &'static [&'static str] {
        if Self::env_flag("CARGO_FROZEN") {
            &["--frozen"]
        } else if Self::env_flag("CARGO_NET_OFFLINE") || Self::env_flag("CARGO_OFFLINE") {
            &["--offline"]
        } else if Self::env_flag("CARGO_LOCKED") {
            &["--locked"]
        } else {
            &[]
        }
    }

    fn env_flag(name: &str) -> bool {
        env::var(name)
            .ok()
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
    }
}

#[derive(Clone)]
struct CargoDependency {
    import_name: String,
    package_id: PackageId,
}

impl From<CargoNodeDependency> for CargoDependency {
    fn from(dependency: CargoNodeDependency) -> Self {
        Self {
            import_name: naming::cargo_crate_name(&dependency.name),
            package_id: PackageId::new(dependency.pkg),
        }
    }
}

impl CargoNodeDependency {
    fn is_normal(&self) -> bool {
        self.dep_kinds.is_empty()
            || self
                .dep_kinds
                .iter()
                .any(|dependency| dependency.kind.is_none())
    }
}

impl CargoPackage {
    fn is_local(&self) -> bool {
        self.source.is_none()
    }

    fn manifest_dir(&self) -> Option<PathBuf> {
        self.manifest_path.parent().map(Path::to_path_buf)
    }

    fn library_target_name(&self) -> Option<String> {
        self.targets
            .iter()
            .find(|target| target.is_library())
            .map(|target| naming::cargo_crate_name(&target.name))
    }

    fn source_root(&self) -> Option<PathBuf> {
        self.targets
            .iter()
            .find(|target| target.is_library())
            .and_then(|target| target.src_path.parent())
            .map(Path::to_path_buf)
            .or_else(|| {
                self.manifest_dir()
                    .map(|manifest_dir| manifest_dir.join("src"))
            })
    }

    fn source_file(&self) -> Option<PathBuf> {
        self.targets
            .iter()
            .find(|target| target.is_library())
            .map(|target| target.src_path.clone())
            .or_else(|| {
                self.source_root()
                    .map(|source_root| source_root.join("lib.rs"))
            })
    }

    fn root(&self, id: PackageId) -> Option<LocalPackage> {
        let module_name = self.library_target_name()?;
        self.local(id, module_name)
    }

    fn local(&self, id: PackageId, import_name: String) -> Option<LocalPackage> {
        Some(LocalPackage {
            id,
            import_name,
            manifest_dir: self.manifest_dir()?,
            source_file: self.source_file()?,
            module_name: self.library_target_name()?,
        })
    }
}

impl CargoTarget {
    fn is_library(&self) -> bool {
        self.kind.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "lib" | "rlib" | "staticlib" | "cdylib" | "dylib"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CargoNodeDependency;

    #[test]
    fn dependency_graph_keeps_only_normal_library_edges() {
        let normal = serde_json::from_str::<CargoNodeDependency>(
            r#"{
                "name": "api",
                "pkg": "api 0.1.0",
                "dep_kinds": [{ "kind": null, "target": null }]
            }"#,
        )
        .expect("normal dependency");
        let development = serde_json::from_str::<CargoNodeDependency>(
            r#"{
                "name": "fixtures",
                "pkg": "fixtures 0.1.0",
                "dep_kinds": [{ "kind": "dev", "target": null }]
            }"#,
        )
        .expect("development dependency");
        let build = serde_json::from_str::<CargoNodeDependency>(
            r#"{
                "name": "codegen",
                "pkg": "codegen 0.1.0",
                "dep_kinds": [{ "kind": "build", "target": null }]
            }"#,
        )
        .expect("build dependency");

        assert!(normal.is_normal());
        assert!(!development.is_normal());
        assert!(!build.is_normal());
    }
}
