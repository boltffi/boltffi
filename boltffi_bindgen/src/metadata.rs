//! Builds a Rust crate and reads the BoltFFI source records it embeds.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use boltffi_binding::{BindingMetadataSurface, RawSourceRecord};
use serde::Deserialize;
use thiserror::Error;

use crate::artifact::{BindingMetadataReadError, BindingMetadataReader};
use crate::cargo::{LibraryCargoArgs, LibraryCargoArgsError};

/// A Cargo library build that reads the BoltFFI source records its artifacts embed.
///
/// The build enables the `boltffi_metadata` cfg and reads Cargo's JSON
/// artifact stream. Artifact decoding is delegated to [`BindingMetadataReader`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingMetadataBuild {
    manifest_path: PathBuf,
    target: Option<String>,
    surface: Option<BindingMetadataSurface>,
    toolchain_selector: Option<String>,
    cargo_args: Result<MetadataCargoArgs, LibraryCargoArgsError>,
    cargo_environment: Vec<(OsString, OsString)>,
}

impl BindingMetadataBuild {
    /// Creates a metadata build for a Cargo manifest.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            target: None,
            surface: None,
            toolchain_selector: None,
            cargo_args: Ok(MetadataCargoArgs::default()),
            cargo_environment: Vec::new(),
        }
    }

    /// Builds for a Cargo target triple.
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    #[allow(missing_docs)]
    pub fn surface(mut self, surface: BindingMetadataSurface) -> Self {
        self.surface = Some(surface);
        self
    }

    /// Whether the build's records describe the native surface its library exports.
    pub fn surface_is_native(&self) -> bool {
        self.surface
            .unwrap_or_else(|| BindingMetadataSurface::from_target_triple(self.target.as_deref()))
            == BindingMetadataSurface::Native
    }

    /// Passes Cargo build arguments to the metadata build.
    pub fn cargo_args(mut self, cargo_args: impl IntoIterator<Item = String>) -> Self {
        let cargo_args = cargo_args.into_iter().collect::<Vec<_>>();
        if self.toolchain_selector.is_none() {
            self.toolchain_selector = cargo_args
                .iter()
                .find(|argument| is_rustup_toolchain_selector(argument))
                .cloned();
        }
        self.cargo_args = MetadataCargoArgs::new(cargo_args);
        self
    }

    /// Passes environment values to Cargo metadata and build commands.
    pub fn cargo_environment<K, V>(mut self, environment: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.cargo_environment = environment
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        self
    }

    /// Selects a rustup Cargo toolchain for metadata and build commands.
    pub fn rustup_toolchain(mut self, toolchain_selector: impl Into<String>) -> Self {
        self.toolchain_selector = Some(toolchain_selector.into());
        self
    }

    /// Runs a Cargo build with the metadata cfgs and reads per-invocation source
    /// records from every reported artifact, dependency crates included.
    pub fn read_source(&self) -> Result<SourceMetadata, BindingMetadataBuildError> {
        let cargo_args = self
            .cargo_args
            .as_ref()
            .map_err(|source| BindingMetadataBuildError::CargoArguments(source.clone()))?;
        let manifest = CargoManifest::new(&self.manifest_path)?;
        let metadata = CargoMetadata::load(
            &manifest,
            self.toolchain_selector.as_deref(),
            &self.cargo_environment,
        )?;
        metadata.library_source(&manifest)?;
        let output =
            CargoBuild::new(self, cargo_args, metadata.target_directory()).plain_output()?;
        let package = metadata.package_info(&manifest)?;
        let local_packages = metadata.local_packages(&manifest)?;

        let artifacts = output.all_artifacts(&manifest)?.into_paths();
        let libraries = output.root_libraries(&manifest)?;
        let source_records = BindingMetadataReader::new(artifacts.clone())
            .read_source_records()
            .map_err(BindingMetadataBuildError::Metadata)?;

        Ok(SourceMetadata {
            source_records,
            package,
            artifacts,
            libraries,
            local_packages,
        })
    }
}

/// Per-invocation source records read from one plain Cargo build.
#[derive(Debug)]
pub struct SourceMetadata {
    /// Source records from every artifact, dependencies included.
    pub source_records: Vec<RawSourceRecord>,
    /// Root package identity from `cargo metadata`.
    pub package: boltffi_ast::PackageInfo,
    /// Compiled artifact paths the records were read from.
    pub artifacts: Vec<PathBuf>,
    /// The root package's linked libraries: its dynamic and static library artifacts.
    pub libraries: Vec<PathBuf>,
    /// The root and every path package its normal dependencies reach.
    pub local_packages: Vec<boltffi_ast::PackageInfo>,
}

