use std::fs;
use std::path::{Path, PathBuf};

use boltffi_backend::bridge::c::CBridge;
use boltffi_backend::core::{CoverageMode, bridge, host};
use boltffi_backend::target::python::PythonCExtHost;
use boltffi_backend::target::ruby::RubyCExtHost;
use boltffi_backend::{GeneratedOutput, Target as BackendTarget};
use boltffi_binding::{BindingMetadataSurface, Bindings, Native, Surface};
use thiserror::Error;

use crate::metadata::{BindingMetadataBuild, BindingMetadataBuildError};
use crate::target::Target;

/// Drives one BoltFFI generation from a compiled crate's embedded metadata
/// to rendered target-language files.
///
/// The driver runs the metadata build, selects the binding contract for the
/// target surface, renders it through the supplied [`Target`], and returns
/// the generated output. It carries no language-specific knowledge: the host
/// and bridge stack inside the [`Target`] decide everything about the
/// produced files.
#[derive(Clone, Debug)]
pub struct Generation {
    manifest_path: PathBuf,
    triple: Option<String>,
    coverage: CoverageMode,
    cargo_args: Vec<String>,
    python_package_module: Option<String>,
    python_distribution_name: Option<String>,
    python_package_version: Option<String>,
    python_native_library: Option<String>,
    ruby_ractor_safe: bool,
}

