use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use boltffi_backend::bridge::c::CBridge;
use boltffi_backend::core::bridge::BridgeBackend;
use boltffi_backend::core::{CoverageMode, bridge, host};
use boltffi_backend::target::{
    java::{JavaDesktopLoader, JavaHost, JavaVersion},
    kmp::{DEFAULT_KMP_MODULE_NAME, DEFAULT_KMP_PACKAGE_NAME, KmpHost, KmpSupportMode},
    kotlin::{KotlinApiStyle, KotlinDesktopLoader, KotlinFactoryStyle, KotlinHost},
    python::PythonCExtHost,
    swift::SwiftHost,
    typescript::TypeScriptHost,
};
use boltffi_backend::{CustomTypeMapping, GeneratedOutput, Target as BackendTarget};
use boltffi_binding::{BindingMetadataSurface, Bindings, Native, Surface, Wasm32};
use thiserror::Error;

use crate::metadata::{BindingMetadataBuild, BindingMetadataBuildError};
use crate::render::kmp::delegate::KmpJvmDelegateAdapter;
use crate::render::kotlin::KotlinOptions;
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
    binding_surface: Option<BindingMetadataSurface>,
    coverage: CoverageMode,
    cargo_args: Vec<String>,
    cargo_environment: Vec<(OsString, OsString)>,
    cargo_toolchain_selector: Option<String>,
    python_package_module: Option<String>,
    python_distribution_name: Option<String>,
    python_package_version: Option<String>,
    python_native_library: Option<String>,
    java_package: Option<String>,
    java_file: Option<String>,
    java_android_library: Option<String>,
    java_desktop_jni_library: Option<String>,
    java_desktop_fallback_library: Option<String>,
    java_c_header: Option<PathBuf>,
    java_desktop_loader: JavaDesktopLoader,
    java_version: JavaVersion,
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
    kmp_kotlin_options: KotlinOptions,
    kmp_support_mode: KmpSupportMode,
    typescript_module: Option<String>,
    typescript_runtime_package: Option<String>,
}