/// Failure while building a crate for embedded binding metadata.
#[derive(Debug, Error)]
pub enum BindingMetadataBuildError {
    #[error(transparent)]
    CargoArguments(#[from] LibraryCargoArgsError),
    /// Cargo could not be started.
    #[error("run cargo rustc for binding metadata: {source}")]
    CargoSpawn {
        /// Process spawn error.
        source: std::io::Error,
    },
    /// Cargo returned a non-zero exit status.
    #[error("cargo rustc for binding metadata failed with status {status}: {stderr}")]
    CargoFailed {
        /// Process exit status.
        status: CargoStatus,
        /// Cargo standard error.
        stderr: String,
    },
    /// Cargo emitted a malformed JSON message.
    #[error("parse cargo JSON message `{line}`: {source}")]
    CargoJson {
        /// Raw Cargo output line.
        line: String,
        /// JSON parse error.
        source: serde_json::Error,
    },
    /// The requested manifest path could not be resolved.
    #[error("resolve cargo manifest path `{path}`: {source}")]
    ManifestPath {
        /// Manifest path passed to Cargo.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
    /// Cargo did not report a readable compiled artifact.
    #[error("cargo rustc for `{manifest_path}` did not report compiled library artifacts")]
    NoArtifacts {
        /// Manifest path passed to Cargo.
        manifest_path: PathBuf,
    },
    /// Cargo metadata did not expose a library target source path.
    #[error("cargo metadata for `{manifest_path}` did not report a library target source")]
    NoLibrarySource {
        /// Manifest path passed to Cargo.
        manifest_path: PathBuf,
    },
    /// Cargo metadata did not expose the selected package.
    #[error("cargo metadata for `{manifest_path}` did not report the selected package")]
    NoPackage {
        /// Manifest path passed to Cargo.
        manifest_path: PathBuf,
    },
    /// Embedded metadata could not be read from the produced artifacts.
    #[error(transparent)]
    Metadata(#[from] BindingMetadataReadError),
}

/// Exit status reported by Cargo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CargoStatus {
    code: Option<i32>,
}

impl CargoStatus {
    fn from_status(status: ExitStatus) -> Self {
        Self {
            code: status.code(),
        }
    }

    /// Returns Cargo's process exit code.
    pub const fn code(self) -> Option<i32> {
        self.code
    }
}

impl std::fmt::Display for CargoStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.code
            .map(|code| write!(formatter, "{code}"))
            .unwrap_or_else(|| formatter.write_str("terminated by signal"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CargoManifest {
    path: PathBuf,
}

impl CargoManifest {
    fn new(path: &Path) -> Result<Self, BindingMetadataBuildError> {
        canonicalize_manifest_path(path)
            .map(|path| Self { path })
            .map_err(|source| BindingMetadataBuildError::ManifestPath {
                path: path.to_path_buf(),
                source,
            })
    }

    fn matches(&self, path: &Path) -> bool {
        canonicalize_manifest_path(path).is_ok_and(|path| path == self.path)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(windows)]
fn canonicalize_manifest_path(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

#[cfg(not(windows))]
fn canonicalize_manifest_path(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
    /// Cargo's resolved target directory. Trustworthy because `load` runs cargo
    /// with the caller's environment and toolchain, so `CARGO_TARGET_DIR` and
    /// any config override are already applied.
    #[serde(default)]
    target_directory: PathBuf,
    #[serde(default)]
    resolve: Option<MetadataResolve>,
}

impl CargoMetadata {
    fn load(
        manifest: &CargoManifest,
        toolchain_selector: Option<&str>,
        cargo_environment: &[(OsString, OsString)],
    ) -> Result<Self, BindingMetadataBuildError> {
        let mut command = Command::new(CargoProgram::from_env().into_os_string());
        command.envs(cargo_environment.iter().map(|(key, value)| (key, value)));
        if let Some(toolchain_selector) = toolchain_selector {
            command.arg(toolchain_selector);
        }
        let output = command
            .arg("metadata")
            .arg("--format-version=1")
            .arg("--all-features")
            .arg("--manifest-path")
            .arg(manifest.path())
            .output()
            .map_err(|source| BindingMetadataBuildError::CargoSpawn { source })?;
        if !output.status.success() {
            return Err(BindingMetadataBuildError::CargoFailed {
                status: CargoStatus::from_status(output.status),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        serde_json::from_slice(&output.stdout).map_err(|source| {
            BindingMetadataBuildError::CargoJson {
                line: String::from_utf8_lossy(&output.stdout).into_owned(),
                source,
            }
        })
    }

    /// `None` when cargo reported no target directory, which leaves cargo's own
    /// default in place rather than guessing one.
    fn target_directory(&self) -> Option<&Path> {
        match self.target_directory.as_os_str().is_empty() {
            true => None,
            false => Some(&self.target_directory),
        }
    }

    fn package(
        &self,
        manifest: &CargoManifest,
    ) -> Result<&MetadataPackage, BindingMetadataBuildError> {
        self.packages
            .iter()
            .find(|package| manifest.matches(&package.manifest_path))
            .ok_or_else(|| BindingMetadataBuildError::NoPackage {
                manifest_path: manifest.path().to_path_buf(),
            })
    }

    fn library_source(
        &self,
        manifest: &CargoManifest,
    ) -> Result<PathBuf, BindingMetadataBuildError> {
        self.package(manifest)?.library_source().ok_or_else(|| {
            BindingMetadataBuildError::NoLibrarySource {
                manifest_path: manifest.path().to_path_buf(),
            }
        })
    }

    fn package_info(
        &self,
        manifest: &CargoManifest,
    ) -> Result<boltffi_ast::PackageInfo, BindingMetadataBuildError> {
        self.package(manifest).map(MetadataPackage::info)
    }

    /// Walks normal dependency edges from the root, since only those are linked into it.
    fn local_packages(
        &self,
        manifest: &CargoManifest,
    ) -> Result<Vec<boltffi_ast::PackageInfo>, BindingMetadataBuildError> {
        let root = self.package(manifest)?;
        let nodes = self
            .resolve
            .iter()
            .flat_map(|resolve| resolve.nodes.iter())
            .map(|node| (node.id.as_str(), node))
            .collect::<HashMap<_, _>>();
        let mut reached = HashSet::from([root.id.as_str()]);
        let mut pending = vec![root.id.as_str()];
        while let Some(id) = pending.pop() {
            nodes
                .get(id)
                .into_iter()
                .flat_map(|node| node.deps.iter())
                .filter(|dependency| dependency.is_normal())
                .for_each(|dependency| {
                    if reached.insert(dependency.pkg.as_str()) {
                        pending.push(dependency.pkg.as_str());
                    }
                });
        }
        Ok(self
            .packages
            .iter()
            .filter(|package| package.source.is_none() && reached.contains(package.id.as_str()))
            .map(MetadataPackage::info)
            .collect())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataResolve {
    nodes: Vec<MetadataNode>,
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataNode {
    id: String,
    #[serde(default)]
    deps: Vec<MetadataDependency>,
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataDependency {
    pkg: String,
    #[serde(default)]
    dep_kinds: Vec<MetadataDependencyKind>,
}

impl MetadataDependency {
    fn is_normal(&self) -> bool {
        self.dep_kinds.is_empty() || self.dep_kinds.iter().any(|kind| kind.kind.is_none())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataDependencyKind {
    kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    version: Option<String>,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
}

impl MetadataPackage {
    fn info(&self) -> boltffi_ast::PackageInfo {
        boltffi_ast::PackageInfo::new(
            self.name.clone(),
            self.version.clone().filter(|version| !version.is_empty()),
        )
    }

    fn library_source(&self) -> Option<PathBuf> {
        self.targets
            .iter()
            .find(|target| target.is_library())
            .map(MetadataTarget::source)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataTarget {
    kind: Vec<String>,
    src_path: PathBuf,
}

impl MetadataTarget {
    fn is_library(&self) -> bool {
        self.kind.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "lib" | "rlib" | "dylib" | "cdylib" | "staticlib"
            )
        })
    }

    fn source(&self) -> PathBuf {
        self.src_path.clone()
    }
}

#[derive(Clone, Debug)]
struct CargoBuild<'build> {
    build: &'build BindingMetadataBuild,
    cargo_args: &'build MetadataCargoArgs,
    target_directory: Option<&'build Path>,
}

impl<'build> CargoBuild<'build> {
    fn new(
        build: &'build BindingMetadataBuild,
        cargo_args: &'build MetadataCargoArgs,
        target_directory: Option<&'build Path>,
    ) -> Self {
        Self {
            build,
            cargo_args,
            target_directory,
        }
    }

    /// Target directory for the metadata build: `<target>/boltffi-metadata`.
    ///
    /// Cargo records `cargo rustc -- --cfg …` and tracked env in a unit's
    /// fingerprint but not in its hash, so this build and an ordinary
    /// `cargo build` of the same crate and features write the *same* artifact
    /// slot and dirty each other — alternating them recompiles the crate every
    /// time. A directory of its own keeps both caches warm.
    ///
    /// `None` leaves cargo's default in place: a caller that chose a target
    /// directory itself keeps it, and a failure to resolve one is not worth
    /// failing the build over.
    fn metadata_target_dir(&self) -> Option<PathBuf> {
        if self
            .cargo_args
            .iter()
            .any(|arg| arg.starts_with("--target-dir"))
        {
            return None;
        }
        Some(self.target_directory?.join("boltffi-metadata"))
    }

    fn plain_output(self) -> Result<CargoOutput, BindingMetadataBuildError> {
        self.plain_command()
            .output()
            .map_err(|source| BindingMetadataBuildError::CargoSpawn { source })
            .and_then(CargoOutput::from_output)
    }

    fn plain_command(self) -> Command {
        let mut command = self.base_command();
        if let Some(target_dir) = self.metadata_target_dir() {
            command
                .arg("--target-dir")
                .arg(target_dir.with_file_name("boltffi-source-records"));
        }
        self.metadata_cfgs(&mut command);
        command
    }

    /// Both builds compile the root crate with the metadata cfgs, so cfg-gated
    /// exports surface identically; only the env gates separate them.
    fn metadata_cfgs(&self, command: &mut Command) {
        let surface = self.build.surface.unwrap_or_else(|| {
            BindingMetadataSurface::from_target_triple(self.build.target.as_deref())
        });
        command
            .arg("--")
            .arg("--cfg")
            .arg("boltffi_metadata")
            .arg("--cfg")
            .arg(format!("boltffi_binding_surface_{}", surface.as_str()));
    }

    fn base_command(&self) -> Command {
        let mut command = Command::new(CargoProgram::from_env().into_os_string());
        command.envs(
            self.build
                .cargo_environment
                .iter()
                .map(|(key, value)| (key, value)),
        );
        if let Some(toolchain_selector) = self.build.toolchain_selector.as_deref() {
            command.arg(toolchain_selector);
        }
        command
            .arg("rustc")
            .arg("--lib")
            .arg("--message-format=json-render-diagnostics")
            .arg("--manifest-path")
            .arg(&self.build.manifest_path);
        if let Some(target) = &self.build.target {
            command.arg("--target").arg(target);
        }
        command.args(self.cargo_args.iter());
        command
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CargoProgram {
    program: OsString,
}

impl CargoProgram {
    fn from_env() -> Self {
        Self {
            program: std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")),
        }
    }

    fn into_os_string(self) -> OsString {
        self.program
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct MetadataCargoArgs {
    arguments: LibraryCargoArgs,
}

impl MetadataCargoArgs {
    fn new(arguments: impl IntoIterator<Item = String>) -> Result<Self, LibraryCargoArgsError> {
        LibraryCargoArgs::parse(Self::without_owned_selectors(
            arguments.into_iter().collect(),
        ))
        .map(|arguments| Self { arguments })
    }

    fn iter(&self) -> impl Iterator<Item = &String> {
        self.arguments.iter()
    }

    fn without_owned_selectors(arguments: Vec<String>) -> Vec<String> {
        let mut skip_value = false;
        arguments
            .into_iter()
            .filter_map(move |argument| {
                if skip_value {
                    skip_value = false;
                    return None;
                }

                if matches!(argument.as_str(), "--manifest-path" | "--target") {
                    skip_value = true;
                    return None;
                }

                (!argument.starts_with("--manifest-path=")
                    && !argument.starts_with("--target=")
                    && !is_rustup_toolchain_selector(&argument))
                .then_some(argument)
            })
            .collect()
    }
}

fn is_rustup_toolchain_selector(argument: &str) -> bool {
    argument.starts_with('+') && argument.len() > 1
}

#[derive(Clone, Debug)]
struct CargoOutput {
    stdout: String,
}

impl CargoOutput {
    fn from_output(output: std::process::Output) -> Result<Self, BindingMetadataBuildError> {
        if output.status.success() {
            Ok(Self {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            })
        } else {
            Err(BindingMetadataBuildError::CargoFailed {
                status: CargoStatus::from_status(output.status),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    fn all_artifacts(
        &self,
        manifest: &CargoManifest,
    ) -> Result<MetadataArtifacts, BindingMetadataBuildError> {
        let artifacts = self
            .messages()?
            .into_iter()
            .flat_map(CargoMessage::into_filenames)
            .filter_map(MetadataArtifact::from_cargo_filename)
            .collect::<Vec<_>>();

        MetadataArtifacts::new(manifest.path(), artifacts)
    }

    fn root_libraries(
        &self,
        manifest: &CargoManifest,
    ) -> Result<Vec<PathBuf>, BindingMetadataBuildError> {
        Ok(self
            .messages()?
            .into_iter()
            .filter(|message| message.built_from(manifest))
            .flat_map(CargoMessage::into_filenames)
            .filter(|path| {
                path.extension()
                    .and_then(OsStr::to_str)
                    .is_some_and(|extension| {
                        matches!(extension, "a" | "dll" | "dylib" | "lib" | "so" | "wasm")
                    })
            })
            .collect())
    }

    fn messages(&self) -> Result<Vec<CargoMessage>, BindingMetadataBuildError> {
        self.stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(CargoMessage::parse)
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MetadataArtifacts {
    artifacts: Vec<MetadataArtifact>,
}

impl MetadataArtifacts {
    fn new(
        manifest_path: &Path,
        artifacts: Vec<MetadataArtifact>,
    ) -> Result<Self, BindingMetadataBuildError> {
        if artifacts.is_empty() {
            Err(BindingMetadataBuildError::NoArtifacts {
                manifest_path: manifest_path.to_path_buf(),
            })
        } else {
            Ok(Self { artifacts })
        }
    }

    fn into_paths(self) -> Vec<PathBuf> {
        self.artifacts
            .into_iter()
            .map(MetadataArtifact::into_path)
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MetadataArtifact {
    path: PathBuf,
}

impl MetadataArtifact {
    fn from_cargo_filename(path: PathBuf) -> Option<Self> {
        path.extension()
            .and_then(OsStr::to_str)
            .is_some_and(Self::metadata_extension)
            .then_some(Self { path })
    }

    fn metadata_extension(extension: &str) -> bool {
        matches!(
            extension,
            "a" | "dll" | "dylib" | "lib" | "o" | "obj" | "rlib" | "so" | "wasm"
        )
    }

    fn into_path(self) -> PathBuf {
        self.path
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
    CompilerArtifact {
        manifest_path: PathBuf,
        filenames: Vec<PathBuf>,
    },
    #[serde(other)]
    Other,
}

impl CargoMessage {
    fn parse(line: &str) -> Result<Self, BindingMetadataBuildError> {
        serde_json::from_str(line).map_err(|source| BindingMetadataBuildError::CargoJson {
            line: line.to_owned(),
            source,
        })
    }

    fn built_from(&self, manifest: &CargoManifest) -> bool {
        match self {
            Self::CompilerArtifact { manifest_path, .. } => manifest.matches(manifest_path),
            Self::Other => false,
        }
    }

    fn into_filenames(self) -> Vec<PathBuf> {
        match self {
            Self::CompilerArtifact { filenames, .. } => filenames,
            Self::Other => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{BindingMetadataSurface, Decl, Native, Wasm32};

    use super::{BindingMetadataBuild, BindingMetadataBuildError, CargoBuild, MetadataCargoArgs};
    use crate::cargo::LibraryCargoArgsError;

    #[test]
    fn metadata_build_tracks_rustup_toolchain_selector_separately() {
        let build = BindingMetadataBuild::new("Cargo.toml")
            .rustup_toolchain("+nightly")
            .cargo_args(vec![
                "+nightly".to_string(),
                "--features".to_string(),
                "ffi".to_string(),
            ]);

        assert_eq!(build.toolchain_selector.as_deref(), Some("+nightly"));
        assert_eq!(
            build.cargo_args,
            MetadataCargoArgs::new(vec!["--features".to_string(), "ffi".to_string()])
        );
    }

    #[test]
    fn cargo_build_applies_target_toolchain_arguments_and_cross_linker_environment() {
        let build = BindingMetadataBuild::new("/workspace/ffi/Cargo.toml")
            .target("x86_64-unknown-linux-gnu")
            .cargo_args(["--features".to_string(), "ffi".to_string()])
            .cargo_environment([(
                OsString::from("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"),
                OsString::from("/opt/cross/bin/clang"),
            )])
            .rustup_toolchain("+nightly");
        let cargo_args = build.cargo_args.as_ref().unwrap();
        let command = CargoBuild::new(&build, cargo_args, Some(Path::new("/workspace/target")))
            .plain_command();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let environment = command.get_envs().collect::<Vec<_>>();

        assert_eq!(arguments.first().map(String::as_str), Some("+nightly"));
        assert!(
            arguments
                .windows(2)
                .any(|arguments| { arguments == ["--target", "x86_64-unknown-linux-gnu"] })
        );
        assert!(
            arguments
                .windows(2)
                .any(|arguments| { arguments == ["--features", "ffi"] })
        );
        assert!(environment.iter().any(|(key, value)| {
            *key == OsStr::new("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER")
                && *value == Some(OsStr::new("/opt/cross/bin/clang"))
        }));
    }

    #[test]
    fn cargo_build_selects_wasm_bindings_without_cross_compiling_the_metadata_artifact() {
        let build = BindingMetadataBuild::new("/workspace/ffi/Cargo.toml")
            .surface(BindingMetadataSurface::Wasm32);
        let cargo_args = build.cargo_args.as_ref().unwrap();
        let command = CargoBuild::new(&build, cargo_args, Some(Path::new("/workspace/target")))
            .plain_command();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(!arguments.iter().any(|argument| argument == "--target"));
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "boltffi_binding_surface_wasm32")
        );
    }

    #[test]
    fn metadata_cargo_args_strip_rustup_toolchain_selectors() {
        assert_eq!(
            MetadataCargoArgs::new(vec![
                "+nightly".to_string(),
                "--features".to_string(),
                "ffi".to_string(),
            ]),
            MetadataCargoArgs::new(vec!["--features".to_string(), "ffi".to_string()])
        );
    }

    #[test]
    fn metadata_build_rejects_incompatible_library_arguments_before_manifest_access() {
        let error = BindingMetadataBuild::new("/missing/Cargo.toml")
            .cargo_args(["--workspace".to_string()])
            .read_source()
            .expect_err("workspace selection must fail before Cargo");

        assert!(matches!(
            error,
            BindingMetadataBuildError::CargoArguments(
                LibraryCargoArgsError::PackageSet { argument }
            ) if argument == "--workspace"
        ));
    }

    #[test]
    fn cargo_build_combines_source_records_across_dependency_artifacts() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_source_record_dependency();

        let source = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect("cargo source metadata read");

        assert_eq!(
            source.source_records.len(),
            2,
            "root and dependency records both surface"
        );
        assert_eq!(
            source.package,
            PackageInfo::new("metadata_fixture", Some("0.0.0".to_owned()))
        );

        let contract =
            boltffi_binding::aggregate_records(&source.source_records, source.package.clone())
                .expect("source records aggregate");
        assert_eq!(
            contract.records.len(),
            1,
            "dependency record joins the contract"
        );
        assert_eq!(
            contract.records[0].id.as_str(),
            "metadata_dependency::Point",
            "dependency identity comes from its own module path"
        );
        assert_eq!(
            contract.functions.len(),
            1,
            "root function joins the contract"
        );

        let bindings =
            boltffi_binding::lower::<Native>(&contract).expect("aggregated contract lowers");
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Record(_)))
                .count(),
            1
        );
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Function(_)))
                .count(),
            1
        );
    }

    #[test]
    fn cargo_build_refuses_a_shadowed_builtin() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_shadowed_builtin();

        let error = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect_err("a declared Duration shadows the builtin spelling");
        assert!(
            error
                .to_string()
                .contains("a declared type named `Duration` shadows the builtin `Duration`"),
            "the declaration is refused where it is written, got: {error}"
        );
    }

    #[test]
    fn conditional_members_capture_only_what_compiles() {
        if cfg!(miri) {
            return;
        }
        let fixture = FixtureCrate::write(
            Source {
                code: r#"
boltffi::scaffolding!();
pub struct Engine;
#[boltffi::export]
impl Engine {
    pub fn new() -> Self { Self }
    pub fn value(&self) -> u32 { 42 }
    #[cfg(any())]
    pub fn unavailable(&self, value: UnavailableType) {}
}
"#
                .to_owned(),
            },
            Dependency::Boltffi,
        );
        let source = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect("valid Rust must compile with capture");
        boltffi_binding::aggregate_records(&source.source_records, source.package)
            .expect("configuration is evaluated before capture");
        let generated = crate::generate::Generation::new(fixture.manifest())
            .render(crate::target::Target::Swift)
            .expect("the active surface generates");
        assert!(
            generated
                .files()
                .iter()
                .any(|file| file.contents().contains("Engine"))
        );
        assert!(
            generated
                .files()
                .iter()
                .all(|file| !file.contents().contains("unavailable"))
        );
    }

    #[test]
    fn macro_generated_and_included_exports_are_declared_and_exported() {
        if cfg!(miri) {
            return;
        }
        let fixture = FixtureCrate::write(
            Source {
                code: r#"
boltffi::scaffolding!();
#[boltffi::export]
pub fn plain_add(a: i32, b: i32) -> i32 { a + b }

pub struct Counter;

#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Point { pub x: f64 }

macro_rules! exported {
    ($function:ident, $class:ident, $record:ident) => {
        #[boltffi::export]
        pub fn $function(a: i32, b: i32) -> i32 { a * b }

        #[boltffi::export]
        impl $class {
            pub fn new() -> Self { Self }
        }

        #[boltffi::data(impl)]
        impl $record {
            pub fn doubled(&self) -> f64 { self.x * 2.0 }
        }
    };
}
exported!(generated_mul, Counter, Point);

include!("included.rs");
"#
                .to_owned(),
            },
            Dependency::Boltffi,
        );
        fs::write(
            fixture.root.join("src").join("included.rs"),
            "#[boltffi::export]\npub fn included_mul(a: i32, b: i32) -> i32 { a * b }\n",
        )
        .expect("write included source");
        let source = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect("valid Rust must compile with capture");
        let unsupported = source
            .source_records
            .iter()
            .filter(|record| {
                matches!(
                    serde_json::from_slice(&record.json),
                    Ok(boltffi_binding::SourceFragment::Unsupported { .. })
                )
            })
            .count();
        assert_eq!(
            unsupported, 0,
            "every invocation is captured where it expands"
        );
        let swift = crate::generate::Generation::new(fixture.manifest())
            .render(crate::target::Target::Swift)
            .expect("generated and included exports generate")
            .files()
            .iter()
            .map(|file| file.contents().to_owned())
            .collect::<String>();
        let artifacts = source
            .artifacts
            .iter()
            .map(|path| fs::read(path).expect("read built artifact"))
            .collect::<Vec<_>>();
        for (name, symbol) in [
            ("plainAdd", "boltffi_function_metadata_fixture_plain_add"),
            (
                "generatedMul",
                "boltffi_function_metadata_fixture_generated_mul",
            ),
            (
                "includedMul",
                "boltffi_function_metadata_fixture_included_mul",
            ),
            ("Counter", "boltffi_init_class_metadata_fixture_counter_new"),
            (
                "doubled",
                "boltffi_method_record_metadata_fixture_point_doubled",
            ),
        ] {
            assert!(swift.contains(name), "`{name}` is declared");
            assert!(
                artifacts.iter().any(|bytes| bytes
                    .windows(symbol.len())
                    .any(|window| window == symbol.as_bytes())),
                "`{symbol}` is exported"
            );
        }
    }

    fn assert_declared_and_exported(
        fixture: &FixtureCrate,
        cargo_args: &[&str],
        present: &[(&str, &str)],
        absent: &[(&str, &str)],
    ) {
        let cargo_args = cargo_args
            .iter()
            .map(|arg| (*arg).to_owned())
            .collect::<Vec<_>>();
        let source = BindingMetadataBuild::new(fixture.manifest())
            .cargo_args(cargo_args.clone())
            .read_source()
            .expect("fixture compiles");
        let swift = crate::generate::Generation::new(fixture.manifest())
            .cargo_args(cargo_args)
            .render(crate::target::Target::Swift)
            .expect("fixture generates")
            .files()
            .iter()
            .map(|file| file.contents().to_owned())
            .collect::<String>();
        let artifacts = source
            .artifacts
            .iter()
            .map(|path| fs::read(path).expect("read built artifact"))
            .collect::<Vec<_>>();
        let exported = |symbol: &str| {
            artifacts.iter().any(|bytes| {
                bytes
                    .windows(symbol.len())
                    .any(|window| window == symbol.as_bytes())
            })
        };
        for (name, symbol) in present {
            assert!(swift.contains(name), "`{name}` is declared");
            assert!(exported(symbol), "`{symbol}` is exported");
        }
        for (name, symbol) in absent {
            assert!(!swift.contains(name), "`{name}` is not declared");
            assert!(!exported(symbol), "`{symbol}` is not exported");
        }
    }

    #[test]
    fn lanes_resolve_every_way_a_type_is_named() {
        if cfg!(miri) {
            return;
        }
        let fixture = FixtureCrate::write(
            Source {
                code: r#"
boltffi::scaffolding!();

pub mod api {
    use super::geo::*;
    use super::geo::Route as Journey;
    use boltffi::export;

    #[export]
    pub fn early_x(point: Point) -> f64 { point.x }

    #[export]
    pub fn journey_name(journey: &Journey) -> String { journey.name.clone() }

    #[export]
    pub fn stop_label(stop: super::geo::inner::Waypoint) -> String { stop.label }
}

pub mod geo {
    use boltffi::data;

    #[data]
    #[derive(Clone, Debug)]
    pub struct Route { pub name: String, pub from: Point, pub stops: Vec<inner::Waypoint> }

    #[data]
    #[derive(Clone, Copy, Debug)]
    pub struct Point { pub x: f64, pub y: f64 }

    pub mod inner {
        #[boltffi::data]
        #[derive(Clone, Debug)]
        pub struct Waypoint { pub at: super::Point, pub label: String }
    }
}

pub struct Counter { value: u32 }

#[boltffi::export]
impl Counter {
    pub fn new() -> Counter { Counter { value: 1 } }
    pub fn offset_x(&self, point: geo::Point) -> f64 { point.x + f64::from(self.value) }
}

pub mod clock {
    pub struct Stamp(pub i64);

    boltffi::custom_type!(
        Stamp,
        remote = Stamp,
        repr = i64,
        into_ffi = |stamp: &Stamp| stamp.0,
        try_from_ffi = |value: i64| Ok::<_, boltffi::CustomTypeConversionError>(Stamp(value)),
    );

    boltffi::custom_type!(
        pub Positive,
        remote = std::num::NonZeroU32,
        repr = u32,
        into_ffi = |value: &std::num::NonZeroU32| value.get(),
        try_from_ffi = |value: u32| std::num::NonZeroU32::new(value).ok_or(boltffi::CustomTypeConversionError),
    );
}

#[boltffi::export]
pub fn stamp_value(stamp: clock::Stamp) -> i64 { stamp.0 }

#[boltffi::export]
pub fn positive_value(value: clock::Positive) -> u32 { value.get() }

pub fn local_types() -> u32 {
    #[boltffi::data]
    #[derive(Clone, Copy)]
    struct Inner { value: u32 }

    #[boltffi::data]
    #[derive(Clone)]
    struct Outer { inner: Inner }

    Outer { inner: Inner { value: 3 } }.inner.value
}
"#
                .to_owned(),
            },
            Dependency::Boltffi,
        );
        assert_declared_and_exported(
            &fixture,
            &[],
            &[
                ("earlyX", "boltffi_function_metadata_fixture_early_x"),
                (
                    "journeyName",
                    "boltffi_function_metadata_fixture_journey_name",
                ),
                ("stopLabel", "boltffi_function_metadata_fixture_stop_label"),
                (
                    "offsetX",
                    "boltffi_method_class_metadata_fixture_counter_offset_x",
                ),
                ("Counter", "boltffi_init_class_metadata_fixture_counter_new"),
                (
                    "stampValue",
                    "boltffi_function_metadata_fixture_stamp_value",
                ),
                (
                    "positiveValue",
                    "boltffi_function_metadata_fixture_positive_value",
                ),
            ],
            &[],
        );
    }

    #[test]
    fn configuration_decides_members_on_both_sides() {
        if cfg!(miri) {
            return;
        }
        let fixture = FixtureCrate::write(
            Source {
                code: r#"
boltffi::scaffolding!();

#[cfg(feature = "extra")]
#[boltffi::data]
#[derive(Clone, Copy)]
pub struct OnlyExtra { pub level: u32 }

#[boltffi::data]
#[derive(Clone)]
pub struct Settings {
    pub name: String,
    #[cfg(feature = "extra")]
    pub extra_field: OnlyExtra,
    #[cfg(not(feature = "extra"))]
    pub fallback_field: u8,
}

#[boltffi::data]
#[derive(Clone)]
pub enum Mode {
    Plain,
    #[cfg(feature = "extra")]
    Boosted(OnlyExtra),
}

pub struct Engine;

#[boltffi::export]
impl Engine {
    pub fn new() -> Engine { Engine }
    #[cfg(feature = "extra")]
    pub fn boost(&self, extra: OnlyExtra) -> u32 { extra.level }
    #[cfg_attr(not(feature = "extra"), boltffi::skip)]
    pub fn describe(&self) -> String { String::new() }
}

#[boltffi::export]
pub trait Listener {
    fn on_event(&self, value: u32);
    #[cfg(feature = "extra")]
    fn on_extra(&self, extra: OnlyExtra);
}

#[boltffi::export]
#[cfg(feature = "extra")]
pub fn make_extra(level: u32) -> OnlyExtra { OnlyExtra { level } }

#[boltffi::export]
pub fn settings_name(settings: Settings) -> String { settings.name }

#[boltffi::export]
pub fn mode_name(mode: Mode) -> String { String::from(match mode { Mode::Plain => "plain", #[cfg(feature = "extra")] Mode::Boosted(_) => "boosted" }) }
"#
                .to_owned(),
            },
            Dependency::Boltffi,
        );
        let mut manifest = fs::read_to_string(fixture.manifest()).expect("read fixture manifest");
        manifest.push_str("\n[features]\nextra = []\n");
        fs::write(fixture.manifest(), manifest).expect("write fixture features");
        let extra = [
            (
                "boost",
                "boltffi_method_class_metadata_fixture_engine_boost",
            ),
            (
                "describe",
                "boltffi_method_class_metadata_fixture_engine_describe",
            ),
            ("makeExtra", "boltffi_function_metadata_fixture_make_extra"),
            (
                "onExtra",
                "boltffi_register_callback_metadata_fixture_listener",
            ),
            (
                "extraField",
                "boltffi_function_metadata_fixture_settings_name",
            ),
            ("boosted", "boltffi_function_metadata_fixture_mode_name"),
        ];
        assert_declared_and_exported(
            &fixture,
            &["--features", "extra"],
            &extra,
            &[("fallbackField", "boltffi_no_such_symbol")],
        );
        assert_declared_and_exported(
            &fixture,
            &[],
            &[
                (
                    "fallbackField",
                    "boltffi_function_metadata_fixture_settings_name",
                ),
                (
                    "onEvent",
                    "boltffi_register_callback_metadata_fixture_listener",
                ),
            ],
            &[
                (
                    "func boost",
                    "boltffi_method_class_metadata_fixture_engine_boost",
                ),
                (
                    "func describe",
                    "boltffi_method_class_metadata_fixture_engine_describe",
                ),
                ("makeExtra", "boltffi_function_metadata_fixture_make_extra"),
                ("onExtra", "boltffi_no_such_symbol"),
                ("extraField", "boltffi_no_such_symbol"),
                ("boosted", "boltffi_no_such_symbol"),
            ],
        );
    }

    #[test]
    fn generation_reads_a_minimal_crate_from_its_source_records() {
        if cfg!(miri) {
            return;
        }
        let fixture = FixtureCrate::write(
            Source {
                code: r#"
boltffi::scaffolding!();
#[boltffi::export]
pub fn captured_value() -> u32 { 42 }
"#
                .to_owned(),
            },
            Dependency::Boltffi,
        );
        let generated = crate::generate::Generation::new(fixture.manifest())
            .render(crate::target::Target::Swift)
            .expect("a crate with one export generates");
        assert!(
            generated
                .files()
                .iter()
                .any(|file| file.contents().contains("capturedValue"))
        );
    }

    #[test]
    fn cargo_build_aggregates_macro_emitted_source_records() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_boltffi_macros();

        let source = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect("cargo source metadata read");

        let contract =
            boltffi_binding::aggregate_records(&source.source_records, source.package.clone())
                .expect("macro-emitted records aggregate");
        assert_eq!(contract.records.len(), 1);
        assert_eq!(
            contract.records[0].id.as_str(),
            "metadata_fixture::Point",
            "the record's identity comes from its defining module"
        );
        assert_eq!(contract.functions.len(), 8);
        let browser_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::browser_len")
            .expect("the interned-string function is captured");
        assert!(
            matches!(
                &browser_fn.parameters[0].type_expr,
                boltffi_ast::TypeExpr::InternedString {
                    pool_id,
                    static_values,
                    ..
                } if pool_id == "metadata_fixture::Browser"
                    && static_values == &["Chrome".to_owned(), "Firefox".to_owned()]
            ),
            "the pool's values inline at the use site through its fragment"
        );
        assert!(
            contract
                .functions
                .iter()
                .any(|function| function.id.as_str() == "metadata_fixture::origin")
        );
        assert_eq!(
            contract.enums.len(),
            2,
            "the error and data enums are captured"
        );
        let shift_error = contract
            .enums
            .iter()
            .find(|declared| declared.id.as_str() == "metadata_fixture::ShiftError")
            .expect("the error enum is captured");
        assert!(
            shift_error.user_attrs.iter().any(|attr| {
                attr.path
                    .last()
                    .is_some_and(|segment| segment.name.as_str() == "error")
            }),
            "the error marker rides the captured enum"
        );
        let shift_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::checked_shift")
            .expect("the fallible function is captured");
        assert!(
            matches!(
                &shift_fn.returns,
                boltffi_ast::ReturnDef::Value(boltffi_ast::TypeExpr::Result { err, .. })
                    if matches!(
                        err.as_ref(),
                        boltffi_ast::TypeExpr::Enum { id, .. }
                            if id.as_str() == "metadata_fixture::ShiftError"
                    )
            ),
            "the error reference classifies as an enum through its defining fragment"
        );
        assert_eq!(contract.classes.len(), 1, "the class impl is captured");
        assert_eq!(contract.classes[0].id.as_str(), "metadata_fixture::Session");
        assert_eq!(contract.classes[0].methods.len(), 2);
        assert!(
            matches!(
                &contract.classes[0].methods[1].returns,
                boltffi_ast::ReturnDef::Value(boltffi_ast::TypeExpr::Record { id, .. })
                    if id.as_str() == "metadata_fixture::Point"
            ),
            "class method references resolve through the compiler"
        );
        assert_eq!(contract.streams.len(), 1, "the stream method is captured");
        assert_eq!(
            contract.streams[0].id.as_str(),
            "metadata_fixture::Session::moves"
        );
        assert_eq!(
            contract.streams[0]
                .owner
                .as_ref()
                .map(|owner| owner.as_str()),
            Some("metadata_fixture::Session")
        );
        assert!(
            matches!(
                &contract.streams[0].item_type,
                boltffi_ast::TypeExpr::Record { id, .. }
                    if id.as_str() == "metadata_fixture::Point"
            ),
            "the stream item resolves cross-module through the compiler"
        );
        assert_eq!(
            contract.constants.len(),
            5,
            "the free constant and the associated constants are captured"
        );
        let origin_const = contract
            .constants
            .iter()
            .find(|constant| constant.id.as_str() == "metadata_fixture::Point::ORIGIN")
            .expect("the record's associated constant is captured");
        assert!(
            matches!(
                &origin_const.owner,
                Some(boltffi_ast::ConstantOwner::Record(id))
                    if id.as_str() == "metadata_fixture::Point"
            ),
            "the associated constant's owner resolves through the target"
        );
        assert!(
            contract
                .constants
                .iter()
                .any(|constant| constant.id.as_str() == "metadata_fixture::Session::MAX_SHIFT"),
            "the class's associated constant is captured"
        );
        assert_eq!(
            contract.customs.len(),
            3,
            "all three custom types are captured"
        );
        assert!(
            contract
                .customs
                .iter()
                .any(|custom| custom.id.as_str() == "metadata_fixture::Stamp")
        );
        assert!(
            contract
                .customs
                .iter()
                .any(|custom| custom.id.as_str() == "metadata_fixture::Label"),
            "the custom_ffi impl is captured"
        );
        let label_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::label_text")
            .expect("the custom_ffi-typed function is captured");
        assert!(
            matches!(
                &label_fn.parameters[0].type_expr,
                boltffi_ast::TypeExpr::Custom { id, .. }
                    if id.as_str() == "metadata_fixture::Label"
            ),
            "the custom_ffi reference classifies through its defining fragment"
        );
        assert_eq!(contract.traits.len(), 1, "the callback trait is captured");
        assert_eq!(contract.traits[0].id.as_str(), "metadata_fixture::Doubler");
        let doubler_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::apply_doubler")
            .expect("the impl-Trait function is captured");
        assert!(
            matches!(
                &doubler_fn.parameters[0].type_expr,
                boltffi_ast::TypeExpr::ImplTrait(bounds)
                    if matches!(
                        &bounds.base,
                        boltffi_ast::BaseTrait::Named { id, .. }
                            if id.as_str() == "metadata_fixture::Doubler"
                    )
            ),
            "the impl-Trait callback resolves through the trait's dyn identity"
        );
        let span_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::range_width")
            .expect("the generic-remote function is captured");
        assert!(
            matches!(
                &span_fn.parameters[0].type_expr,
                boltffi_ast::TypeExpr::Custom { id, .. }
                    if id.as_str() == "metadata_fixture::ByteSpan"
            ),
            "the generic remote classifies through its custom_type! fragment"
        );
        let stamp_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::stamp_value")
            .expect("the custom-typed function is captured");
        assert!(
            matches!(
                &stamp_fn.parameters[0].type_expr,
                boltffi_ast::TypeExpr::Custom { id, .. }
                    if id.as_str() == "metadata_fixture::Stamp"
            ),
            "the custom reference classifies through its defining fragment"
        );
        assert_eq!(
            contract.records[0].methods.len(),
            1,
            "the data(impl) block merges into its record"
        );
        assert_eq!(
            contract.records[0].methods[0].id.as_str(),
            "metadata_fixture::Point::doubled"
        );
        let origin_fn = contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == "metadata_fixture::origin")
            .expect("origin is captured");
        let boltffi_ast::ReturnDef::Value(returned) = &origin_fn.returns else {
            panic!("origin returns a value");
        };
        assert!(
            matches!(
                returned,
                boltffi_ast::TypeExpr::Record { id, .. }
                    if id.as_str() == "metadata_fixture::Point"
            ),
            "the cross-module reference resolves through the compiler: {returned:?}"
        );

        let bindings =
            boltffi_binding::lower::<Native>(&contract).expect("aggregated contract lowers");
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Record(_) | Decl::Function(_)))
                .count(),
            9,
            "the point record and all eight functions lower"
        );
    }

    #[test]
    fn cargo_build_source_records_lower_equivalently_to_the_legacy_scan() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_boltffi_macros();

        let source = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect("cargo source metadata read");
        let contract =
            boltffi_binding::aggregate_records(&source.source_records, source.package.clone())
                .expect("records aggregate");
        let actual =
            boltffi_binding::lower::<Native>(&contract).expect("aggregated contract lowers");

        let lib_rs = fixture
            .manifest()
            .parent()
            .expect("fixture manifest has a directory")
            .join("src/lib.rs");
        let mut scanned = boltffi_scan::crate_scoped(
            boltffi_scan::scan_source(&lib_rs, source.package.clone())
                .expect("legacy scan reads the fixture source"),
        );
        scanned.records.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.enums.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.functions.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.classes.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.traits.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.streams.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.constants.sort_by(|a, b| a.id.cmp(&b.id));
        scanned.customs.sort_by(|a, b| a.id.cmp(&b.id));
        let expected = boltffi_binding::lower::<Native>(&scanned).expect("scanned contract lowers");

        assert_eq!(
            actual.decls(),
            expected.decls(),
            "records-fed lowering matches the legacy scanner's declarations"
        );

        let actual_wasm = boltffi_binding::lower::<Wasm32>(&contract)
            .expect("aggregated contract lowers to wasm32");
        let expected_wasm =
            boltffi_binding::lower::<Wasm32>(&scanned).expect("scanned contract lowers to wasm32");
        assert_eq!(
            actual_wasm.decls(),
            expected_wasm.decls(),
            "the same host-built records serve the wasm32 surface without cross-compiling"
        );
    }

    #[test]
    fn cargo_build_reads_source_records_from_a_wasm32_target_build() {
        if cfg!(miri) {
            return;
        }
        if !wasm32_target_installed() {
            eprintln!("skipping: the wasm32-unknown-unknown target is not installed");
            return;
        }

        let fixture = FixtureCrate::with_boltffi_macros();

        let source = BindingMetadataBuild::new(fixture.manifest())
            .target("wasm32-unknown-unknown")
            .read_source()
            .expect("wasm32 source metadata read");

        let contract =
            boltffi_binding::aggregate_records(&source.source_records, source.package.clone())
                .expect("wasm32-built records aggregate");
        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.functions.len(), 8);
        assert_eq!(contract.classes.len(), 1);
        assert_eq!(contract.streams.len(), 1);

        let bindings =
            boltffi_binding::lower::<Wasm32>(&contract).expect("wasm32-built contract lowers");
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Record(_) | Decl::Function(_)))
                .count(),
            9,
            "the point record and all eight functions lower from the wasm32 artifacts"
        );
    }

    fn source_bindings(build: BindingMetadataBuild) -> boltffi_binding::Bindings<Native> {
        let source = build.read_source().expect("cargo source metadata read");
        let contract = boltffi_binding::aggregate_records(&source.source_records, source.package)
            .expect("records aggregate");
        boltffi_binding::lower::<Native>(&contract).expect("aggregated contract lowers")
    }

    #[test]
    fn cargo_build_reads_macro_emitted_source_records() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_boltffi_macros();

        let bindings = source_bindings(BindingMetadataBuild::new(fixture.manifest()));
        assert_eq!(
            bindings.package().name().as_path_string(),
            "metadata_fixture"
        );
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Record(_)))
                .count(),
            1
        );
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Function(_)))
                .count(),
            8
        );
    }

    #[cfg(windows)]
    #[test]
    fn cargo_build_supports_nested_workspace_member_with_inherited_dependency() {
        if cfg!(miri) {
            return;
        }

        let fixture = NestedWorkspaceFixture::with_inherited_dependency();
        let source = BindingMetadataBuild::new(fixture.manifest())
            .read_source()
            .expect("nested workspace source records read");

        assert_eq!(source.package.name, "metadata_nested_fixture");
        assert_eq!(source.source_records.len(), 1);
    }

    #[test]
    fn cargo_build_reads_feature_gated_source_records() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_feature_gated_boltffi_macros();

        let bindings = source_bindings(
            BindingMetadataBuild::new(fixture.manifest())
                .cargo_args(["--features".to_owned(), "native-ffi".to_owned()]),
        );
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Record(_)))
                .count(),
            1
        );
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Function(_)))
                .count(),
            1
        );
    }

    #[test]
    fn generation_reports_a_crate_without_source_records() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::without_metadata();

        let error = crate::generate::Generation::new(fixture.manifest())
            .render(crate::target::Target::Swift)
            .expect_err("source records are required");

        assert!(matches!(
            error,
            crate::generate::GenerationError::NoSourceRecords
        ));
    }

    #[test]
    fn metadata_cargo_args_keep_build_flags_without_owned_selectors() {
        let args = MetadataCargoArgs::new(
            [
                "--features",
                "demo",
                "--manifest-path",
                "ignored/Cargo.toml",
                "--target=aarch64-apple-darwin",
                "--release",
                "--package=demo",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "--features".to_owned(),
                "demo".to_owned(),
                "--release".to_owned(),
                "--package=demo".to_owned(),
            ]
        );
    }

    struct FixtureCrate {
        root: PathBuf,
        manifest: PathBuf,
    }

    impl FixtureCrate {
        fn with_boltffi_macros() -> Self {
            Self::write(Source::with_boltffi_macros(), Dependency::Boltffi)
        }

        fn with_shadowed_builtin() -> Self {
            Self::write(Source::with_shadowed_builtin(), Dependency::Boltffi)
        }

        fn with_feature_gated_boltffi_macros() -> Self {
            let root = temp_root("boltffi-bindgen-cargo-metadata");
            let source_dir = root.join("src");
            let manifest = root.join("Cargo.toml");
            fs::create_dir_all(&source_dir).expect("create metadata fixture source dir");
            fs::write(
                &manifest,
                format!(
                    "[package]\nname = \"metadata_fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[features]\nnative-ffi = []\n\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                    toml_path(&workspace_crate("boltffi"))
                ),
            )
            .expect("write metadata fixture manifest");
            fs::write(
                source_dir.join("lib.rs"),
                "boltffi::scaffolding!();\n\n#[cfg(feature = \"native-ffi\")]\npub mod ffi;\n",
            )
            .expect("write metadata fixture lib");
            fs::write(
                source_dir.join("ffi.rs"),
                r#"
use boltffi::{data, export};

#[data]
#[derive(Clone, Copy)]
pub struct CoreFfi {
    pub value: u32,
}

#[export]
pub fn view() -> CoreFfi {
    CoreFfi { value: 7 }
}
"#,
            )
            .expect("write metadata fixture ffi");
            Self { root, manifest }
        }

        fn without_metadata() -> Self {
            Self::write(Source::without_metadata(), Dependency::None)
        }

        fn with_source_record_dependency() -> Self {
            Self::write(Source::with_root_source_record(), Dependency::SourceRecord)
        }

        fn write(source: Source, dependency: Dependency) -> Self {
            let root = temp_root("boltffi-bindgen-cargo-metadata");
            let source_dir = root.join("src");
            let manifest = root.join("Cargo.toml");
            fs::create_dir_all(&source_dir).expect("create metadata fixture source dir");
            fs::write(&manifest, dependency.root_manifest())
                .expect("write metadata fixture manifest");
            fs::write(source_dir.join("lib.rs"), source.into_string())
                .expect("write metadata fixture lib");
            dependency.write(&root);
            Self { root, manifest }
        }

        fn manifest(&self) -> PathBuf {
            self.manifest.clone()
        }
    }

    #[cfg(windows)]
    struct NestedWorkspaceFixture {
        root: PathBuf,
        manifest: PathBuf,
    }

    #[cfg(windows)]
    impl NestedWorkspaceFixture {
        fn with_inherited_dependency() -> Self {
            let root = temp_root("boltffi-bindgen-nested-workspace");
            let member = root.join("crates").join("nested").join("member");
            let dependency = root.join("shared");
            let source_dir = member.join("src");
            let dependency_source_dir = dependency.join("src");
            let manifest = member.join("Cargo.toml");
            fs::create_dir_all(&source_dir).expect("create nested workspace source dir");
            fs::create_dir_all(&dependency_source_dir)
                .expect("create nested workspace dependency source dir");
            fs::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers = [\"crates/nested/member\", \"shared\"]\nresolver = \"3\"\n\n[workspace.dependencies]\nnested_dependency = { path = \"shared\" }\n",
            )
            .expect("write nested workspace manifest");
            fs::write(
                &manifest,
                "[package]\nname = \"metadata_nested_fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\nnested_dependency = { workspace = true }\n",
            )
            .expect("write nested workspace member manifest");
            fs::write(
                source_dir.join("lib.rs"),
                Source::with_source_record_static(
                    "metadata_nested_fixture",
                    &[],
                    b"{}",
                    "pub fn exported() -> u32 { nested_dependency::value() }\n",
                )
                .into_string(),
            )
            .expect("write nested workspace member source");
            fs::write(
                dependency.join("Cargo.toml"),
                "[package]\nname = \"nested_dependency\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
            )
            .expect("write nested workspace dependency manifest");
            fs::write(
                dependency_source_dir.join("lib.rs"),
                "pub fn value() -> u32 { 7 }\n",
            )
            .expect("write nested workspace dependency source");
            Self { root, manifest }
        }

        fn manifest(&self) -> PathBuf {
            self.manifest.clone()
        }
    }

    #[cfg(windows)]
    impl Drop for NestedWorkspaceFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    impl Drop for FixtureCrate {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    enum Dependency {
        Boltffi,
        SourceRecord,
        None,
    }

    impl Dependency {
        fn root_manifest(&self) -> String {
            let dependency = match self {
                Self::Boltffi => format!(
                    "\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                    toml_path(&workspace_crate("boltffi"))
                ),
                Self::SourceRecord => {
                    "\n[dependencies]\nmetadata_dependency = { path = \"metadata_dependency\" }\n"
                        .to_owned()
                }
                Self::None => String::new(),
            };
            format!(
                "[package]\nname = \"metadata_fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n{dependency}"
            )
        }

        fn write(self, root: &Path) {
            let body = match self {
                Self::SourceRecord => Source::with_dependency_source_record().into_string(),
                Self::Boltffi | Self::None => return,
            };
            let package = root.join("metadata_dependency");
            let source = package.join("src");
            fs::create_dir_all(&source).expect("create metadata dependency source dir");
            fs::write(
                package.join("Cargo.toml"),
                "[package]\nname = \"metadata_dependency\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
            )
            .expect("write metadata dependency manifest");
            fs::write(source.join("lib.rs"), body).expect("write metadata dependency lib");
        }
    }

    struct Source {
        code: String,
    }

    impl Source {
        fn with_shadowed_builtin() -> Self {
            Self {
                code: r#"
boltffi::scaffolding!();

pub mod timing {
    use boltffi::data;

    #[data]
    #[derive(Clone, Copy)]
    pub struct Duration {
        pub millis: u64,
    }

    #[data]
    #[derive(Clone, Copy)]
    pub struct Sample {
        pub own: Duration,
        pub wall: std::time::Duration,
    }
}
"#
                .to_owned(),
            }
        }

        fn with_boltffi_macros() -> Self {
            Self {
                code: r#"
boltffi::scaffolding!();

pub mod domain {
    use boltffi::data;

    #[data]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub x: f64,
    }

    #[boltffi::data(impl)]
    impl Point {
        pub const ORIGIN: Point = Point { x: 0.0 };
        pub const UNIT: Self = Point { x: 1.0 };
        #[allow(dead_code)]
        const PRIVATE: f64 = 0.5;
        #[boltffi::skip]
        pub const HIDDEN: f64 = 0.25;

        pub fn doubled(&self) -> Point {
            Point { x: self.x * 2.0 }
        }
    }

    #[data]
    #[derive(Clone, Copy)]
    pub enum Mode {
        Fast,
        Slow,
    }

    #[boltffi::data(impl)]
    impl Mode {
        pub const DEFAULT: Self = Mode::Fast;
    }
}

pub mod clock {
    use std::ops::Range;

    use boltffi::{CustomFfiConvertible, custom_ffi, custom_type};

    pub struct Stamp(pub i64);

    custom_type!(
        ByteSpan,
        remote = Range<u32>,
        repr = u64,
        into_ffi = |span: &Range<u32>| (u64::from(span.start) << 32) | u64::from(span.end),
        try_from_ffi = |bits: u64| {
            Ok::<_, boltffi::CustomTypeConversionError>((bits >> 32) as u32..bits as u32)
        },
    );

    #[boltffi::export]
    pub fn range_width(span: ByteSpan) -> u32 {
        span.end - span.start
    }

    custom_type!(
        Stamp,
        remote = Stamp,
        repr = i64,
        into_ffi = |stamp: &Stamp| stamp.0,
        try_from_ffi = |value: i64| Ok::<_, boltffi::CustomTypeConversionError>(Stamp(value)),
    );

    pub struct Label(pub String);

    #[custom_ffi]
    impl CustomFfiConvertible for Label {
        type FfiRepr = String;
        type Error = String;

        fn into_ffi(&self) -> String {
            self.0.clone()
        }

        fn try_from_ffi(repr: String) -> Result<Self, String> {
            Ok(Label(repr))
        }
    }
}

pub mod pools {
    boltffi::interned_string_pool! {
        pub Browser {
            CHROME = "Chrome",
            FIREFOX = "Firefox",
        }
    }
}

pub mod api {
    use std::sync::Arc;

    use boltffi::{EventSubscription, InternedString, export};

    use crate::clock::{Label, Stamp};
    use crate::domain::Point;
    use crate::pools::Browser;

    #[export]
    pub fn origin() -> Point {
        Point { x: 0.0 }
    }

    #[export]
    pub fn label_text(label: Label) -> String {
        label.0
    }

    #[export]
    pub trait Doubler {
        fn double(&self, value: i32) -> i32;
    }

    #[export]
    pub fn apply_doubler(doubler: impl Doubler, value: i32) -> i32 {
        doubler.double(value)
    }

    #[export]
    pub fn apply_boxed_doubler(doubler: Box<dyn Doubler>, value: i32) -> i32 {
        doubler.double(value)
    }

    #[export]
    pub const LIMIT: u32 = 8;

    #[export]
    pub fn stamp_value(stamp: Stamp) -> i64 {
        stamp.0
    }

    #[export]
    pub fn browser_len(name: InternedString<Browser>) -> u32 {
        match name.repr() {
            boltffi::InternedStringRepr::Interned(id) => *id,
            boltffi::InternedStringRepr::Dynamic(value) => value.len() as u32,
        }
    }

    #[boltffi::error]
    #[derive(Clone, Debug)]
    pub enum ShiftError {
        OutOfRange,
    }

    impl std::fmt::Display for ShiftError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "out of range")
        }
    }

    impl std::error::Error for ShiftError {}

    #[export]
    pub fn checked_shift(point: Point, by: f64) -> Result<Point, ShiftError> {
        if by.is_finite() {
            Ok(Point { x: point.x + by })
        } else {
            Err(ShiftError::OutOfRange)
        }
    }

    pub struct Session {
        origin: Point,
    }

    #[export(single_threaded)]
    impl Session {
        pub const MAX_SHIFT: f64 = 9.0;

        pub fn new() -> Self {
            Self { origin: Point { x: 0.0 } }
        }

        pub fn shift(&self, by: f64) -> Point {
            Point { x: self.origin.x + by }
        }

        #[boltffi::ffi_stream(item = Point)]
        pub fn moves(&self) -> Arc<EventSubscription<Point>> {
            todo!()
        }
    }
}
"#
                .to_owned(),
            }
        }

        fn without_metadata() -> Self {
            Self {
                code: "pub fn exported() -> u32 { 1 }\n".to_owned(),
            }
        }

        fn with_root_source_record() -> Self {
            let mut function = boltffi_ast::FunctionDef::new(
                boltffi_ast::FunctionId::new("$self::origin"),
                source_name("origin"),
            );
            function.returns = boltffi_ast::ReturnDef::value(boltffi_ast::TypeExpr::record(
                boltffi_ast::RecordId::new("$slot:0"),
                boltffi_ast::Path::single("Point"),
            ));
            let json = serde_json::to_vec(&boltffi_binding::SourceFragment::Function(function))
                .expect("function fragment serializes");
            Self::with_source_record_static(
                "metadata_fixture",
                &[r#"{"id":"metadata_dependency::Point"}"#],
                &json,
                "pub fn exported() -> u32 { metadata_dependency::value() }\n",
            )
        }

        fn with_dependency_source_record() -> Self {
            let mut record = boltffi_ast::RecordDef::new(
                boltffi_ast::RecordId::new("$self::Point"),
                source_name("Point"),
            );
            record.fields = vec![boltffi_ast::FieldDef::new(
                source_name("x"),
                boltffi_ast::TypeExpr::Primitive(boltffi_ast::Primitive::F64),
            )];
            let json = serde_json::to_vec(&boltffi_binding::SourceFragment::Record(record))
                .expect("record fragment serializes");
            Self::with_source_record_static(
                "metadata_dependency",
                &[],
                &json,
                "pub fn value() -> u32 { 7 }\n",
            )
        }

        fn with_source_record_static(
            module: &str,
            slots: &[&str],
            json: &[u8],
            body: &str,
        ) -> Self {
            let record_bytes = source_record_bytes(module, "0.0.0", module, slots, json);
            let length = record_bytes.len();
            let bytes = record_bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Self {
                code: format!(
                    "#[cfg_attr(target_vendor = \"apple\", unsafe(link_section = \"__DATA,__boltffisrc\"))]\n#[cfg_attr(not(target_vendor = \"apple\"), unsafe(link_section = \".boltffisrc\"))]\n#[used]\nstatic BOLTFFI_SOURCE: [u8; {length}] = [{bytes}];\n{body}"
                ),
            }
        }

        fn into_string(self) -> String {
            self.code
        }
    }

    fn source_name(spelling: &str) -> boltffi_ast::SourceName {
        boltffi_ast::SourceName::new(
            spelling,
            boltffi_ast::CanonicalName::single(spelling.to_lowercase()),
        )
    }

    fn source_record_bytes(
        package: &str,
        version: &str,
        module: &str,
        slots: &[&str],
        json: &[u8],
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        for field in [package, version, module] {
            payload.extend_from_slice(&(field.len() as u16).to_le_bytes());
            payload.extend_from_slice(field.as_bytes());
        }
        payload.extend_from_slice(&(slots.len() as u16).to_le_bytes());
        for slot in slots {
            payload.extend_from_slice(&(slot.len() as u16).to_le_bytes());
            payload.extend_from_slice(slot.as_bytes());
        }
        payload.extend_from_slice(&(json.len() as u32).to_le_bytes());
        payload.extend_from_slice(json);

        let mut bytes = b"BFFISRC1".to_vec();
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&payload);
        bytes
    }

    fn wasm32_target_installed() -> bool {
        std::process::Command::new("rustc")
            .args([
                "--print",
                "target-libdir",
                "--target",
                "wasm32-unknown-unknown",
            ])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
            .is_some_and(|libdir| {
                fs::read_dir(libdir).is_ok_and(|mut entries| entries.next().is_some())
            })
    }

    fn workspace_crate(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join(name)
    }

    fn toml_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn temp_root(prefix: &str) -> PathBuf {
        static TEMP_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos(),
            TEMP_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
