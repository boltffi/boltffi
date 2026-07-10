//! Builds a Rust crate and reads embedded BoltFFI binding metadata.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use boltffi_binding::{
    BINDING_METADATA_BUILD_ENV, BINDING_METADATA_FEATURES_ENV, BINDING_METADATA_ROOT_ENV,
    BINDING_METADATA_SOURCE_ENV, BINDING_METADATA_SURFACE_ENV, BindingMetadataEnvelope,
    BindingMetadataSurface,
};
use boltffi_ffi_rules::cargo_graph::CargoFeatureSelection;
pub use boltffi_ffi_rules::cargo_graph::ResolvedFeatures;
use serde::Deserialize;
use serde_json::{Error as JsonError, from_slice, from_str};
use thiserror::Error;

use crate::artifact::{BindingMetadataReadError, BindingMetadataReader};

/// A Cargo library build that extracts embedded BoltFFI binding metadata.
///
/// The build enables the `boltffi_metadata` cfg and reads Cargo's JSON
/// artifact stream. Artifact decoding is delegated to
/// [`BindingMetadataReader`], so section framing and contract validation
/// stay on the same path used by direct artifact reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingMetadataBuild {
    manifest_path: PathBuf,
    target: Option<String>,
    toolchain_selector: Option<String>,
    cargo_args: MetadataCargoArgs,
}

impl BindingMetadataBuild {
    /// Creates a metadata build for a Cargo manifest.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            target: None,
            toolchain_selector: None,
            cargo_args: MetadataCargoArgs::default(),
        }
    }

    /// Builds for a Cargo target triple.
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
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

    /// Selects a rustup Cargo toolchain for metadata and build commands.
    pub fn rustup_toolchain(mut self, toolchain_selector: impl Into<String>) -> Self {
        self.toolchain_selector = Some(toolchain_selector.into());
        self
    }

    /// Runs Cargo and returns the validated metadata envelopes.
    pub fn read(&self) -> Result<Vec<BindingMetadataEnvelope>, BindingMetadataBuildError> {
        let manifest = CargoManifest::new(&self.manifest_path)?;
        let metadata = CargoMetadata::load(&manifest, self.toolchain_selector.as_deref())?;
        let source_root = SourceRoot::resolve(&metadata, &manifest)?;
        let features = metadata.active_features(&manifest, &self.cargo_args)?;
        let output = CargoBuild::new(self, &manifest, &source_root, features).output()?;
        let artifacts = output.artifacts(&manifest)?;
        BindingMetadataReader::new(artifacts.into_paths())
            .read_required()
            .map_err(BindingMetadataBuildError::Metadata)
    }
}

pub fn resolve_cargo_features(
    manifest_path: impl AsRef<Path>,
    cargo_args: impl IntoIterator<Item = String>,
    toolchain_selector: Option<&str>,
) -> Result<ResolvedFeatures, BindingMetadataBuildError> {
    let manifest = CargoManifest::new(manifest_path.as_ref())?;
    let cargo_args = cargo_args.into_iter().collect::<Vec<_>>();
    let toolchain_selector = toolchain_selector.map(str::to_owned).or_else(|| {
        cargo_args
            .iter()
            .find(|argument| is_rustup_toolchain_selector(argument))
            .cloned()
    });
    let cargo_args = MetadataCargoArgs::new(cargo_args);
    CargoMetadata::load(&manifest, toolchain_selector.as_deref())?
        .active_features(&manifest, &cargo_args)
}

