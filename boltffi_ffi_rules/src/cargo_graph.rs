use crate::naming;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use syn::{Attribute, Item};
use walkdir::WalkDir;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageId(String);

#[derive(Clone, Debug)]
pub struct ExportedPackage {
    id: PackageId,
    import_name: String,
    manifest_dir: PathBuf,
    source_root: PathBuf,
    module_name: String,
}

pub struct PackageGraph {
    packages: HashMap<PackageId, CargoPackage>,
    dependencies: HashMap<PackageId, Vec<CargoDependency>>,
    root_id: PackageId,
}

#[derive(Debug)]
pub struct LoadError {
    message: String,
}

#[derive(Clone, Copy)]
enum MetadataMode {
    CurrentBuild,
    Standalone,
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: Option<CargoResolve>,
}

#[derive(Clone, Deserialize)]
struct CargoPackage {
    id: String,
    manifest_path: PathBuf,
    source: Option<String>,
    targets: Vec<CargoTarget>,
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
}

impl PackageId {
    fn new(value: String) -> Self {
        Self(value)
    }
}

impl PackageGraph {
    pub fn load(manifest_dir: &Path) -> Result<Option<Self>, LoadError> {
        Self::load_manifest(manifest_dir, None, MetadataMode::CurrentBuild)
    }

    pub fn load_for_module(
        manifest_dir: &Path,
        root_module_name: &str,
    ) -> Result<Option<Self>, LoadError> {
        Self::load_manifest(
            manifest_dir,
            Some(root_module_name),
            MetadataMode::Standalone,
        )
    }

    pub fn root_id(&self) -> &PackageId {
        &self.root_id
    }

    pub fn package(&self, id: &PackageId) -> Option<ExportedPackage> {
        self.packages
            .get(id)
            .and_then(|package| package.root_export(id.clone()))
    }

    pub fn exported_dependencies(&self, id: &PackageId) -> Vec<ExportedPackage> {
        self.dependencies
            .get(id)
            .into_iter()
            .flat_map(|dependencies| dependencies.iter())
            .filter_map(|dependency| {
                let package = self.packages.get(&dependency.package_id)?;
                package.is_local().then_some(())?;
                package.has_legacy_exports().then_some(())?;
                package.export(
                    dependency.package_id.clone(),
                    dependency.import_name.clone(),
                )
            })
            .collect()
    }

    pub fn reachable_exported_dependencies(&self, id: &PackageId) -> Vec<ExportedPackage> {
        self.collect_reachable_exported_dependencies(id, &mut HashSet::new())
    }

    fn load_manifest(
        manifest_dir: &Path,
        root_module_name: Option<&str>,
        metadata_mode: MetadataMode,
    ) -> Result<Option<Self>, LoadError> {
        let manifest_path = manifest_dir.join("Cargo.toml");
        if !manifest_path.exists() {
            return Ok(None);
        }

        let output = MetadataCommand::new(&manifest_path, metadata_mode).output()?;

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
        Ok(Some(Self::from_metadata(metadata, root_id)))
    }

    fn from_metadata(metadata: CargoMetadata, root_id: PackageId) -> Self {
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

        let missing = || {
            LoadError::new(format!(
                "cargo metadata did not include package for {}",
                manifest_path.display()
            ))
        };

        if let Some(package) = packages.iter().find(|package| {
            package
                .manifest_path
                .canonicalize()
                .is_ok_and(|path| path == canonical_manifest)
        }) {
            return Ok(PackageId::new(package.id.clone()));
        }

        let module_name = root_module_name
            .map(naming::cargo_crate_name)
            .ok_or_else(missing)?;
        let mut candidates = packages.iter().filter(|package| {
            package.is_local()
                && package.library_target_name().as_deref() == Some(module_name.as_str())
        });
        match (candidates.next(), candidates.next()) {
            (Some(package), None) => Ok(PackageId::new(package.id.clone())),
            (Some(_), Some(_)) => Err(LoadError::new(format!(
                "multiple local packages match module name `{module_name}` for {}",
                manifest_path.display()
            ))),
            (None, _) => Err(missing()),
        }
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

    pub fn source_root(&self) -> &Path {
        &self.source_root
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
    mode: MetadataMode,
}

impl<'a> MetadataCommand<'a> {
    fn new(manifest_path: &'a Path, mode: MetadataMode) -> Self {
        Self {
            manifest_path,
            mode,
        }
    }

    fn output(self) -> Result<Output, LoadError> {
        let mut command = Command::new(Self::cargo_executable());
        command
            .args(["metadata", "--format-version", "1", "--manifest-path"])
            .arg(self.manifest_path)
            .args(self.mode.args());
        command
            .output()
            .map_err(|error| LoadError::new(format!("cargo metadata failed: {error}")))
    }

    fn cargo_executable() -> OsString {
        env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
    }
}

impl MetadataMode {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::CurrentBuild | Self::Standalone if Self::env_flag("CARGO_FROZEN") => {
                &["--frozen"]
            }
            Self::CurrentBuild | Self::Standalone
                if Self::env_flag("CARGO_NET_OFFLINE") || Self::env_flag("CARGO_OFFLINE") =>
            {
                &["--offline"]
            }
            Self::CurrentBuild | Self::Standalone if Self::env_flag("CARGO_LOCKED") => {
                &["--locked"]
            }
            Self::CurrentBuild | Self::Standalone => &[],
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

    fn root_export(&self, id: PackageId) -> Option<ExportedPackage> {
        let module_name = self.library_target_name()?;
        self.export(id, module_name)
    }

    fn export(&self, id: PackageId, import_name: String) -> Option<ExportedPackage> {
        Some(ExportedPackage {
            id,
            import_name,
            manifest_dir: self.manifest_dir()?,
            source_root: self.source_root()?,
            module_name: self.library_target_name()?,
        })
    }

    fn has_legacy_exports(&self) -> bool {
        self.source_root().is_some_and(|source_root| {
            WalkDir::new(source_root)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "rs")
                })
                .map(|entry| entry.path().to_path_buf())
                .any(|path| {
                    fs::read_to_string(path)
                        .ok()
                        .and_then(|source| syn::parse_file(&source).ok())
                        .is_some_and(|syntax| LegacyExportDetector::file_has_exports(&syntax))
                })
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

struct LegacyExportDetector;

impl LegacyExportDetector {
    fn file_has_exports(file: &syn::File) -> bool {
        file.items.iter().any(Self::item_has_exports)
    }

    fn item_has_exports(item: &Item) -> bool {
        match item {
            Item::Struct(item_struct) => {
                Self::has_any_attribute(&item_struct.attrs, &["ffi_record", "data", "error"])
                    || Self::has_ffi_type_derive(&item_struct.attrs)
            }
            Item::Enum(item_enum) => Self::has_any_attribute(&item_enum.attrs, &["data", "error"]),
            Item::Impl(item_impl) => Self::has_any_attribute(
                &item_impl.attrs,
                &["ffi_class", "export", "data", "custom_ffi"],
            ),
            Item::Trait(item_trait) => {
                Self::has_any_attribute(&item_trait.attrs, &["ffi_trait", "export"])
            }
            Item::Fn(item_fn) => Self::has_any_attribute(&item_fn.attrs, &["ffi_export", "export"]),
            Item::Macro(item_macro) => item_macro
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "custom_type"),
            Item::Mod(item_mod) => item_mod
                .content
                .as_ref()
                .is_some_and(|(_, items)| items.iter().any(Self::item_has_exports)),
            _ => false,
        }
    }

    fn has_any_attribute(attrs: &[Attribute], names: &[&str]) -> bool {
        attrs.iter().any(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|segment| names.iter().any(|name| segment.ident == *name))
        })
    }