impl Generation {
    /// Creates a generation for a Cargo manifest.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            triple: None,
            binding_surface: None,
            coverage: CoverageMode::Complete,
            cargo_args: Vec::new(),
            cargo_environment: Vec::new(),
            cargo_toolchain_selector: None,
            python_package_module: None,
            python_distribution_name: None,
            python_package_version: None,
            python_native_library: None,
            java_package: None,
            java_file: None,
            java_android_library: None,
            java_desktop_jni_library: None,
            java_desktop_fallback_library: None,
            java_c_header: None,
            java_desktop_loader: JavaDesktopLoader::default(),
            java_version: JavaVersion::default(),
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
            kmp_kotlin_options: KotlinOptions::default(),
            kmp_support_mode: KmpSupportMode::Strict,
            typescript_module: None,
            typescript_runtime_package: None,
        }
    }

    /// Builds for a Cargo target triple.
    pub fn triple(mut self, triple: impl Into<String>) -> Self {
        self.triple = Some(triple.into());
        self
    }

    #[allow(missing_docs)]
    pub fn binding_surface(mut self, surface: BindingMetadataSurface) -> Self {
        self.binding_surface = Some(surface);
        self
    }

    /// Passes Cargo build arguments to metadata generation.
    pub fn cargo_args(mut self, cargo_args: impl IntoIterator<Item = String>) -> Self {
        self.cargo_args = cargo_args.into_iter().collect();
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

    /// Selects a rustup Cargo toolchain for metadata generation.
    pub fn cargo_toolchain_selector(mut self, toolchain_selector: Option<String>) -> Self {
        self.cargo_toolchain_selector = toolchain_selector;
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

    /// Sets the generated Java package name.
    pub fn java_package(mut self, package: impl Into<String>) -> Self {
        self.java_package = Some(package.into());
        self
    }

    /// Sets the generated Java owner file name.
    pub fn java_file(mut self, file: impl Into<String>) -> Self {
        self.java_file = Some(file.into());
        self
    }

    /// Sets the Android native library load name used by Java.
    pub fn java_android_library(mut self, library: impl Into<String>) -> Self {
        self.java_android_library = Some(library.into());
        self
    }

    /// Sets the desktop JNI wrapper library load name used by Java.
    pub fn java_desktop_jni_library(mut self, library: impl Into<String>) -> Self {
        self.java_desktop_jni_library = Some(library.into());
        self
    }

    /// Sets the desktop fallback native library load name used by Java.
    pub fn java_desktop_fallback_library(mut self, library: impl Into<String>) -> Self {
        self.java_desktop_fallback_library = Some(library.into());
        self
    }

    /// Sets the generated C header included by the Java JNI bridge.
    pub fn java_c_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.java_c_header = Some(path.into());
        self
    }

    /// Sets how the generated Java module loads desktop native libraries.
    pub fn java_desktop_loader(mut self, loader: JavaDesktopLoader) -> Self {
        self.java_desktop_loader = loader;
        self
    }

    /// Sets the generated Java source and runtime release.
    pub fn java_version(mut self, version: JavaVersion) -> Self {
        self.java_version = version;
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

    /// Sets Kotlin/JNI loader options used by generated KMP JVM and Android delegates.
    pub fn kmp_kotlin_options(mut self, kotlin_options: KotlinOptions) -> Self {
        self.kmp_kotlin_options = kotlin_options;
        self
    }

    /// Sets the KMP support mode recorded in generated support metadata.
    pub fn kmp_support_mode(mut self, support_mode: KmpSupportMode) -> Self {
        self.kmp_support_mode = support_mode;
        self
    }

    #[allow(missing_docs)]
    pub fn typescript_module(mut self, module: impl Into<String>) -> Self {
        self.typescript_module = Some(module.into());
        self
    }

    #[allow(missing_docs)]
    pub fn typescript_runtime_package(mut self, package: impl Into<String>) -> Self {
        self.typescript_runtime_package = Some(package.into());
        self
    }

    /// Reads the embedded metadata, selects the target surface contract, and renders it.
    pub fn render(&self, target: Target) -> Result<GeneratedOutput, GenerationError> {
        match target {
            Target::Python | Target::Java | Target::Kotlin | Target::KotlinMultiplatform => {
                let bindings = self.bindings::<Native>()?;
                self.render_native_bindings(target, &bindings)
            }
            Target::Swift => self.render_swift(),
            Target::TypeScript => self.render_typescript(),
            Target::Header | Target::Dart | Target::CSharp => {
                Err(GenerationError::UnsupportedTarget { target })
            }
        }
    }

    /// Renders a C header from the same metadata-backed native bindings path.
    pub fn render_c_header(
        &self,
        header_path: impl Into<PathBuf>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        self.render_c_header_bindings(&bindings, header_path)
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

    fn render_native_bindings(
        &self,
        target: Target,
        bindings: &Bindings<Native>,
    ) -> Result<GeneratedOutput, GenerationError> {
        match target {
            Target::Python => self.render_python_bindings(bindings),
            Target::Java => self.render_java_bindings(bindings),
            Target::Kotlin => self.render_kotlin_bindings(bindings),
            Target::KotlinMultiplatform => self.render_kmp_bindings(bindings),
            Target::Swift | Target::TypeScript | Target::Header | Target::Dart | Target::CSharp => {
                Err(GenerationError::UnsupportedTarget { target })
            }
        }
    }

    fn render_java_bindings(
        &self,
        bindings: &Bindings<Native>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let package = self
            .java_package
            .as_deref()
            .unwrap_or("com.example.boltffi");
        let file = self.java_file.as_deref().unwrap_or("BoltFfi");
        self.java_host(package, file)?
            .render_with_coverage(bindings, self.coverage)
            .map_err(GenerationError::Render)
    }

    fn java_host(&self, package: &str, file: &str) -> Result<JavaHost, GenerationError> {
        let host = JavaHost::for_version(package, file, self.java_version)
            .map_err(GenerationError::Render)?
            .desktop_loader(self.java_desktop_loader);
        let host = self
            .java_android_library
            .iter()
            .try_fold(host, |host, library| host.android_library(library.clone()))
            .map_err(GenerationError::Render)?;
        let host = self
            .java_desktop_jni_library
            .iter()
            .try_fold(host, |host, library| {
                host.desktop_jni_library(library.clone())
            })
            .map_err(GenerationError::Render)?;
        let host = self
            .java_desktop_fallback_library
            .iter()
            .try_fold(host, |host, library| {
                host.desktop_fallback_library(library.clone())
            })
            .map_err(GenerationError::Render)?;
        Ok(self
            .java_c_header
            .iter()
            .fold(host, |host, header| host.c_header(header.clone())))
    }

    fn render_kotlin_bindings(
        &self,
        bindings: &Bindings<Native>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let package = self
            .kotlin_package
            .as_deref()
            .unwrap_or("com.example.boltffi");
        let file = self.kotlin_file.as_deref().unwrap_or("BoltFfi");
        let target = self
            .kotlin_host(package, file)?
            .into_target()
            .map_err(GenerationError::Render)?;
        self.render_backend(&target, bindings)
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

    fn render_python_bindings(
        &self,
        bindings: &Bindings<Native>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let target = self
            .python_host()?
            .into_target(bindings)
            .map_err(GenerationError::Render)?;
        self.render_backend(&target, bindings)
    }

    fn render_kmp_bindings(
        &self,
        bindings: &Bindings<Native>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let target = self.kmp_host(bindings)?.into_target();
        self.render_backend(&target, bindings)
    }

    fn render_swift(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let target = self
            .swift_host()?
            .into_target()
            .map_err(GenerationError::Render)?;
        self.render_backend(&target, &bindings)
    }

    fn render_typescript(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Wasm32>()?;
        self.render_typescript_bindings(&bindings)
    }

    fn render_typescript_bindings(
        &self,
        bindings: &Bindings<Wasm32>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let module = self.typescript_module.as_deref().unwrap_or("boltffi");
        let host = TypeScriptHost::new(module)
            .map_err(GenerationError::Render)?
            .runtime_package(
                self.typescript_runtime_package
                    .as_deref()
                    .unwrap_or("@boltffi/runtime"),
            );
        self.render_backend(&host.into_target(), bindings)
    }

    fn render_c_header_bindings(
        &self,
        bindings: &Bindings<Native>,
        header_path: impl Into<PathBuf>,
    ) -> Result<GeneratedOutput, GenerationError> {
        let bridge = CBridge::new(header_path).map_err(GenerationError::Render)?;
        let contract = bridge
            .build_contract(bindings)
            .map_err(GenerationError::Render)?;
        bridge
            .render_bridge(bindings, &contract)
            .map_err(GenerationError::Render)
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

    fn kmp_host(&self, bindings: &Bindings<Native>) -> Result<KmpHost, GenerationError> {
        let package_name = self.effective_kmp_package_name();
        let module_name = self.effective_kmp_module_name();
        let delegate = KmpJvmDelegateAdapter::new(
            package_name.clone(),
            module_name.clone(),
            self.kmp_kotlin_options.clone(),
        )
        .adapt_bindings(bindings)
        .map_err(|source| GenerationError::KmpJvmDelegate {
            message: source.to_string(),
        })?;
        let host = KmpHost::new().support_mode(self.kmp_support_mode);
        let host = host.package_name(package_name).module_name(module_name);
        let host = self
            .kmp_min_sdk
            .iter()
            .fold(host, |host, min_sdk| host.min_sdk(*min_sdk));
        Ok(host.jvm_delegate(delegate))
    }

    fn effective_kmp_package_name(&self) -> String {
        self.kmp_package_name
            .clone()
            .unwrap_or_else(|| DEFAULT_KMP_PACKAGE_NAME.to_string())
    }

    fn effective_kmp_module_name(&self) -> String {
        self.kmp_module_name
            .clone()
            .unwrap_or_else(|| DEFAULT_KMP_MODULE_NAME.to_string())
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
        let surface = self
            .binding_surface
            .unwrap_or_else(|| BindingMetadataSurface::from_target_triple(self.triple.as_deref()));
        self.metadata_build()
            .read()?
            .into_iter()
            .find(|envelope| envelope.surface() == surface)
            .and_then(|envelope| S::from_serialized(envelope.into_bindings()))
            .ok_or(GenerationError::MissingSurface { surface })
    }

    fn metadata_build(&self) -> BindingMetadataBuild {
        let surface = self
            .binding_surface
            .unwrap_or_else(|| BindingMetadataSurface::from_target_triple(self.triple.as_deref()));
        let mut build = BindingMetadataBuild::new(&self.manifest_path)
            .surface(surface)
            .cargo_environment(self.cargo_environment.clone());
        if !self.cargo_args.is_empty() {
            build = build.cargo_args(self.cargo_args.clone());
        }
        if let Some(toolchain_selector) = &self.cargo_toolchain_selector {
            build = build.rustup_toolchain(toolchain_selector.clone());
        }
        if let Some(triple) = &self.triple {
            build = build.target(triple);
        }
        build
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
    /// The Kotlin/JNI delegate adapter failed before backend rendering.
    #[error("adapt KMP JVM delegate: {message}")]
    KmpJvmDelegate {
        /// Adapter failure message.
        message: String,
    },
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_ast::{
        CanonicalName as SourceCanonicalName, FunctionDef as SourceFunctionDef,
        FunctionId as SourceFunctionId, PackageInfo as SourcePackageInfo,
        ParameterDef as SourceParameterDef, Primitive as SourcePrimitive,
        ReturnDef as SourceReturnDef, SourceContract, SourceName, TypeExpr as SourceTypeExpr,
    };
    use boltffi_backend::target::kmp::KMP_SUPPORT_REPORT_FILE;

    use super::*;

    fn primitive_function_bindings() -> Bindings<Native> {
        bindings_for_functions(vec![primitive_function(
            "demo::add",
            "add",
            vec![
                ("left", SourcePrimitive::I32),
                ("right", SourcePrimitive::I32),
            ],
            SourcePrimitive::I32,
        )])
    }

    fn primitive_function_bindings_wasm32() -> Bindings<Wasm32> {
        bindings_for_functions_wasm32(vec![primitive_function(
            "demo::add",
            "add",
            vec![
                ("left", SourcePrimitive::I32),
                ("right", SourcePrimitive::I32),
            ],
            SourcePrimitive::I32,
        )])
    }

    fn bindings_for_functions(functions: Vec<SourceFunctionDef>) -> Bindings<Native> {
        let mut source = SourceContract::new(SourcePackageInfo::new("demo", None));
        source.functions = functions;
        boltffi_binding::lower::<Native>(&source).expect("primitive function should lower")
    }

    fn bindings_for_functions_wasm32(functions: Vec<SourceFunctionDef>) -> Bindings<Wasm32> {
        let mut source = SourceContract::new(SourcePackageInfo::new("demo", None));
        source.functions = functions;
        boltffi_binding::lower::<Wasm32>(&source).expect("primitive function should lower")
    }

    fn primitive_function(
        id: &str,
        name: &str,
        params: Vec<(&str, SourcePrimitive)>,
        returns: SourcePrimitive,
    ) -> SourceFunctionDef {
        let mut function = SourceFunctionDef::new(SourceFunctionId::new(id), source_name(name));
        function.parameters = params
            .into_iter()
            .map(|(name, primitive)| {
                SourceParameterDef::value(source_name(name), SourceTypeExpr::Primitive(primitive))
            })
            .collect();
        function.returns = SourceReturnDef::value(SourceTypeExpr::Primitive(returns));
        function
    }

    fn source_name(part: &str) -> SourceName {
        SourceName::from_canonical(SourceCanonicalName::single(part))
    }

    fn name(part: &str) -> SourceName {
        source_name(part)
    }

    fn file<'output>(output: &'output GeneratedOutput, path: &str) -> &'output str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new(path))
            .unwrap_or_else(|| panic!("missing generated file {path}"))
            .contents()
    }

    fn output_paths(output: &GeneratedOutput) -> Vec<String> {
        output
            .files()
            .iter()
            .map(|file| file.path().as_path().display().to_string())
            .collect()
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    fn render_primitive_kmp_output() -> GeneratedOutput {
        let bindings = primitive_function_bindings();
        let generation = Generation::new("Cargo.toml")
            .kmp_package_name("com.boltffi.demo")
            .kmp_module_name("Demo");

        generation
            .render_native_bindings(Target::KotlinMultiplatform, &bindings)
            .expect("primitive KMP bindings should render through the production target route")
    }

    #[test]
    fn generation_preserves_the_complete_cargo_build_contract() {
        let generation = Generation::new("selected/Cargo.toml")
            .triple("x86_64-unknown-linux-gnu")
            .cargo_args(["--features".to_string(), "ffi".to_string()])
            .cargo_environment([(
                OsString::from("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"),
                OsString::from("/opt/cross/bin/clang"),
            )])
            .cargo_toolchain_selector(Some("+nightly".to_string()));

        assert_eq!(
            generation.metadata_build(),
            BindingMetadataBuild::new("selected/Cargo.toml")
                .target("x86_64-unknown-linux-gnu")
                .surface(BindingMetadataSurface::Native)
                .cargo_args(["--features".to_string(), "ffi".to_string()])
                .cargo_environment([(
                    OsString::from("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"),
                    OsString::from("/opt/cross/bin/clang"),
                )])
                .rustup_toolchain("+nightly")
        );
    }

    #[test]
    fn kmp_generation_public_render_route_attempts_metadata_read() {
        let error = Generation::new("missing-kmp-fixture/Cargo.toml")
            .render(Target::KotlinMultiplatform)
            .expect_err("KMP public render route should try to read metadata");

        assert!(matches!(error, GenerationError::Metadata(_)), "{error}");
    }

    #[test]
    fn c_header_generation_uses_requested_header_path_for_native_bindings() {
        let bindings = primitive_function_bindings();
        let output = Generation::new("Cargo.toml")
            .render_c_header_bindings(&bindings, "selected_package.h")
            .expect("C header should render for primitive bindings");

        assert_eq!(output.files().len(), 1);
        assert_eq!(
            output.files()[0].path().as_path(),
            Path::new("selected_package.h")
        );
        assert!(
            output.files()[0]
                .contents()
                .contains("boltffi_function_demo_add")
        );
    }

    #[test]
    fn java_generation_wires_primitive_bindings_through_shared_jni() {
        let bindings = primitive_function_bindings();
        let output = Generation::new("Cargo.toml")
            .java_package("com.boltffi.demo")
            .java_file("Demo")
            .java_android_library("demo")
            .java_desktop_jni_library("demo_jni")
            .java_desktop_fallback_library("demo")
            .java_desktop_loader(JavaDesktopLoader::None)
            .java_c_header("jni/demo.h")
            .render_native_bindings(Target::Java, &bindings)
            .expect("primitive Java bindings should render through the production target route");

        assert_eq!(
            output_paths(&output),
            vec!["jni/demo.h", "jni/jni_glue.c", "com/boltffi/demo/Demo.java",]
        );
        assert!(
            file(&output, "com/boltffi/demo/Demo.java")
                .contains("public static int add(int left, int right)")
        );
        assert!(file(&output, "jni/jni_glue.c").contains(
            "JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1add"
        ));
    }

    #[test]
    fn typescript_generation_wires_primitive_bindings_through_wasm32() {
        let bindings = primitive_function_bindings_wasm32();
        let output = Generation::new("Cargo.toml")
            .typescript_module("demo")
            .typescript_runtime_package("@example/runtime")
            .render_typescript_bindings(&bindings)
            .expect(
                "primitive TypeScript bindings should render through the production target route",
            );

        assert_eq!(output_paths(&output), vec!["demo.ts", "demo_node.ts"]);
        assert!(file(&output, "demo.ts").contains("from \"@example/runtime\""));
        assert!(
            file(&output, "demo.ts")
                .contains("export function add(left: number, right: number): number")
        );
        assert!(
            file(&output, "demo.ts").contains("_exports.boltffi_function_demo_add as Function")
        );
    }

    #[test]
    fn kmp_generation_wires_jni_delegate_for_sync_primitive_bindings() {
        let output = render_primitive_kmp_output();

        assert!(
            file(&output, "src/commonMain/kotlin/com/boltffi/demo/Demo.kt")
                .contains("expect fun add(left: Int, right: Int): Int")
        );
        assert!(
            file(
                &output,
                "src/jvmMain/kotlin/com/boltffi/demo/DemoJvmActual.kt"
            )
            .contains("return com.boltffi.demo.jvm.add(left, right)")
        );
        assert!(
            file(&output, "src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt")
                .contains("external fun boltffi_function_demo_add(left: Int, right: Int): Int")
        );
        assert!(
            file(&output, "src/jvmMain/c/jni_glue.c")
                .contains("_result = boltffi_function_demo_add(left, right);")
        );
    }

    #[test]
    fn kmp_generation_uses_configured_kotlin_loader_options() {
        let bindings = primitive_function_bindings();
        let output = Generation::new("Cargo.toml")
            .kmp_package_name("com.boltffi.demo")
            .kmp_module_name("Demo")
            .kmp_kotlin_options(KotlinOptions {
                library_name: Some(crate::load_library_name("configured-library")),
                desktop_jni_library_name: Some(crate::library_name("configured-library")),
                desktop_fallback_library_name: Some(crate::library_name("my-lib")),
                ..KotlinOptions::default()
            })
            .render_native_bindings(Target::KotlinMultiplatform, &bindings)
            .expect("configured KMP loader options should render");

        let jvm_internal = file(&output, "src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt");
        assert!(jvm_internal.contains("val androidLibrary = \"configured-library\""));
        assert!(jvm_internal.contains("val desktopPreferredLibrary = \"configured_library_jni\""));
        assert!(jvm_internal.contains("val desktopFallbackLibrary = \"my_lib\""));
    }

    #[test]
    fn kmp_generation_emits_compile_ready_jvm_android_smoke_for_sync_primitive_bindings() {
        let output = render_primitive_kmp_output();

        assert_eq!(
            output_paths(&output),
            vec![
                "settings.gradle.kts",
                "build.gradle.kts",
                "src/commonMain/kotlin/com/boltffi/demo/Demo.kt",
                KMP_SUPPORT_REPORT_FILE,
                "src/jvmMain/kotlin/com/boltffi/demo/DemoJvmActual.kt",
                "src/androidMain/kotlin/com/boltffi/demo/DemoAndroidActual.kt",
                "src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt",
                "src/androidMain/kotlin/com/boltffi/demo/jvm/Demo.kt",
                "src/jvmMain/c/jni_glue.c",
                "src/androidMain/c/jni_glue.c",
            ]
        );

        let common = file(&output, "src/commonMain/kotlin/com/boltffi/demo/Demo.kt");
        let jvm_actual = file(
            &output,
            "src/jvmMain/kotlin/com/boltffi/demo/DemoJvmActual.kt",
        );
        let android_actual = file(
            &output,
            "src/androidMain/kotlin/com/boltffi/demo/DemoAndroidActual.kt",
        );
        let jvm_internal = file(&output, "src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt");
        let android_internal = file(
            &output,
            "src/androidMain/kotlin/com/boltffi/demo/jvm/Demo.kt",
        );
        let jvm_jni = file(&output, "src/jvmMain/c/jni_glue.c");
        let android_jni = file(&output, "src/androidMain/c/jni_glue.c");
        let build_gradle = file(&output, "build.gradle.kts");
        let settings_gradle = file(&output, "settings.gradle.kts");
        let report: serde_json::Value =
            serde_json::from_str(file(&output, KMP_SUPPORT_REPORT_FILE))
                .expect("KMP support report should be valid JSON");

        assert!(common.contains("package com.boltffi.demo"));
        assert!(common.contains("expect fun add(left: Int, right: Int): Int"));
        assert!(!common.contains("actual fun"));
        assert!(!common.contains("Native."));

        assert_eq!(jvm_actual, android_actual);
        assert!(jvm_actual.contains("actual fun add(left: Int, right: Int): Int"));
        assert!(jvm_actual.contains("return com.boltffi.demo.jvm.add(left, right)"));
        assert!(!jvm_actual.contains("Native."));

        assert_eq!(jvm_internal, android_internal);
        assert!(jvm_internal.contains("package com.boltffi.demo.jvm"));
        assert!(jvm_internal.contains("private object Native"));
        assert!(jvm_internal.contains(
            "@JvmStatic external fun boltffi_function_demo_add(left: Int, right: Int): Int"
        ));
        assert!(jvm_internal.contains("fun add(left: Int, right: Int): Int"));
        assert!(jvm_internal.contains("return Native.boltffi_function_demo_add(left, right)"));
        assert!(!jvm_internal.contains("expect fun"));
        assert!(!jvm_internal.contains("actual fun"));

        assert_eq!(jvm_jni, android_jni);
        assert!(jvm_jni.contains("#include <boltffi_generated/demo.h>"));
        assert!(jvm_jni.contains(
            "JNIEXPORT jint JNICALL Java_com_boltffi_demo_jvm_Native_boltffi_1function_1demo_1add"
        ));
        assert!(jvm_jni.contains("boltffi_function_demo_add(left, right)"));

        assert!(build_gradle.contains("kotlin(\"multiplatform\") version \"2.4.0\""));
        assert!(build_gradle.contains("id(\"com.android.library\") version \"8.5.2\""));
        assert!(build_gradle.contains("jvm {"));
        assert!(build_gradle.contains("androidTarget {"));
        assert!(build_gradle.contains("namespace = \"com.boltffi.demo\""));
        assert!(settings_gradle.contains("rootProject.name = \"demo-kmp\""));

        assert_eq!(report["mode"], "strict");
        assert_eq!(
            report["selected_platforms"],
            serde_json::json!(["jvm", "android"])
        );
        assert_eq!(
            report["admitted_apis"],
            serde_json::json!([{ "kind": "function", "name": "add" }])
        );
        assert_eq!(report["rejected_apis"], serde_json::json!([]));
    }

    #[test]
    fn kmp_generation_gradle_smoke_compiles_current_project_when_enabled() {
        if !kmp_gradle_smoke_enabled() {
            return;
        }

        let gradle = kmp_gradle_command();
        let tasks = kmp_gradle_smoke_tasks();
        let output_directory = unique_temp_dir("boltffi-kmp-gradle-smoke");
        let output = render_primitive_kmp_output();
        Generation::write_output(output, &output_directory)
            .expect("generated KMP Gradle project should be written");

        let result = Command::new(&gradle)
            .current_dir(&output_directory)
            .args(["--no-daemon", "--stacktrace"])
            .args(&tasks)
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to run Gradle command `{}` for KMP smoke in `{}`: {error}\n\
                     note: this smoke compiles a generated KMP module that configures androidTarget, \
                     so opt-in runs require Gradle plus Android SDK/tooling",
                    gradle.to_string_lossy(),
                    output_directory.display()
                )
            });

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);

        assert!(
            result.status.success(),
            "KMP Gradle smoke failed with status {:?}\n\
             generated project retained at: {}\n\
             note: this smoke compiles a generated KMP module that configures androidTarget, \
             so opt-in runs require Gradle plus Android SDK/tooling\n\
             stdout:\n{}\nstderr:\n{}",
            result.status.code(),
            output_directory.display(),
            stdout,
            stderr
        );

        fs::remove_dir_all(output_directory).expect("cleanup generated KMP Gradle smoke project");
    }

    fn kmp_gradle_command() -> OsString {
        std::env::var_os("BOLTFFI_KMP_GRADLE")
            .map(resolve_kmp_gradle_command)
            .unwrap_or_else(|| OsString::from("gradle"))
    }

    fn resolve_kmp_gradle_command(command: OsString) -> OsString {
        let path = PathBuf::from(command.clone());
        if path.is_relative() && path.components().count() > 1 {
            return workspace_root().join(path).into_os_string();
        }

        command
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("boltffi_bindgen should be a workspace member")
            .to_path_buf()
    }

    fn kmp_gradle_smoke_enabled() -> bool {
        match std::env::var("BOLTFFI_KMP_GRADLE_SMOKE") {
            Ok(value)
                if matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                ) =>
            {
                true
            }
            Ok(value)
                if matches!(
                    value.to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                ) =>
            {
                false
            }
            Ok(value) => panic!(
                "BOLTFFI_KMP_GRADLE_SMOKE must be one of 1/true/yes/on or 0/false/no/off, got `{value}`"
            ),
            Err(_) => false,
        }
    }

    fn kmp_gradle_smoke_tasks() -> Vec<String> {
        let tasks = std::env::var("BOLTFFI_KMP_GRADLE_TASKS")
            .unwrap_or_else(|_| "compileKotlinJvm".to_string());
        parse_kmp_gradle_smoke_tasks(&tasks)
    }

    fn parse_kmp_gradle_smoke_tasks(tasks: &str) -> Vec<String> {
        let tasks = tasks
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert!(
            !tasks.is_empty(),
            "BOLTFFI_KMP_GRADLE_TASKS must contain at least one Gradle task"
        );
        tasks
    }

    #[test]
    fn kmp_gradle_command_resolves_repository_relative_path_overrides() {
        assert_eq!(
            PathBuf::from(resolve_kmp_gradle_command(OsString::from(
                "tools/gradle/bin/gradle"
            ))),
            workspace_root().join("tools/gradle/bin/gradle")
        );
        assert_eq!(
            resolve_kmp_gradle_command(OsString::from("gradle")),
            OsString::from("gradle")
        );
    }

    #[test]
    #[should_panic(expected = "BOLTFFI_KMP_GRADLE_TASKS must contain at least one Gradle task")]
    fn kmp_gradle_smoke_tasks_rejects_empty_task_override() {
        parse_kmp_gradle_smoke_tasks(" \t\n ");
    }

    #[test]
    fn kmp_generation_uses_backend_planned_kotlin_name_for_delegate_matching() {
        let bindings = bindings_for_functions(vec![primitive_function(
            "demo::DoTheThing",
            "DoTheThing",
            vec![("value", SourcePrimitive::I32)],
            SourcePrimitive::I32,
        )]);
        let generation = Generation::new("Cargo.toml")
            .kmp_package_name("com.boltffi.demo")
            .kmp_module_name("Demo");
        let target = generation
            .kmp_host(&bindings)
            .expect("KMP host should adapt primitive bindings")
            .into_target();

        let output = generation
            .render_backend(&target, &bindings)
            .expect("backend-planned Kotlin names should be covered by the delegate");

        let common = file(&output, "src/commonMain/kotlin/com/boltffi/demo/Demo.kt");
        assert!(
            common.contains("expect fun dothething(`value`: Int): Int"),
            "{common}"
        );
        assert!(
            file(
                &output,
                "src/jvmMain/kotlin/com/boltffi/demo/DemoJvmActual.kt"
            )
            .contains("return com.boltffi.demo.jvm.dothething(`value`)")
        );
        assert!(
            file(&output, "src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt")
                .contains("fun dothething(`value`: Int): Int")
        );
        assert!(
            file(&output, "src/jvmMain/c/jni_glue.c")
                .contains("_result = boltffi_function_demo_do_the_thing(value);")
        );
    }

    #[test]
    fn kmp_generation_preserves_distinct_backend_symbols_for_same_public_name_overloads() {
        let bindings = bindings_for_functions(vec![
            primitive_function(
                "demo::signed::read",
                "read",
                vec![("value", SourcePrimitive::I32)],
                SourcePrimitive::I32,
            ),
            primitive_function(
                "demo::wide::read",
                "read",
                vec![("value", SourcePrimitive::I64)],
                SourcePrimitive::I64,
            ),
        ]);
        let generation = Generation::new("Cargo.toml")
            .kmp_package_name("com.boltffi.demo")
            .kmp_module_name("Demo");
        let target = generation
            .kmp_host(&bindings)
            .expect("KMP host should adapt primitive overloads")
            .into_target();

        let output = generation
            .render_backend(&target, &bindings)
            .expect("same-name overloads with distinct signatures should keep both delegates");
        let jni = file(&output, "src/jvmMain/c/jni_glue.c");

        let common = file(&output, "src/commonMain/kotlin/com/boltffi/demo/Demo.kt");
        assert!(
            common.contains("expect fun read(`value`: Int): Int"),
            "{common}"
        );
        assert!(
            common.contains("expect fun read(`value`: Long): Long"),
            "{common}"
        );
        assert!(jni.contains("_result = boltffi_function_demo_signed_read(value);"));
        assert!(jni.contains("_result = boltffi_function_demo_wide_read(value);"));
    }
}