/// Failure while building a crate for embedded binding metadata.
#[derive(Debug, Error)]
pub enum BindingMetadataBuildError {
    /// Cargo could not be started.
    #[error("run cargo build for binding metadata: {source}")]
    CargoSpawn {
        /// Process spawn error.
        source: std::io::Error,
    },
    /// Cargo returned a non-zero exit status.
    #[error("cargo build for binding metadata failed with status {status}: {stderr}")]
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
        source: JsonError,
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
    #[error("cargo build for `{manifest_path}` did not report compiled library artifacts")]
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
        fs::canonicalize(path)
            .map(|path| Self { path })
            .map_err(|source| BindingMetadataBuildError::ManifestPath {
                path: path.to_path_buf(),
                source,
            })
    }

    fn matches(&self, path: &Path) -> bool {
        fs::canonicalize(path).is_ok_and(|path| path == self.path)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceRoot {
    path: PathBuf,
}

impl SourceRoot {
    fn resolve(
        metadata: &CargoMetadata,
        manifest: &CargoManifest,
    ) -> Result<Self, BindingMetadataBuildError> {
        metadata.library_source(manifest).map(|path| Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
}

impl CargoMetadata {
    fn load(
        manifest: &CargoManifest,
        toolchain_selector: Option<&str>,
    ) -> Result<Self, BindingMetadataBuildError> {
        let mut command = Command::new(CargoProgram::from_env().into_os_string());
        if let Some(toolchain_selector) = toolchain_selector {
            command.arg(toolchain_selector);
        }
        command
            .arg("metadata")
            .arg("--format-version=1")
            .arg("--no-deps")
            .arg("--manifest-path")
            .arg(manifest.path());
        let output = command
            .output()
            .map_err(|source| BindingMetadataBuildError::CargoSpawn { source })?;
        if !output.status.success() {
            return Err(BindingMetadataBuildError::CargoFailed {
                status: CargoStatus::from_status(output.status),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        from_slice(&output.stdout).map_err(|source| BindingMetadataBuildError::CargoJson {
            line: String::from_utf8_lossy(&output.stdout).into_owned(),
            source,
        })
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

    fn active_features(
        &self,
        manifest: &CargoManifest,
        args: &MetadataCargoArgs,
    ) -> Result<ResolvedFeatures, BindingMetadataBuildError> {
        let root_package = self.package(manifest)?;
        Ok(args
            .feature_selection
            .resolve_for_package(&root_package.name, root_package.features()))
    }
}

#[derive(Clone, Debug, Deserialize)]
struct MetadataPackage {
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
}

impl MetadataPackage {
    fn library_source(&self) -> Option<PathBuf> {
        self.targets
            .iter()
            .find(|target| target.is_library())
            .map(MetadataTarget::source)
    }

    fn features(&self) -> &BTreeMap<String, Vec<String>> {
        &self.features
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
    manifest: &'build CargoManifest,
    source_root: &'build SourceRoot,
    features: ResolvedFeatures,
}

impl<'build> CargoBuild<'build> {
    fn new(
        build: &'build BindingMetadataBuild,
        manifest: &'build CargoManifest,
        source_root: &'build SourceRoot,
        features: ResolvedFeatures,
    ) -> Self {
        Self {
            build,
            manifest,
            source_root,
            features,
        }
    }

    fn output(self) -> Result<CargoOutput, BindingMetadataBuildError> {
        let surface = BindingMetadataSurface::from_target_triple(self.build.target.as_deref());
        let mut command = Command::new(CargoProgram::from_env().into_os_string());
        if let Some(toolchain_selector) = self.build.toolchain_selector.as_deref() {
            command.arg(toolchain_selector);
        }
        command
            .arg("build")
            .arg("--lib")
            .arg("--message-format=json-render-diagnostics")
            .arg("--manifest-path")
            .arg(&self.build.manifest_path);
        if let Some(target) = &self.build.target {
            command.arg("--target").arg(target);
        }
        command.args(self.build.cargo_args.iter());
        command.env(BINDING_METADATA_BUILD_ENV, "1");
        command.env(BINDING_METADATA_SOURCE_ENV, self.source_root.path());
        command.env(BINDING_METADATA_SURFACE_ENV, surface.as_str());
        command.env(BINDING_METADATA_FEATURES_ENV, self.features.env_value());
        if let Some(root) = self.manifest.path().parent() {
            command.env(BINDING_METADATA_ROOT_ENV, root);
        }
        MetadataRustflags::from_env().apply(&mut command);
        command
            .output()
            .map_err(|source| BindingMetadataBuildError::CargoSpawn { source })
            .and_then(CargoOutput::from_output)
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
    arguments: Vec<String>,
    feature_selection: CargoFeatureSelection,
}

impl MetadataCargoArgs {
    fn new(arguments: impl IntoIterator<Item = String>) -> Self {
        let arguments = Self::without_owned_selectors(arguments.into_iter().collect());
        let feature_selection = CargoFeatureSelection::from_args(&arguments);
        Self {
            arguments,
            feature_selection,
        }
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

    fn artifacts(
        &self,
        manifest: &CargoManifest,
    ) -> Result<MetadataArtifacts, BindingMetadataBuildError> {
        let artifacts = self
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(CargoMessage::parse)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|message| message.filenames(manifest))
            .filter_map(MetadataArtifact::from_cargo_filename)
            .collect::<Vec<_>>();

        MetadataArtifacts::new(manifest.path(), artifacts)
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
        from_str(line).map_err(|source| BindingMetadataBuildError::CargoJson {
            line: line.to_owned(),
            source,
        })
    }

    fn filenames(self, manifest: &CargoManifest) -> Vec<PathBuf> {
        match self {
            Self::CompilerArtifact {
                manifest_path,
                filenames,
            } if manifest.matches(&manifest_path) => filenames,
            Self::Other => Vec::new(),
            Self::CompilerArtifact { .. } => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MetadataRustflags {
    Encoded(OsString),
    Plain(OsString),
}

impl MetadataRustflags {
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
                value.push(" --cfg boltffi_metadata");
                value
            }
            None => OsString::from("--cfg boltffi_metadata"),
        })
    }

    fn append_encoded(mut existing: OsString) -> OsString {
        if !existing.is_empty() {
            existing.push(OsStr::new("\u{1f}"));
        }
        existing.push(OsStr::new("--cfg"));
        existing.push(OsStr::new("\u{1f}"));
        existing.push(OsStr::new("boltffi_metadata"));
        existing
    }

    fn apply(self, command: &mut Command) {
        match self {
            Self::Encoded(value) => {
                command.env("CARGO_ENCODED_RUSTFLAGS", value);
            }
            Self::Plain(value) => {
                command.env("RUSTFLAGS", value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_ast::{PackageInfo, SourceContract};
    use boltffi_binding::{
        BindingMetadataEnvelope, BindingMetadataSection, Decl, Native, SerializedBindings,
        lower_with_declarations,
    };

    use super::{
        BindingMetadataBuild, BindingMetadataBuildError, MetadataCargoArgs, resolve_cargo_features,
    };
    use crate::artifact::BindingMetadataReadError;

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
    fn cargo_build_reads_metadata_from_reported_artifacts() {
        if cfg!(miri) {
            return;
        }

        let expected = metadata_envelope("metadata_fixture");
        let fixture = FixtureCrate::with_metadata(&expected);

        let envelopes = BindingMetadataBuild::new(fixture.manifest())
            .read()
            .expect("cargo metadata build reads");

        assert_eq!(envelopes, vec![expected]);
    }

    #[test]
    fn cargo_build_ignores_dependency_metadata_artifacts() {
        if cfg!(miri) {
            return;
        }

        let expected = metadata_envelope("metadata_fixture");
        let dependency = metadata_envelope("metadata_dependency");
        let fixture = FixtureCrate::with_metadata_dependency(&expected, &dependency);

        let envelopes = BindingMetadataBuild::new(fixture.manifest())
            .read()
            .expect("cargo metadata build reads");

        assert_eq!(envelopes, vec![expected]);
    }

    #[test]
    fn cargo_build_reads_macro_emitted_metadata_without_expanding_wrappers() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_boltffi_macros();

        let envelopes = BindingMetadataBuild::new(fixture.manifest())
            .read()
            .expect("cargo metadata build reads");

        assert_eq!(envelopes.len(), 1);
        let SerializedBindings::Native(bindings) = envelopes[0].bindings() else {
            panic!("expected native metadata");
        };
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
            1
        );
    }

    #[test]
    fn cargo_build_reads_feature_gated_macro_metadata() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_feature_gated_boltffi_macros();

        let envelopes = BindingMetadataBuild::new(fixture.manifest())
            .cargo_args(["--features".to_owned(), "native-ffi".to_owned()])
            .read()
            .expect("cargo metadata build reads");

        assert_eq!(envelopes.len(), 1);
        let SerializedBindings::Native(bindings) = envelopes[0].bindings() else {
            panic!("expected native metadata");
        };
        assert_eq!(
            bindings
                .decls()
                .iter()
                .filter(|decl| matches!(decl, Decl::Enum(_)))
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
    fn cargo_build_rejects_crate_without_metadata() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::without_metadata();

        let error = BindingMetadataBuild::new(fixture.manifest())
            .read()
            .expect_err("metadata is required");

        assert!(matches!(
            error,
            BindingMetadataBuildError::Metadata(BindingMetadataReadError::NoMetadata { .. })
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

    #[test]
    fn cargo_features_resolve_root_features() {
        if cfg!(miri) {
            return;
        }

        let fixture = FixtureCrate::with_feature_gated_boltffi_macros();
        let features = resolve_cargo_features(
            fixture.manifest(),
            ["--features".to_owned(), "native-ffi".to_owned()],
            None,
        )
        .expect("resolve Cargo features");

        assert_eq!(
            features.iter().collect::<Vec<_>>(),
            vec!["native-ffi", "transport"]
        );
        assert_eq!(features.env_value(), "native-ffi,transport");
    }

    struct FixtureCrate {
        root: PathBuf,
        manifest: PathBuf,
    }

    impl FixtureCrate {
        fn with_metadata(envelope: &BindingMetadataEnvelope) -> Self {
            Self::write(Source::with_metadata(envelope), Dependency::None)
        }

        fn with_boltffi_macros() -> Self {
            Self::write(Source::with_boltffi_macros(), Dependency::Boltffi)
        }

        fn with_feature_gated_boltffi_macros() -> Self {
            let root = temp_root("boltffi-bindgen-cargo-metadata");
            let source_dir = root.join("src");
            let manifest = root.join("Cargo.toml");
            fs::create_dir_all(&source_dir).expect("create metadata fixture source dir");
            fs::write(
                &manifest,
                format!(
                    "[package]\nname = \"metadata_fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[features]\nnative-ffi = [\"transport\"]\ntransport = []\n\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                    workspace_crate("boltffi").display()
                ),
            )
            .expect("write metadata fixture manifest");
            fs::write(
                source_dir.join("lib.rs"),
                r#"
#[cfg_attr(feature = "native-ffi", boltffi::data)]
#[derive(Clone, Copy)]
pub enum Signal {
    Ping,
    Pong,
}

#[cfg_attr(feature = "native-ffi", boltffi::export)]
pub fn signal_name(signal: Signal) -> String {
    match signal {
        Signal::Ping => "Ping".to_owned(),
        Signal::Pong => "Pong".to_owned(),
    }
}
"#,
            )
            .expect("write metadata fixture lib");
            Self { root, manifest }
        }

        fn with_metadata_dependency(
            envelope: &BindingMetadataEnvelope,
            dependency: &BindingMetadataEnvelope,
        ) -> Self {
            Self::write(
                Source::with_dependency_metadata(envelope),
                Dependency::Metadata(dependency),
            )
        }

        fn without_metadata() -> Self {
            Self::write(Source::without_metadata(), Dependency::None)
        }

        fn write(source: Source, dependency: Dependency<'_>) -> Self {
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

    impl Drop for FixtureCrate {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    enum Dependency<'envelope> {
        Boltffi,
        Metadata(&'envelope BindingMetadataEnvelope),
        None,
    }

    impl Dependency<'_> {
        fn root_manifest(&self) -> String {
            let dependency = match self {
                Self::Boltffi => format!(
                    "\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                    workspace_crate("boltffi").display()
                ),
                Self::Metadata(_) => {
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
            if let Self::Metadata(envelope) = self {
                let package = root.join("metadata_dependency");
                let source = package.join("src");
                fs::create_dir_all(&source).expect("create metadata dependency source dir");
                fs::write(
                    package.join("Cargo.toml"),
                    "[package]\nname = \"metadata_dependency\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
                )
                .expect("write metadata dependency manifest");
                fs::write(
                    source.join("lib.rs"),
                    Source::with_metadata_and_body(envelope, "pub fn value() -> u32 { 7 }\n")
                        .into_string(),
                )
                .expect("write metadata dependency lib");
            }
        }
    }

    struct Source {
        code: String,
    }

    impl Source {
        fn with_metadata(envelope: &BindingMetadataEnvelope) -> Self {
            Self::with_metadata_and_body(envelope, "pub fn exported() -> u32 { 1 }\n")
        }

        fn with_boltffi_macros() -> Self {
            Self {
                code: r#"
pub mod domain {
    use boltffi::data;

    #[data]
    pub struct Point {
        pub x: f64,
    }
}

pub mod api {
    use boltffi::export;

    use crate::domain::Point;

    #[export]
    pub fn origin() -> Point {
        Point { x: 0.0 }
    }
}
"#
                .to_owned(),
            }
        }

        fn with_dependency_metadata(envelope: &BindingMetadataEnvelope) -> Self {
            Self::with_metadata_and_body(
                envelope,
                "pub fn exported() -> u32 { metadata_dependency::value() }\n",
            )
        }

        fn with_metadata_and_body(envelope: &BindingMetadataEnvelope, body: &str) -> Self {
            let section_bytes = envelope.to_section_bytes().expect("metadata section bytes");
            let length = section_bytes.len();
            let bytes = section_bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let mach_o_section = BindingMetadataSection::MachO.link_section();
            let object_section = BindingMetadataSection::Object.link_section();
            Self {
                code: format!(
                    "#![allow(unexpected_cfgs)]\n#[cfg(boltffi_metadata)]\n#[cfg_attr(target_vendor = \"apple\", unsafe(link_section = \"{mach_o_section}\"))]\n#[cfg_attr(not(target_vendor = \"apple\"), unsafe(link_section = \"{object_section}\"))]\n#[used]\nstatic BOLTFFI_METADATA: [u8; {length}] = [{bytes}];\n{body}"
                ),
            }
        }

        fn without_metadata() -> Self {
            Self {
                code: "pub fn exported() -> u32 { 1 }\n".to_owned(),
            }
        }

        fn into_string(self) -> String {
            self.code
        }
    }

    fn metadata_envelope(package: &str) -> BindingMetadataEnvelope {
        let source = SourceContract::new(PackageInfo::new(package, None));
        let lowered = lower_with_declarations::<Native>(&source).expect("empty source lowers");
        BindingMetadataEnvelope::new(SerializedBindings::native(lowered.into_bindings()))
            .expect("metadata envelope")
    }

    fn workspace_crate(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join(name)
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