impl Generation {
    /// Creates a generation for a Cargo manifest.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            triple: None,
            coverage: CoverageMode::Complete,
            cargo_args: Vec::new(),
            python_package_module: None,
            python_distribution_name: None,
            python_package_version: None,
            python_native_library: None,
            ruby_ractor_safe: false,
        }
    }

    /// Builds for a Cargo target triple.
    pub fn triple(mut self, triple: impl Into<String>) -> Self {
        self.triple = Some(triple.into());
        self
    }

    /// Passes Cargo build arguments to metadata generation.
    pub fn cargo_args(mut self, cargo_args: impl IntoIterator<Item = String>) -> Self {
        self.cargo_args = cargo_args.into_iter().collect();
        self
    }

    /// Sets how unsupported backend declarations are handled.
    pub fn coverage_mode(mut self, coverage: CoverageMode) -> Self {
        self.coverage = coverage;
        self
    }

    /// Sets the generated Python package module name.
    pub fn python_module_name(mut self, module_name: impl Into<String>) -> Self {
        self.python_package_module = Some(module_name.into());
        self
    }

    /// Sets the generated Python distribution name.
    pub fn python_distribution_name(mut self, distribution_name: impl Into<String>) -> Self {
        self.python_distribution_name = Some(distribution_name.into());
        self
    }

    /// Sets the generated Python package version.
    pub fn python_package_version(mut self, package_version: Option<String>) -> Self {
        self.python_package_version = package_version;
        self
    }

    /// Sets the native library artifact name loaded by the Python package.
    pub fn python_native_library(mut self, native_library: impl Into<String>) -> Self {
        self.python_native_library = Some(native_library.into());
        self
    }

    /// Declares the generated Ruby extension Ractor-safe (`rb_ext_ractor_safe(true)`).
    pub fn ruby_ractor_safe(mut self, ractor_safe: bool) -> Self {
        self.ruby_ractor_safe = ractor_safe;
        self
    }

    /// Reads the embedded metadata, selects the target surface contract, and renders it.
    pub fn render(&self, target: Target) -> Result<GeneratedOutput, GenerationError> {
        match target {
            Target::Python => self.render_python(),
            Target::Ruby => self.render_ruby(),
            Target::Swift
            | Target::Kotlin
            | Target::KotlinMultiplatform
            | Target::Java
            | Target::TypeScript
            | Target::Header
            | Target::Dart
            | Target::CSharp => Err(GenerationError::UnsupportedTarget { target }),
        }
    }

    /// Renders the bindings and writes every generated file under `output_dir`.
    pub fn write(
        &self,
        target: Target,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        let output = self.render(target)?;
        Self::write_output(output, output_dir)
    }

    fn render_python(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let target = self
            .python_host()?
            .into_target(&bindings)
            .map_err(GenerationError::Render)?;
        self.render_backend(&target, &bindings)
    }

    fn render_ruby(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let crate_stem = bindings
            .package()
            .name()
            .as_path_string()
            .replace("::", "_");
        let target = BackendTarget::new(
            RubyCExtHost::new().ractor_safe(self.ruby_ractor_safe),
            CBridge::new(format!("ext/{crate_stem}/boltffi.h")).map_err(GenerationError::Render)?,
        );
        self.render_backend(&target, &bindings)
    }

    fn render_backend<H, S>(
        &self,
        target: &BackendTarget<H, S>,
        bindings: &Bindings<S::Surface>,
    ) -> Result<GeneratedOutput, GenerationError>
    where
        H: host::HostBackend<Bridge = S::Contract, Surface = S::Surface>,
        S: bridge::BridgeStack,
    {
        target
            .render_with_coverage(bindings, self.coverage)
            .map_err(GenerationError::Render)
    }

    fn python_host(&self) -> Result<PythonCExtHost, GenerationError> {
        let host = self
            .python_package_module
            .as_deref()
            .map(|module| PythonCExtHost::new().module_name(module))
            .transpose()
            .map_err(GenerationError::Render)
            .map(Option::unwrap_or_default)?;
        let host = self
            .python_distribution_name
            .iter()
            .fold(host, |host, name| host.distribution_name(name.clone()));
        let host = self
            .python_native_library
            .iter()
            .fold(host, |host, library| host.native_library(library.clone()));
        Ok(host.version(self.python_package_version.clone()))
    }

    /// Writes generated output to a directory.
    pub fn write_output(
        output: GeneratedOutput,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        output
            .files()
            .iter()
            .map(|file| {
                let path = output_dir.join(file.path().as_path());
                write_file(&path, file.contents())?;
                Ok(path)
            })
            .collect()
    }

    fn bindings<S: Surface>(&self) -> Result<Bindings<S>, GenerationError> {
        let surface = BindingMetadataSurface::from_target_triple(self.triple.as_deref());
        let mut build = BindingMetadataBuild::new(&self.manifest_path);
        if !self.cargo_args.is_empty() {
            build = build.cargo_args(self.cargo_args.clone());
        }
        if let Some(triple) = &self.triple {
            build = build.target(triple);
        }
        build
            .read()?
            .into_iter()
            .find(|envelope| envelope.surface() == surface)
            .and_then(|envelope| S::from_serialized(envelope.into_bindings()))
            .ok_or(GenerationError::MissingSurface { surface })
    }
}

/// Failure while generating bindings from embedded crate metadata.
#[derive(Debug, Error)]
pub enum GenerationError {
    /// The metadata build or artifact read failed.
    #[error(transparent)]
    Metadata(#[from] BindingMetadataBuildError),
    /// The compiled crate embedded no metadata for the requested surface.
    #[error("compiled crate embeds no binding metadata for the {surface:?} surface")]
    MissingSurface {
        /// Surface selected from the target triple.
        surface: BindingMetadataSurface,
    },
    /// The target backend failed to render the bindings.
    #[error("render bindings: {0}")]
    Render(boltffi_backend::Error),
    /// The target is not wired to the IR generation pipeline.
    #[error("IR generation is not available for {target}")]
    UnsupportedTarget {
        /// Requested target.
        target: Target,
    },
    /// A generated file could not be written to disk.
    #[error("write generated file `{path}`: {source}")]
    Write {
        /// Generated file path.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
}

fn write_file(path: &Path, contents: &str) -> Result<(), GenerationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| GenerationError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, contents).map_err(|source| GenerationError::Write {
        path: path.to_path_buf(),
        source,
    })
}