    fn has_ffi_type_derive(attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            attr.path().is_ident("derive")
                && attr
                    .meta
                    .require_list()
                    .is_ok_and(|meta| meta.tokens.to_string().contains("FfiType"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
    }

    fn virtual_root_manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crate dir has a parent")
            .join("Cargo.toml")
    }

    fn package_with_library_target(id: &str, target_name: &str) -> CargoPackage {
        CargoPackage {
            id: id.to_owned(),
            manifest_path: crate_manifest_path(),
            source: None,
            targets: vec![CargoTarget {
                name: target_name.to_owned(),
                kind: vec!["lib".to_owned()],
                src_path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
            }],
        }
    }

    #[test]
    fn resolves_root_by_manifest_path() {
        let packages = vec![package_with_library_target("my-lib-id", "my_lib")];

        let root_id = PackageGraph::resolve_root_id(&packages, &crate_manifest_path(), None)
            .expect("manifest path match");

        assert_eq!(root_id, PackageId::new("my-lib-id".to_owned()));
    }

    #[test]
    fn falls_back_to_module_name_for_virtual_root_manifest() {
        let packages = vec![package_with_library_target("my-lib-id", "my_lib")];

        let root_id =
            PackageGraph::resolve_root_id(&packages, &virtual_root_manifest_path(), Some("my_lib"))
                .expect("module name fallback match");

        assert_eq!(root_id, PackageId::new("my-lib-id".to_owned()));
    }

    #[test]
    fn module_name_fallback_normalizes_dashed_crate_names() {
        let packages = vec![package_with_library_target("my-lib-id", "my_lib")];

        let root_id =
            PackageGraph::resolve_root_id(&packages, &virtual_root_manifest_path(), Some("my-lib"))
                .expect("dashed module name normalizes to library target name");

        assert_eq!(root_id, PackageId::new("my-lib-id".to_owned()));
    }

    #[test]
    fn module_name_fallback_ignores_registry_packages() {
        let registry_package = CargoPackage {
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_owned()),
            ..package_with_library_target("registry-id", "my_lib")
        };
        let packages = vec![
            registry_package,
            package_with_library_target("local-id", "my_lib"),
        ];

        let root_id =
            PackageGraph::resolve_root_id(&packages, &virtual_root_manifest_path(), Some("my_lib"))
                .expect("local package match");

        assert_eq!(root_id, PackageId::new("local-id".to_owned()));
    }

    #[test]
    fn module_name_fallback_errors_on_ambiguous_matches() {
        let packages = vec![
            package_with_library_target("underscore-id", "foo_bar"),
            package_with_library_target("dash-id", "foo-bar"),
        ];

        let error = PackageGraph::resolve_root_id(
            &packages,
            &virtual_root_manifest_path(),
            Some("foo-bar"),
        )
        .expect_err("ambiguous module name");

        assert!(error.to_string().contains("multiple local packages"));
    }

    #[test]
    fn errors_when_no_package_matches_manifest_or_module_name() {
        let packages = vec![package_with_library_target("my-lib-id", "my_lib")];

        let error = PackageGraph::resolve_root_id(
            &packages,
            &virtual_root_manifest_path(),
            Some("other-lib"),
        )
        .expect_err("no matching package");

        assert!(
            error
                .to_string()
                .contains("cargo metadata did not include package")
        );
    }
}
