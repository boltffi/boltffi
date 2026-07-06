use std::fs;
use std::path::{Path, PathBuf};

use boltffi_backend::core::{CoverageMode, bridge, host};
use boltffi_backend::target::{
    kmp::{KmpHost, KmpSupportMode},
    kotlin::{KotlinApiStyle, KotlinDesktopLoader, KotlinFactoryStyle, KotlinHost},
    python::PythonCExtHost,
    swift::SwiftHost,
};
use boltffi_backend::{CustomTypeMapping, GeneratedOutput, Target as BackendTarget};
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
    kotlin_package: Option<String>,
    kotlin_file: Option<String>,
    kotlin_android_library: Option<String>,
    kotlin_desktop_jni_library: Option<String>,
    kotlin_desktop_fallback_library: Option<String>,
    kotlin_c_header: Option<PathBuf>,
    kotlin_desktop_loader: KotlinDesktopLoader,
    kotlin_api_style: KotlinApiStyle,
    kotlin_factory_style: KotlinFactoryStyle,
    kotlin_custom_mappings: Vec<(String, CustomTypeMapping)>,
    swift_custom_mappings: Vec<(String, CustomTypeMapping)>,
    swift_ffi_module: Option<String>,
    swift_file: Option<String>,
    swift_c_header: Option<PathBuf>,
    kmp_package_name: Option<String>,
    kmp_module_name: Option<String>,
    kmp_min_sdk: Option<u32>,
    kmp_support_mode: KmpSupportMode,
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
            kotlin_package: None,
            kotlin_file: None,
            kotlin_android_library: None,
            kotlin_desktop_jni_library: None,
            kotlin_desktop_fallback_library: None,
            kotlin_c_header: None,
            kotlin_desktop_loader: KotlinDesktopLoader::default(),
            kotlin_api_style: KotlinApiStyle::default(),
            kotlin_factory_style: KotlinFactoryStyle::default(),
            kotlin_custom_mappings: Vec::new(),
            swift_custom_mappings: Vec::new(),
            swift_ffi_module: None,
            swift_file: None,
            swift_c_header: None,
            kmp_package_name: None,
            kmp_module_name: None,
            kmp_min_sdk: None,
            kmp_support_mode: KmpSupportMode::Strict,
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

    /// Sets the generated Kotlin package name.
    pub fn kotlin_package(mut self, package: impl Into<String>) -> Self {
        self.kotlin_package = Some(package.into());
        self
    }

    /// Sets the generated Kotlin owner file name.
    pub fn kotlin_file(mut self, file: impl Into<String>) -> Self {
        self.kotlin_file = Some(file.into());
        self
    }

    /// Sets the Android native library load name used by Kotlin.
    pub fn kotlin_android_library(mut self, library: impl Into<String>) -> Self {
        self.kotlin_android_library = Some(library.into());
        self
    }

    /// Sets the desktop JNI wrapper library load name used by Kotlin.
    pub fn kotlin_desktop_jni_library(mut self, library: impl Into<String>) -> Self {
        self.kotlin_desktop_jni_library = Some(library.into());
        self
    }

    /// Sets the desktop fallback native library load name used by Kotlin.
    pub fn kotlin_desktop_fallback_library(mut self, library: impl Into<String>) -> Self {
        self.kotlin_desktop_fallback_library = Some(library.into());
        self
    }

    /// Sets the generated C header included by the JNI bridge.
    pub fn kotlin_c_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.kotlin_c_header = Some(path.into());
        self
    }

    /// Sets how the generated Kotlin module loads desktop native libraries.
    pub fn kotlin_desktop_loader(mut self, loader: KotlinDesktopLoader) -> Self {
        self.kotlin_desktop_loader = loader;
        self
    }

    /// Sets the generated Kotlin API layout.
    pub fn kotlin_api_style(mut self, style: KotlinApiStyle) -> Self {
        self.kotlin_api_style = style;
        self
    }

    /// Sets the generated Kotlin class factory layout.
    pub fn kotlin_factory_style(mut self, style: KotlinFactoryStyle) -> Self {
        self.kotlin_factory_style = style;
        self
    }

    /// Registers Kotlin API mappings for custom types.
    pub fn kotlin_custom_mappings(
        mut self,
        mappings: impl IntoIterator<Item = (String, CustomTypeMapping)>,
    ) -> Self {
        self.kotlin_custom_mappings = mappings.into_iter().collect();
        self
    }

    /// Registers Swift API mappings for custom types.
    pub fn swift_custom_mappings(
        mut self,
        mappings: impl IntoIterator<Item = (String, CustomTypeMapping)>,
    ) -> Self {
        self.swift_custom_mappings = mappings.into_iter().collect();
        self
    }

    /// Sets the C FFI module imported by the generated Swift source.
    pub fn swift_ffi_module(mut self, module: impl Into<String>) -> Self {
        self.swift_ffi_module = Some(module.into());
        self
    }

    /// Sets the generated Swift source file.
    pub fn swift_file(mut self, file: impl Into<String>) -> Self {
        self.swift_file = Some(file.into());
        self
    }

    /// Sets the C bridge header path generated with the Swift source.
    pub fn swift_c_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.swift_c_header = Some(path.into());
        self
    }

    /// Sets the generated Kotlin Multiplatform package name.
    pub fn kmp_package_name(mut self, package_name: impl Into<String>) -> Self {
        self.kmp_package_name = Some(package_name.into());
        self
    }

    /// Sets the generated Kotlin Multiplatform module/source class name.
    pub fn kmp_module_name(mut self, module_name: impl Into<String>) -> Self {
        self.kmp_module_name = Some(module_name.into());
        self
    }

    /// Sets the Android minSdk written into generated KMP Gradle output.
    pub fn kmp_min_sdk(mut self, min_sdk: u32) -> Self {
        self.kmp_min_sdk = Some(min_sdk);
        self
    }

    /// Sets the KMP support mode recorded in generated support metadata.
    pub fn kmp_support_mode(mut self, support_mode: KmpSupportMode) -> Self {
        self.kmp_support_mode = support_mode;
        self
    }

    /// Reads the embedded metadata, selects the target surface contract, and renders it.
    pub fn render(&self, target: Target) -> Result<GeneratedOutput, GenerationError> {
        match target {
            Target::Python => self.render_python(),
            Target::Kotlin => self.render_kotlin(),
            Target::KotlinMultiplatform => self.render_kmp(),
            Target::Swift => self.render_swift(),
            Target::Java | Target::TypeScript | Target::Header | Target::Dart | Target::CSharp => {
                Err(GenerationError::UnsupportedTarget { target })
            }
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

    fn render_kotlin(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let package = self
            .kotlin_package
            .as_deref()
            .unwrap_or("com.example.boltffi");
        let file = self.kotlin_file.as_deref().unwrap_or("BoltFfi");
        let target = self
            .kotlin_host(package, file)?
            .into_target()
            .map_err(GenerationError::Render)?;
        self.render_backend(&target, &bindings)
    }

    fn kotlin_host(&self, package: &str, file: &str) -> Result<KotlinHost, GenerationError> {
        let host = KotlinHost::new(package, file)
            .map_err(GenerationError::Render)?
            .desktop_loader(self.kotlin_desktop_loader)
            .api_style(self.kotlin_api_style)
            .factory_style(self.kotlin_factory_style);
        let host = self
            .kotlin_custom_mappings
            .iter()
            .fold(host, |host, (custom_type, mapping)| {
                host.custom_mapping(custom_type.clone(), mapping.clone())
            });
        let host = self
            .kotlin_android_library
            .iter()
            .try_fold(host, |host, library| host.android_library(library.clone()))
            .map_err(GenerationError::Render)?;
        let host = self
            .kotlin_desktop_jni_library
            .iter()
            .try_fold(host, |host, library| {
                host.desktop_jni_library(library.clone())
            })
            .map_err(GenerationError::Render)?;
        let host = self
            .kotlin_desktop_fallback_library
            .iter()
            .try_fold(host, |host, library| {
                host.desktop_fallback_library(library.clone())
            })
            .map_err(GenerationError::Render)?;
        Ok(self
            .kotlin_c_header
            .iter()
            .fold(host, |host, header| host.c_header(header.clone())))
    }

    fn render_kmp(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let target = self.kmp_host().into_target();
        self.render_backend(&target, &bindings)
    }

    fn render_swift(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let target = self
            .swift_host()?
            .into_target()
            .map_err(GenerationError::Render)?;
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

    fn swift_host(&self) -> Result<SwiftHost, GenerationError> {
        let module = self.swift_ffi_module.as_deref().unwrap_or("BoltFFI");
        let host = SwiftHost::new(module).map_err(GenerationError::Render)?;
        let host = self
            .swift_custom_mappings
            .iter()
            .fold(host, |host, (custom_type, mapping)| {
                host.custom_mapping(custom_type.clone(), mapping.clone())
            });
        let host = self
            .swift_file
            .iter()
            .try_fold(host, |host, file| host.file(file.clone()))
            .map_err(GenerationError::Render)?;
        Ok(self
            .swift_c_header
            .iter()
            .fold(host, |host, header| host.c_header(header.clone())))
    }

    fn kmp_host(&self) -> KmpHost {
        let host = KmpHost::new().support_mode(self.kmp_support_mode);
        let host = self
            .kmp_package_name
            .iter()
            .fold(host, |host, package| host.package_name(package.clone()));
        let host = self
            .kmp_module_name
            .iter()
            .fold(host, |host, module| host.module_name(module.clone()));
        self.kmp_min_sdk
            .iter()
            .fold(host, |host, min_sdk| host.min_sdk(*min_sdk))
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
