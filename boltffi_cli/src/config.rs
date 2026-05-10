use crate::target::{
    AndroidArchitecture, AppleArchitecture, AppleIosArchitecture, CSharpRuntimeIdentifier,
    DartNativeArchitecture, JavaHostTarget, RustTarget, resolve_android_targets,
    resolve_apple_ios_targets, resolve_apple_macos_targets, resolve_apple_simulator_targets,
    resolve_dart_native_targets, resolve_java_host_targets,
};
use boltffi_bindgen::render::python::NamingConvention;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Swift,
    Kotlin,
    Java,
    TypeScript,
    Header,
    Dart,
    Python,
    CSharp,
}

impl Target {
    pub const fn name(self) -> &'static str {
        match self {
            Target::Swift => "swift",
            Target::Kotlin => "kotlin",
            Target::Java => "java",
            Target::TypeScript => "typescript",
            Target::Header => "header",
            Target::Dart => "dart",
            Target::Python => "python",
            Target::CSharp => "csharp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Experimental {
    WholeTarget(Target),
    Feature { target: Target, name: &'static str },
}

impl Experimental {
    pub const ALL: &'static [Experimental] = &[
        Experimental::Feature {
            target: Target::TypeScript,
            name: "async_streams",
        },
        Experimental::WholeTarget(Target::Dart),
        Experimental::WholeTarget(Target::Python),
    ];

    pub fn is_target_experimental(target: Target) -> bool {
        Self::ALL.iter().any(
            |experimental| matches!(experimental, Experimental::WholeTarget(t) if *t == target),
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub experimental: Vec<String>,
    #[serde(default)]
    pub cargo: CargoConfig,
    pub package: PackageConfig,
    #[serde(default)]
    pub targets: TargetsConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CargoConfig {
    #[serde(default)]
    pub global_args: Vec<String>,
    #[serde(default)]
    pub command_args: HashMap<String, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageConfig {
    pub name: String,
    #[serde(rename = "crate")]
    pub crate_name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TargetsConfig {
    #[serde(default)]
    pub apple: AppleConfig,
    #[serde(default)]
    pub android: AndroidConfig,
    #[serde(default)]
    pub wasm: WasmConfig,
    #[serde(default)]
    pub java: JavaConfig,
    #[serde(default)]
    pub dart: DartConfig,
    #[serde(default)]
    pub python: PythonConfig,
    #[serde(default)]
    pub csharp: CSharpConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DartConfig {
    #[serde(default = "default_dart_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub native_architectures: Option<Vec<DartNativeArchitecture>>,
}

impl Default for DartConfig {
    fn default() -> Self {
        Self {
            output: default_dart_output(),
            enabled: false,
            native_architectures: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PythonConfig {
    #[serde(default = "default_python_output")]
    pub output: PathBuf,
    pub module_name: Option<String>,
    #[serde(default, alias = "pack")]
    pub wheel: PythonWheelConfig,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            output: default_python_output(),
            module_name: None,
            wheel: PythonWheelConfig::default(),
            enabled: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PythonWheelConfig {
    #[serde(alias = "wheel_output")]
    pub output: Option<PathBuf>,
    pub interpreters: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CSharpConfig {
    #[serde(default = "default_csharp_output")]
    pub output: PathBuf,
    pub package_id: Option<String>,
    pub target_framework: Option<String>,
    pub package_output: Option<PathBuf>,
    pub runtime_identifiers: Option<Vec<CSharpRuntimeIdentifier>>,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for CSharpConfig {
    fn default() -> Self {
        Self {
            output: default_csharp_output(),
            package_id: None,
            target_framework: None,
            package_output: None,
            runtime_identifiers: None,
            enabled: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ErrorStyle {
    #[default]
    Throwing,
    Result,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FactoryStyle {
    #[default]
    Constructors,
    CompanionMethods,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypeConversion {
    UuidString,
    UrlString,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct TypeMapping {
    #[serde(rename = "type")]
    pub native_type: String,
    pub conversion: TypeConversion,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AppleSwiftConfig {
    pub module_name: Option<String>,
    pub output: Option<PathBuf>,
    pub ffi_module_name: Option<String>,
    pub tools_version: Option<String>,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AndroidKotlinConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub module_name: Option<String>,
    pub library_name: Option<String>,
    #[serde(default)]
    pub desktop_loader: KotlinDesktopLoader,
    #[serde(default)]
    pub api_style: KotlinApiStyle,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub factory_style: FactoryStyle,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KotlinApiStyle {
    #[default]
    TopLevel,
    ModuleObject,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KotlinDesktopLoader {
    #[default]
    Bundled,
    System,
    None,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppleConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_apple_output")]
    pub output: PathBuf,
    #[serde(default = "default_apple_deployment_target")]
    pub deployment_target: String,
    #[serde(default)]
    pub include_macos: bool,
    pub ios_architectures: Option<Vec<AppleIosArchitecture>>,
    pub simulator_architectures: Option<Vec<AppleArchitecture>>,
    pub macos_architectures: Option<Vec<AppleArchitecture>>,
    #[serde(default)]
    pub swift: AppleSwiftConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub xcframework: XcframeworkConfig,
    #[serde(default)]
    pub spm: SpmConfig,
    #[serde(default)]
    pub debug_symbols: DebugSymbolsConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AndroidConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
    pub ndk_version: Option<String>,
    pub architectures: Option<Vec<AndroidArchitecture>>,
    #[serde(default)]
    pub kotlin: AndroidKotlinConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub pack: AndroidPackConfig,
    #[serde(default)]
    pub debug_symbols: DebugSymbolsConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct HeaderConfig {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DebugSymbolsFormat {
    #[default]
    Zip,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DebugSymbolsBundle {
    #[default]
    Unstripped,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DebugSymbolsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub format: DebugSymbolsFormat,
    #[serde(default)]
    pub bundle: DebugSymbolsBundle,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct XcframeworkConfig {
    pub output: Option<PathBuf>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpmDistribution {
    #[default]
    Local,
    Remote,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SpmConfig {
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub distribution: SpmDistribution,
    pub repo_url: Option<String>,
    #[serde(default)]
    pub layout: SpmLayout,
    pub package_name: Option<String>,
    pub wrapper_sources: Option<PathBuf>,
    #[serde(default)]
    pub skip_package_swift: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AndroidPackConfig {
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct JavaConfig {
    pub package: Option<String>,
    pub module_name: Option<String>,
    pub min_version: Option<u8>,
    #[serde(default)]
    pub jvm: JavaJvmConfig,
    #[serde(default)]
    pub android: JavaAndroidConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JavaJvmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_java_jvm_output")]
    pub output: PathBuf,
    pub host_targets: Option<Vec<JavaHostTarget>>,
    #[serde(default)]
    pub strip_symbols: bool,
    #[serde(default)]
    pub debug_symbols: DebugSymbolsConfig,
}

impl Default for JavaJvmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_java_jvm_output(),
            host_targets: None,
            strip_symbols: false,
            debug_symbols: DebugSymbolsConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JavaAndroidConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_java_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
}

impl Default for JavaAndroidConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_java_android_output(),
            min_sdk: default_android_min_sdk(),
        }
    }
}

fn default_java_jvm_output() -> PathBuf {
    PathBuf::from("dist/java")
}

fn default_java_android_output() -> PathBuf {
    PathBuf::from("dist/java/android")
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SpmLayout {
    Bundled,
    Split,
    #[default]
    FfiOnly,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WasmConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_wasm_triple")]
    pub triple: String,
    #[serde(default)]
    pub profile: WasmProfile,
    #[serde(default = "default_wasm_output")]
    pub output: PathBuf,
    pub artifact_path: Option<PathBuf>,
    #[serde(default)]
    pub optimize: WasmOptimizeConfig,
    #[serde(default)]
    pub typescript: WasmTypeScriptConfig,
    #[serde(default)]
    pub npm: WasmNpmConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum WasmProfile {
    Debug,
    #[default]
    Release,
}

impl WasmProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WasmOptimizeConfig {
    pub enabled: Option<bool>,
    pub level: Option<WasmOptimizeLevel>,
    pub strip_debug: Option<bool>,
    pub on_missing: Option<WasmOptimizeOnMissing>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum WasmOptimizeLevel {
    #[serde(rename = "0")]
    O0,
    #[serde(rename = "1")]
    O1,
    #[serde(rename = "2")]
    O2,
    #[serde(rename = "3")]
    O3,
    #[serde(rename = "4")]
    O4,
    #[serde(rename = "s")]
    Size,
    #[serde(rename = "z")]
    MinSize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WasmOptimizeOnMissing {
    Error,
    Warn,
    Skip,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WasmTypeScriptConfig {
    pub output: Option<PathBuf>,
    pub runtime_package: Option<String>,
    pub runtime_version: Option<String>,
    pub module_name: Option<String>,
    pub source_map: Option<bool>,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WasmNpmConfig {
    pub package_name: Option<String>,
    pub output: Option<PathBuf>,
    pub targets: Option<Vec<WasmNpmTarget>>,
    pub generate_package_json: Option<bool>,
    pub generate_readme: Option<bool>,
    pub version: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WasmNpmTarget {
    Bundler,
    Web,
    Nodejs,
}

impl Default for AppleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output: default_apple_output(),
            deployment_target: default_apple_deployment_target(),
            include_macos: false,
            ios_architectures: None,
            simulator_architectures: None,
            macos_architectures: None,
            swift: AppleSwiftConfig::default(),
            header: HeaderConfig::default(),
            xcframework: XcframeworkConfig::default(),
            spm: SpmConfig::default(),
            debug_symbols: DebugSymbolsConfig::default(),
        }
    }
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output: default_android_output(),
            min_sdk: default_android_min_sdk(),
            ndk_version: None,
            architectures: None,
            kotlin: AndroidKotlinConfig::default(),
            header: HeaderConfig::default(),
            pack: AndroidPackConfig::default(),
            debug_symbols: DebugSymbolsConfig::default(),
        }
    }
}

impl Default for SpmConfig {
    fn default() -> Self {
        Self {
            output: None,
            distribution: SpmDistribution::Local,
            repo_url: None,
            layout: SpmLayout::default(),
            package_name: None,
            wrapper_sources: None,
            skip_package_swift: false,
        }
    }
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            triple: default_wasm_triple(),
            profile: WasmProfile::Release,
            output: default_wasm_output(),
            artifact_path: None,
            optimize: WasmOptimizeConfig::default(),
            typescript: WasmTypeScriptConfig::default(),
            npm: WasmNpmConfig::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_apple_output() -> PathBuf {
    PathBuf::from("dist/apple")
}

fn default_apple_deployment_target() -> String {
    "16.0".to_string()
}

fn default_android_output() -> PathBuf {
    PathBuf::from("dist/android")
}

fn default_android_min_sdk() -> u32 {
    24
}

fn default_wasm_triple() -> String {
    "wasm32-unknown-unknown".to_string()
}

fn default_wasm_output() -> PathBuf {
    PathBuf::from("dist/wasm")
}

fn default_dart_output() -> PathBuf {
    PathBuf::from("dist/dart")
}

fn default_python_output() -> PathBuf {
    PathBuf::from("dist/python")
}

fn default_csharp_output() -> PathBuf {
    PathBuf::from("dist/csharp")
}

fn read_toml_value(path: &Path) -> Result<toml::Value, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(|err| ConfigError::Read {
        path: path.to_path_buf(),
        source: err,
    })?;

    toml::from_str(&content).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

fn merge_toml_value(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, overlay_value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(base_value) => merge_toml_value(base_value, overlay_value),
                    None => {
                        base_table.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_value, overlay_value) => *base_value = overlay_value,
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::load_with_overlay(path, None)
    }

    pub fn load_with_overlay(
        path: &Path,
        overlay_path: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        let mut merged = read_toml_value(path)?;

        if let Some(overlay_path) = overlay_path {
            let overlay = read_toml_value(overlay_path)?;
            merge_toml_value(&mut merged, overlay);
        }

        let config: Config = merged.try_into().map_err(|source| match overlay_path {
            Some(overlay_path) => ConfigError::ParseMerged {
                base_path: path.to_path_buf(),
                overlay_path: overlay_path.to_path_buf(),
                source: Box::new(source),
            },
            None => ConfigError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            },
        })?;

        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        let content = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;

        std::fs::write(path, content).map_err(|err| ConfigError::Write {
            path: path.to_path_buf(),
            source: err,
        })
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.is_apple_enabled()
            && self.apple_spm_distribution() == SpmDistribution::Remote
            && self.apple_spm_repo_url().is_none()
        {
            return Err(ConfigError::Validation(
                "targets.apple.spm.repo_url is required when distribution = \"remote\"".to_string(),
            ));
        }

        if self.is_wasm_enabled()
            && let Some(targets) = self.targets.wasm.npm.targets.as_ref()
            && targets.is_empty()
        {
            return Err(ConfigError::Validation(
                "targets.wasm.npm.targets must be non-empty when provided".to_string(),
            ));
        }

        if self.is_android_enabled()
            && let Some(architectures) = self.targets.android.architectures.as_ref()
        {
            if architectures.is_empty() {
                return Err(ConfigError::Validation(
                    "targets.android.architectures must be non-empty when provided".to_string(),
                ));
            }

            let mut seen = HashSet::new();
            for architecture in architectures {
                if !seen.insert(*architecture) {
                    return Err(ConfigError::Validation(format!(
                        "targets.android.architectures contains duplicate architecture '{}'",
                        architecture.canonical_name()
                    )));
                }
            }
        }

        if self.is_apple_enabled() {
            validate_unique(
                self.targets.apple.ios_architectures.as_deref(),
                "targets.apple.ios_architectures",
                AppleIosArchitecture::canonical_name,
            )?;
            validate_unique(
                self.targets.apple.simulator_architectures.as_deref(),
                "targets.apple.simulator_architectures",
                AppleArchitecture::canonical_name,
            )?;
            if self.apple_include_macos() {
                validate_unique(
                    self.targets.apple.macos_architectures.as_deref(),
                    "targets.apple.macos_architectures",
                    AppleArchitecture::canonical_name,
                )?;
            }

            if self.apple_targets().is_empty() {
                return Err(ConfigError::Validation(
                    "Apple packaging requires at least one configured slice across targets.apple.ios_architectures, targets.apple.simulator_architectures, or enabled macOS architectures".to_string(),
                ));
            }
        }

        if self.is_java_jvm_enabled()
            && let Some(host_targets) = self.targets.java.jvm.host_targets.as_ref()
            && host_targets.is_empty()
        {
            return Err(ConfigError::Validation(
                "targets.java.jvm.host_targets must be non-empty when provided".to_string(),
            ));
        }

        if self.is_python_enabled()
            && let Some(interpreters) = self.targets.python.wheel.interpreters.as_ref()
        {
            if interpreters.is_empty() {
                return Err(ConfigError::Validation(
                    "targets.python.wheel.interpreters must be non-empty when provided".to_string(),
                ));
            }

            let mut seen = HashSet::new();
            for interpreter in interpreters {
                let normalized = interpreter.trim();
                if normalized.is_empty() {
                    return Err(ConfigError::Validation(
                        "targets.python.wheel.interpreters must not contain empty values"
                            .to_string(),
                    ));
                }

                if !seen.insert(normalized.to_string()) {
                    return Err(ConfigError::Validation(format!(
                        "targets.python.wheel.interpreters contains duplicate interpreter '{}'",
                        normalized
                    )));
                }
            }
        }

        if self.is_python_enabled()
            && let Some(module_name) = self.targets.python.module_name.as_deref()
            && !NamingConvention::is_valid_module_name(module_name)
        {
            return Err(ConfigError::Validation(format!(
                "targets.python.module_name must be a valid Python identifier, got '{}'",
                module_name
            )));
        }

        if self.is_csharp_enabled() {
            if let Some(runtime_identifiers) = self.targets.csharp.runtime_identifiers.as_ref() {
                if runtime_identifiers.is_empty() {
                    return Err(ConfigError::Validation(
                        "targets.csharp.runtime_identifiers must be non-empty when provided"
                            .to_string(),
                    ));
                }

                let mut seen = HashSet::new();
                for runtime_identifier in runtime_identifiers {
                    if !seen.insert(*runtime_identifier) {
                        return Err(ConfigError::Validation(format!(
                            "targets.csharp.runtime_identifiers contains duplicate runtime identifier '{}'",
                            runtime_identifier.canonical_name()
                        )));
                    }
                }
            }

            if let Some(package_id) = self.targets.csharp.package_id.as_deref()
                && package_id.trim().is_empty()
            {
                return Err(ConfigError::Validation(
                    "targets.csharp.package_id must not be empty".to_string(),
                ));
            }

            if let Some(target_framework) = self.targets.csharp.target_framework.as_deref()
                && target_framework.trim().is_empty()
            {
                return Err(ConfigError::Validation(
                    "targets.csharp.target_framework must not be empty".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn library_name(&self) -> &str {
        self.package
            .crate_name
            .as_deref()
            .unwrap_or(&self.package.name)
    }

    pub fn crate_artifact_name(&self) -> String {
        boltffi_bindgen::library_name(self.library_name()).into_string()
    }

    pub fn swift_module_name(&self) -> String {
        self.targets
            .apple
            .swift
            .module_name
            .clone()
            .unwrap_or_else(|| to_pascal_case(&self.package.name))
    }

    pub fn xcframework_name(&self) -> String {
        self.targets
            .apple
            .xcframework
            .name
            .clone()
            .unwrap_or_else(|| self.swift_module_name())
    }

    pub fn is_apple_enabled(&self) -> bool {
        self.targets.apple.enabled
    }

    pub fn is_android_enabled(&self) -> bool {
        self.targets.android.enabled
    }

    pub fn is_wasm_enabled(&self) -> bool {
        self.targets.wasm.enabled
    }

    pub fn is_dart_enabled(&self) -> bool {
        self.targets.dart.enabled
    }

    pub fn is_python_enabled(&self) -> bool {
        self.targets.python.enabled
    }

    pub fn is_csharp_enabled(&self) -> bool {
        self.targets.csharp.enabled
    }

    pub fn apple_include_macos(&self) -> bool {
        self.targets.apple.include_macos
    }

    pub fn apple_ios_architectures(&self) -> &[AppleIosArchitecture] {
        self.targets
            .apple
            .ios_architectures
            .as_deref()
            .unwrap_or(AppleIosArchitecture::ALL)
    }

    pub fn apple_simulator_architectures(&self) -> &[AppleArchitecture] {
        self.targets
            .apple
            .simulator_architectures
            .as_deref()
            .unwrap_or(AppleArchitecture::ALL)
    }

    pub fn apple_macos_architectures(&self) -> &[AppleArchitecture] {
        self.targets
            .apple
            .macos_architectures
            .as_deref()
            .unwrap_or(AppleArchitecture::ALL)
    }

    pub fn apple_ios_targets(&self) -> Vec<RustTarget> {
        resolve_apple_ios_targets(self.apple_ios_architectures())
    }

    pub fn apple_simulator_targets(&self) -> Vec<RustTarget> {
        resolve_apple_simulator_targets(self.apple_simulator_architectures())
    }

    pub fn apple_macos_targets(&self) -> Vec<RustTarget> {
        self.targets
            .apple
            .macos_architectures
            .as_deref()
            .map(resolve_apple_macos_targets)
            .unwrap_or_else(|| RustTarget::ALL_MACOS.to_vec())
    }

    pub fn apple_targets(&self) -> Vec<RustTarget> {
        let mut targets = self.apple_ios_targets();
        targets.extend(self.apple_simulator_targets());
        if self.apple_include_macos() {
            targets.extend(self.apple_macos_targets());
        }
        targets
    }

    pub fn apple_deployment_target(&self) -> &str {
        &self.targets.apple.deployment_target
    }

    pub fn apple_output(&self) -> PathBuf {
        self.targets.apple.output.clone()
    }

    pub fn apple_swift_output(&self) -> PathBuf {
        self.targets
            .apple
            .swift
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.join("Sources"))
    }

    pub fn apple_swift_ffi_module_name(&self) -> Option<&str> {
        self.targets.apple.swift.ffi_module_name.as_deref()
    }

    pub fn apple_header_output(&self) -> PathBuf {
        self.targets
            .apple
            .header
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.join("include"))
    }

    pub fn apple_xcframework_output(&self) -> PathBuf {
        self.targets
            .apple
            .xcframework
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.clone())
    }

    pub fn apple_spm_output(&self) -> PathBuf {
        self.targets
            .apple
            .spm
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.clone())
    }

    pub fn apple_debug_symbols_enabled(&self) -> bool {
        self.targets.apple.debug_symbols.enabled
    }

    pub fn apple_debug_symbols_output(&self) -> PathBuf {
        self.targets
            .apple
            .debug_symbols
            .output
            .clone()
            .unwrap_or_else(|| self.targets.apple.output.join("symbols"))
    }

    pub fn apple_debug_symbols_format(&self) -> DebugSymbolsFormat {
        self.targets.apple.debug_symbols.format
    }

    pub fn apple_debug_symbols_bundle(&self) -> DebugSymbolsBundle {
        self.targets.apple.debug_symbols.bundle
    }

    pub fn apple_spm_layout(&self) -> SpmLayout {
        self.targets.apple.spm.layout
    }

    pub fn apple_spm_wrapper_sources(&self) -> Option<&Path> {
        self.targets.apple.spm.wrapper_sources.as_deref()
    }

    pub fn android_min_sdk(&self) -> u32 {
        self.targets.android.min_sdk
    }

    pub fn android_ndk_version(&self) -> Option<&str> {
        self.targets.android.ndk_version.as_deref()
    }

    pub fn android_architectures(&self) -> &[AndroidArchitecture] {
        self.targets
            .android
            .architectures
            .as_deref()
            .unwrap_or(AndroidArchitecture::ALL)
    }

    pub fn android_targets(&self) -> Vec<RustTarget> {
        resolve_android_targets(self.android_architectures())
    }

    pub fn android_output(&self) -> PathBuf {
        self.targets.android.output.clone()
    }

    pub fn android_kotlin_package(&self) -> String {
        self.targets
            .android
            .kotlin
            .package
            .clone()
            .unwrap_or_else(|| {
                let normalized_name = self.package.name.replace('-', "_");
                format!("com.example.{}", normalized_name)
            })
    }

    pub fn android_kotlin_module_name(&self) -> String {
        self.targets
            .android
            .kotlin
            .module_name
            .clone()
            .unwrap_or_else(|| self.kotlin_class_name())
    }

    pub fn android_kotlin_library_name(&self) -> Option<&str> {
        self.targets.android.kotlin.library_name.as_deref()
    }

    pub fn android_kotlin_desktop_loader(&self) -> KotlinDesktopLoader {
        self.targets.android.kotlin.desktop_loader
    }

    pub fn android_kotlin_api_style(&self) -> KotlinApiStyle {
        self.targets.android.kotlin.api_style
    }

    pub fn android_kotlin_factory_style(&self) -> FactoryStyle {
        self.targets.android.kotlin.factory_style
    }

    pub fn android_kotlin_output(&self) -> PathBuf {
        self.targets
            .android
            .kotlin
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("kotlin"))
    }

    pub fn android_header_output(&self) -> PathBuf {
        self.targets
            .android
            .header
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("include"))
    }

    pub fn android_pack_output(&self) -> PathBuf {
        self.targets
            .android
            .pack
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("jniLibs"))
    }

    pub fn android_debug_symbols_enabled(&self) -> bool {
        self.targets.android.debug_symbols.enabled
    }

    pub fn android_debug_symbols_output(&self) -> PathBuf {
        self.targets
            .android
            .debug_symbols
            .output
            .clone()
            .unwrap_or_else(|| self.targets.android.output.join("symbols"))
    }

    pub fn android_debug_symbols_format(&self) -> DebugSymbolsFormat {
        self.targets.android.debug_symbols.format
    }

    pub fn android_debug_symbols_bundle(&self) -> DebugSymbolsBundle {
        self.targets.android.debug_symbols.bundle
    }

    pub fn kotlin_class_name(&self) -> String {
        to_pascal_case(&self.package.name)
    }

    pub fn apple_spm_distribution(&self) -> SpmDistribution {
        self.targets.apple.spm.distribution
    }

    pub fn apple_spm_repo_url(&self) -> Option<&str> {
        self.targets.apple.spm.repo_url.as_deref()
    }

    pub fn apple_swift_tools_version(&self) -> Option<&str> {
        self.targets.apple.swift.tools_version.as_deref()
    }

    pub fn apple_spm_package_name(&self) -> Option<&str> {
        self.targets.apple.spm.package_name.as_deref()
    }

    pub fn apple_spm_skip_package_swift(&self) -> bool {
        self.targets.apple.spm.skip_package_swift
    }

    pub fn swift_type_mappings(&self) -> &HashMap<String, TypeMapping> {
        &self.targets.apple.swift.type_mappings
    }

    pub fn kotlin_type_mappings(&self) -> &HashMap<String, TypeMapping> {
        &self.targets.android.kotlin.type_mappings
    }

    pub fn is_java_jvm_enabled(&self) -> bool {
        self.targets.java.jvm.enabled
    }

    pub fn is_java_android_enabled(&self) -> bool {
        self.targets.java.android.enabled
    }

    pub fn is_enabled(&self, target: Target) -> bool {
        match target {
            Target::Swift => self.is_apple_enabled(),
            Target::Kotlin => self.is_android_enabled(),
            Target::Java => self.is_java_jvm_enabled(),
            Target::TypeScript => self.is_wasm_enabled(),
            Target::Header => self.is_apple_enabled() || self.is_android_enabled(),
            Target::Dart => self.is_dart_enabled(),
            Target::Python => self.is_python_enabled(),
            Target::CSharp => self.is_csharp_enabled(),
        }
    }

    pub fn should_process(&self, target: Target, experimental_flag: bool) -> bool {
        self.is_enabled(target)
            && (!Experimental::is_target_experimental(target)
                || experimental_flag
                || self.is_experimental_enabled(&Experimental::WholeTarget(target)))
    }

    pub fn cargo_args_for_command(&self, command_name: &str) -> Vec<String> {
        self.cargo
            .global_args
            .iter()
            .chain(
                self.cargo
                    .command_args
                    .get(command_name)
                    .into_iter()
                    .flat_map(|args| args.iter()),
            )
            .cloned()
            .collect()
    }

    fn is_experimental_enabled(&self, exp: &Experimental) -> bool {
        let key = match exp {
            Experimental::WholeTarget(target) => target.name().to_string(),
            Experimental::Feature { target, name } => format!("{}.{}", target.name(), name),
        };
        self.experimental.contains(&key)
    }

    pub fn typescript_experimental(
        &self,
    ) -> boltffi_bindgen::render::typescript::TypeScriptExperimental {
        boltffi_bindgen::render::typescript::TypeScriptExperimental {
            async_streams: self.is_experimental_enabled(&Experimental::Feature {
                target: Target::TypeScript,
                name: "async_streams",
            }),
        }
    }

    pub fn java_package(&self) -> String {
        self.targets
            .java
            .package
            .clone()
            .unwrap_or_else(|| format!("com.example.{}", self.package.name.replace('-', "_")))
    }

    pub fn java_module_name(&self) -> String {
        self.targets
            .java
            .module_name
            .clone()
            .unwrap_or_else(|| to_pascal_case(&self.package.name))
    }

    pub fn java_min_version(&self) -> Option<u8> {
        self.targets.java.min_version
    }

    pub fn java_jvm_output(&self) -> PathBuf {
        self.targets.java.jvm.output.clone()
    }

    pub fn java_jvm_strip_symbols(&self) -> bool {
        self.targets.java.jvm.strip_symbols
    }

    pub fn java_jvm_debug_symbols_enabled(&self) -> bool {
        self.targets.java.jvm.debug_symbols.enabled
    }

    pub fn java_jvm_debug_symbols_output(&self) -> PathBuf {
        self.targets
            .java
            .jvm
            .debug_symbols
            .output
            .clone()
            .unwrap_or_else(|| self.targets.java.jvm.output.join("symbols"))
    }

    pub fn java_jvm_debug_symbols_format(&self) -> DebugSymbolsFormat {
        self.targets.java.jvm.debug_symbols.format
    }

    pub fn java_jvm_debug_symbols_bundle(&self) -> DebugSymbolsBundle {
        self.targets.java.jvm.debug_symbols.bundle
    }

    pub fn java_jvm_requested_host_targets(&self) -> &[JavaHostTarget] {
        self.targets
            .java
            .jvm
            .host_targets
            .as_deref()
            .unwrap_or(JavaHostTarget::DEFAULTS)
    }

    pub fn java_jvm_host_targets(&self) -> std::result::Result<Vec<JavaHostTarget>, String> {
        resolve_java_host_targets(self.java_jvm_requested_host_targets())
    }

    pub fn java_android_output(&self) -> PathBuf {
        self.targets.java.android.output.clone()
    }

    pub fn python_output(&self) -> PathBuf {
        self.targets.python.output.clone()
    }

    pub fn python_module_name(&self) -> String {
        self.targets
            .python
            .module_name
            .clone()
            .unwrap_or_else(|| self.crate_artifact_name())
    }

    pub fn python_wheel_output(&self) -> PathBuf {
        self.targets
            .python
            .wheel
            .output
            .clone()
            .unwrap_or_else(|| self.python_output().join("wheelhouse"))
    }

    pub fn python_wheel_interpreters(&self) -> Option<&[String]> {
        self.targets.python.wheel.interpreters.as_deref()
    }

    pub fn csharp_output(&self) -> PathBuf {
        self.targets.csharp.output.clone()
    }

    pub fn csharp_package_id(&self) -> String {
        self.targets
            .csharp
            .package_id
            .clone()
            .unwrap_or_else(|| self.package.name.clone())
    }

    pub fn csharp_target_framework(&self) -> String {
        self.targets
            .csharp
            .target_framework
            .clone()
            .unwrap_or_else(|| "net10.0".to_string())
    }

    pub fn csharp_package_output(&self) -> PathBuf {
        self.targets
            .csharp
            .package_output
            .clone()
            .unwrap_or_else(|| self.csharp_output().join("packages"))
    }

    pub fn csharp_requested_runtime_identifiers(&self) -> &[CSharpRuntimeIdentifier] {
        self.targets
            .csharp
            .runtime_identifiers
            .as_deref()
            .unwrap_or(CSharpRuntimeIdentifier::DEFAULTS)
    }

    pub fn csharp_runtime_identifiers(
        &self,
    ) -> std::result::Result<Vec<CSharpRuntimeIdentifier>, String> {
        CSharpRuntimeIdentifier::resolve_requested(self.csharp_requested_runtime_identifiers())
    }

    pub fn wasm_triple(&self) -> &str {
        &self.targets.wasm.triple
    }

    pub fn wasm_profile(&self) -> WasmProfile {
        self.targets.wasm.profile
    }

    pub fn wasm_output(&self) -> PathBuf {
        self.targets.wasm.output.clone()
    }

    pub fn wasm_artifact_path(&self, profile: WasmProfile) -> PathBuf {
        self.targets.wasm.artifact_path.clone().unwrap_or_else(|| {
            PathBuf::from("target")
                .join(self.wasm_triple())
                .join(profile.as_str())
                .join(format!("{}.wasm", self.crate_artifact_name()))
        })
    }

    pub fn wasm_has_artifact_path_override(&self) -> bool {
        self.targets.wasm.artifact_path.is_some()
    }

    pub fn wasm_optimize_enabled(&self, profile: WasmProfile) -> bool {
        self.targets
            .wasm
            .optimize
            .enabled
            .unwrap_or(matches!(profile, WasmProfile::Release))
    }

    pub fn wasm_optimize_level(&self) -> WasmOptimizeLevel {
        self.targets
            .wasm
            .optimize
            .level
            .unwrap_or(WasmOptimizeLevel::Size)
    }

    pub fn wasm_optimize_strip_debug(&self) -> bool {
        self.targets.wasm.optimize.strip_debug.unwrap_or(true)
    }

    pub fn wasm_optimize_on_missing(&self) -> WasmOptimizeOnMissing {
        self.targets
            .wasm
            .optimize
            .on_missing
            .unwrap_or(WasmOptimizeOnMissing::Error)
    }

    pub fn wasm_typescript_output(&self) -> PathBuf {
        self.targets
            .wasm
            .typescript
            .output
            .clone()
            .unwrap_or_else(|| self.targets.wasm.output.join("pkg"))
    }

    pub fn wasm_runtime_package(&self) -> String {
        self.targets
            .wasm
            .typescript
            .runtime_package
            .clone()
            .unwrap_or_else(|| "@boltffi/runtime".to_string())
    }

    pub fn wasm_runtime_version(&self) -> String {
        self.targets
            .wasm
            .typescript
            .runtime_version
            .clone()
            .unwrap_or_else(|| "*".to_string())
    }

    pub fn wasm_typescript_module_name(&self) -> String {
        self.targets
            .wasm
            .typescript
            .module_name
            .clone()
            .unwrap_or_else(|| normalize_module_name(&self.package.name))
    }

    pub fn wasm_source_map_enabled(&self) -> bool {
        self.targets.wasm.typescript.source_map.unwrap_or(true)
    }

    #[allow(dead_code)]
    pub fn wasm_typescript_type_mappings(&self) -> &HashMap<String, TypeMapping> {
        &self.targets.wasm.typescript.type_mappings
    }

    pub fn wasm_npm_package_name(&self) -> Option<&str> {
        self.targets.wasm.npm.package_name.as_deref()
    }

    pub fn wasm_npm_output(&self) -> PathBuf {
        self.targets
            .wasm
            .npm
            .output
            .clone()
            .unwrap_or_else(|| self.wasm_typescript_output())
    }

    pub fn wasm_npm_targets(&self) -> Vec<WasmNpmTarget> {
        self.targets.wasm.npm.targets.clone().unwrap_or_else(|| {
            vec![
                WasmNpmTarget::Bundler,
                WasmNpmTarget::Web,
                WasmNpmTarget::Nodejs,
            ]
        })
    }

    pub fn wasm_npm_generate_package_json(&self) -> bool {
        self.targets.wasm.npm.generate_package_json.unwrap_or(true)
    }

    pub fn wasm_npm_generate_readme(&self) -> bool {
        self.targets.wasm.npm.generate_readme.unwrap_or(true)
    }

    pub fn package_version(&self) -> Option<String> {
        self.package
            .version
            .clone()
            .or_else(|| cargo_package_field("version"))
    }

    pub fn wasm_npm_version(&self) -> Option<String> {
        self.targets
            .wasm
            .npm
            .version
            .clone()
            .or_else(|| self.package_version())
    }

    pub fn package_license(&self) -> Option<String> {
        self.package
            .license
            .clone()
            .or_else(|| cargo_package_field("license"))
    }

    pub fn wasm_npm_license(&self) -> Option<String> {
        self.targets
            .wasm
            .npm
            .license
            .clone()
            .or_else(|| self.package_license())
    }

    pub fn package_repository(&self) -> Option<String> {
        self.package
            .repository
            .clone()
            .or_else(|| cargo_package_field("repository"))
    }

    pub fn wasm_npm_repository(&self) -> Option<String> {
        self.targets
            .wasm
            .npm
            .repository
            .clone()
            .or_else(|| self.package_repository())
    }

    pub fn dart_output(&self) -> PathBuf {
        self.targets.dart.output.clone()
    }

    pub fn dart_native_architectures(&self) -> &[DartNativeArchitecture] {
        self.targets
            .dart
            .native_architectures
            .as_deref()
            .unwrap_or(DartNativeArchitecture::ALL)
    }

    pub fn dart_targets(&self) -> Vec<RustTarget> {
        resolve_dart_native_targets(self.dart_native_architectures())
    }
}

fn normalize_module_name(input: &str) -> String {
    let normalized = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("_{}", normalized)
    } else {
        normalized
    }
}

fn validate_unique<T, F>(
    values: Option<&[T]>,
    field_name: &str,
    canonical_name: F,
) -> Result<(), ConfigError>
where
    T: Copy + Eq + std::hash::Hash,
    F: Fn(T) -> &'static str,
{
    let Some(values) = values else {
        return Ok(());
    };

    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(*value) {
            return Err(ConfigError::Validation(format!(
                "{field_name} contains duplicate architecture '{}'",
                canonical_name(*value)
            )));
        }
    }

    Ok(())
}

fn to_pascal_case(input: &str) -> String {
    input
        .split(['_', '-'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn cargo_package_field(field_name: &str) -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find_map(|line| parse_key_value(line).filter(|(key, _)| key == field_name))
        })
        .map(|(_, value)| value)
}

fn parse_key_value(line: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = line.split_once('=')?;
    let key = raw_key.trim().to_string();
    let value = raw_value.trim().trim_matches('"').to_string();
    Some((key, value))
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config from {path}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config from {path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },

    #[error("failed to parse merged config from {base_path} with overlay {overlay_path}: {source}")]
    ParseMerged {
        base_path: PathBuf,
        overlay_path: PathBuf,
        source: Box<toml::de::Error>,
    },

    #[error("invalid config: {0}")]
    Validation(String),

    #[error("failed to serialize config")]
    Serialize(#[source] toml::ser::Error),

    #[error("failed to write config to {path}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn parse_config_with_overlay(base: &str, overlay: &str) -> Config {
        let mut merged: toml::Value = toml::from_str(base).expect("base toml parse failed");
        let overlay: toml::Value = toml::from_str(overlay).expect("overlay toml parse failed");
        merge_toml_value(&mut merged, overlay);

        let parsed: Config = merged.try_into().expect("merged config parse failed");
        parsed.validate().expect("merged config validation failed");
        parsed
    }

    #[test]
    fn parses_targets_apple_table() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
deployment_target = "16.0"
include_macos = false
"#,
        );

        assert_eq!(config.targets.apple.deployment_target, "16.0");
        assert!(!config.targets.apple.include_macos);
    }

    #[test]
    fn defaults_apple_architectures_to_ios_and_simulator_defaults() {
        let config = parse_config(
            r#"
[package]
name = "mylib"
"#,
        );

        assert_eq!(
            config
                .apple_ios_architectures()
                .iter()
                .map(|architecture| architecture.canonical_name())
                .collect::<Vec<_>>(),
            vec!["arm64"]
        );
        assert_eq!(
            config
                .apple_simulator_architectures()
                .iter()
                .map(|architecture| architecture.canonical_name())
                .collect::<Vec<_>>(),
            vec!["arm64", "x86_64"]
        );
        assert_eq!(
            config
                .apple_targets()
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec![
                "aarch64-apple-ios",
                "aarch64-apple-ios-sim",
                "x86_64-apple-ios",
            ]
        );
    }

    #[test]
    fn includes_macos_targets_only_when_enabled() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
include_macos = true
"#,
        );

        assert_eq!(
            config
                .apple_targets()
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec![
                "aarch64-apple-ios",
                "aarch64-apple-ios-sim",
                "x86_64-apple-ios",
                "aarch64-apple-darwin",
                "x86_64-apple-darwin",
            ]
        );
    }

    #[test]
    fn ignores_macos_architectures_when_macos_is_disabled() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
macos_architectures = ["x86_64"]
"#,
        );

        assert_eq!(
            config
                .apple_targets()
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec![
                "aarch64-apple-ios",
                "aarch64-apple-ios-sim",
                "x86_64-apple-ios",
            ]
        );
    }

    #[test]
    fn allows_empty_macos_architectures_when_macos_is_disabled() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple]
include_macos = false
macos_architectures = []
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn allows_empty_apple_ios_architectures_when_other_apple_slices_remain() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
ios_architectures = []
"#,
        );

        assert!(config.apple_ios_architectures().is_empty());
        assert_eq!(
            config
                .apple_targets()
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec!["aarch64-apple-ios-sim", "x86_64-apple-ios"]
        );
    }

    #[test]
    fn rejects_empty_apple_target_matrix() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple]
ios_architectures = []
simulator_architectures = []
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message
                    == "Apple packaging requires at least one configured slice across targets.apple.ios_architectures, targets.apple.simulator_architectures, or enabled macOS architectures"
        ));
    }

    #[test]
    fn rejects_duplicate_apple_simulator_architectures() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple]
simulator_architectures = ["arm64", "arm64"]
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message
                    == "targets.apple.simulator_architectures contains duplicate architecture 'arm64'"
        ));
    }

    #[test]
    fn rejects_invalid_apple_ios_architectures() {
        let parsed = toml::from_str::<Config>(
            r#"
[package]
name = "mylib"

[targets.apple]
ios_architectures = ["x86_64"]
"#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn defaults_android_architectures_to_all_supported_abis() {
        let config = parse_config(
            r#"
[package]
name = "mylib"
"#,
        );

        assert_eq!(
            config
                .android_architectures()
                .iter()
                .map(|architecture| architecture.canonical_name())
                .collect::<Vec<_>>(),
            vec!["arm64", "armv7", "x86_64", "x86"]
        );
    }

    #[test]
    fn rejects_noncanonical_android_architecture_aliases() {
        let parsed = toml::from_str::<Config>(
            r#"
[package]
name = "mylib"

[targets.android]
architectures = ["arm"]
"#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn rejects_empty_android_architectures() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.android]
architectures = []
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message == "targets.android.architectures must be non-empty when provided"
        ));
    }

    #[test]
    fn rejects_duplicate_android_architectures() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.android]
architectures = ["armv7", "armv7"]
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message
                    == "targets.android.architectures contains duplicate architecture 'armv7'"
        ));
    }

    #[test]
    fn defaults_java_jvm_host_targets_to_current() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
"#,
        );

        assert_eq!(
            config
                .java_jvm_requested_host_targets()
                .iter()
                .map(|target| target.canonical_name())
                .collect::<Vec<_>>(),
            vec!["current"]
        );
        assert_eq!(
            config
                .java_jvm_host_targets()
                .expect("resolved current host")
                .len(),
            1
        );
    }

    #[test]
    fn resolves_default_debug_symbols_outputs() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
"#,
        );

        assert_eq!(
            config.apple_debug_symbols_output(),
            PathBuf::from("dist/apple/symbols")
        );
        assert_eq!(
            config.android_debug_symbols_output(),
            PathBuf::from("dist/android/symbols")
        );
        assert_eq!(
            config.java_jvm_debug_symbols_output(),
            PathBuf::from("dist/java/symbols")
        );
    }

    #[test]
    fn parses_target_debug_symbols_configuration() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple.debug_symbols]
enabled = true
output = "dist/apple/debug-artifacts"
format = "zip"
bundle = "unstripped"

[targets.android.debug_symbols]
enabled = true
output = "dist/android/debug-artifacts"

[targets.java.jvm]
enabled = true

[targets.java.jvm.debug_symbols]
enabled = true
output = "dist/java/debug-artifacts"
"#,
        );

        assert!(config.apple_debug_symbols_enabled());
        assert!(config.android_debug_symbols_enabled());
        assert!(config.java_jvm_debug_symbols_enabled());
        assert_eq!(
            config.apple_debug_symbols_output(),
            PathBuf::from("dist/apple/debug-artifacts")
        );
        assert_eq!(
            config.android_debug_symbols_output(),
            PathBuf::from("dist/android/debug-artifacts")
        );
        assert_eq!(
            config.java_jvm_debug_symbols_output(),
            PathBuf::from("dist/java/debug-artifacts")
        );
        assert_eq!(config.apple_debug_symbols_format(), DebugSymbolsFormat::Zip);
        assert_eq!(
            config.apple_debug_symbols_bundle(),
            DebugSymbolsBundle::Unstripped
        );
    }

    #[test]
    fn parses_java_jvm_host_target_aliases() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
host_targets = ["current", "darwin-aarch64", "linux-x86-64", "windows-x86-64"]
"#,
        );

        assert_eq!(
            config
                .java_jvm_requested_host_targets()
                .iter()
                .map(|target| target.canonical_name())
                .collect::<Vec<_>>(),
            vec!["current", "darwin-arm64", "linux-x86_64", "windows-x86_64"]
        );
    }

    #[test]
    fn rejects_empty_java_jvm_host_targets() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
host_targets = []
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message == "targets.java.jvm.host_targets must be non-empty when provided"
        ));
    }

    #[test]
    fn resolves_current_java_host_targets_with_deduping() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let current_host_value = current_host.canonical_name();
        let config = parse_config(&format!(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
host_targets = ["current", "{current_host_value}"]
"#
        ));

        assert_eq!(
            config
                .java_jvm_host_targets()
                .expect("resolved current host"),
            vec![current_host]
        );
    }

    #[test]
    fn preserves_explicit_cross_host_java_targets() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let explicit_other_host = [
            JavaHostTarget::DarwinArm64,
            JavaHostTarget::DarwinX86_64,
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::LinuxAarch64,
            JavaHostTarget::WindowsX86_64,
        ]
        .into_iter()
        .find(|target| *target != current_host)
        .expect("alternate host target");
        let config = parse_config(&format!(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
host_targets = ["current", "{}"]
"#,
            explicit_other_host.canonical_name()
        ));

        assert_eq!(
            config
                .java_jvm_host_targets()
                .expect("resolved host targets"),
            vec![current_host, explicit_other_host]
        );
    }

    #[test]
    fn rejects_empty_wasm_npm_targets() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.wasm.npm]
targets = []
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_err());
    }

    #[test]
    fn rejects_remote_spm_without_repo_url() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple.spm]
distribution = "remote"
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_err());
    }

    #[test]
    fn allows_remote_spm_without_repo_url_when_apple_disabled() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.apple]
enabled = false

[targets.apple.spm]
distribution = "remote"
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn allows_empty_wasm_npm_targets_when_wasm_disabled() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[targets.wasm]
enabled = false

[targets.wasm.npm]
targets = []
"#,
        )
        .expect("toml parse failed");

        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn rejects_legacy_top_level_target_tables() {
        let parsed = toml::from_str::<Config>(
            r#"
[package]
name = "mylib"

[apple]
deployment_target = "16.0"
"#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn merges_global_and_command_specific_cargo_args() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[cargo]
global_args = ["--locked"]

[cargo.command_args]
build = ["--features", "mobile"]
"#,
        );

        assert_eq!(
            config.cargo_args_for_command("build"),
            vec![
                "--locked".to_string(),
                "--features".to_string(),
                "mobile".to_string()
            ]
        );
    }

    #[test]
    fn returns_global_cargo_args_when_command_specific_is_missing() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[cargo]
global_args = ["--frozen"]
"#,
        );

        assert_eq!(
            config.cargo_args_for_command("test"),
            vec!["--frozen".to_string()]
        );
    }

    #[test]
    fn overlay_can_override_android_architectures_while_inheriting_other_fields() {
        let config = parse_config_with_overlay(
            r#"
[package]
name = "mylib"

[targets.android]
min_sdk = 26
output = "dist/android-release"
architectures = ["arm64", "armv7"]
"#,
            r#"
[targets.android]
architectures = ["arm64"]
"#,
        );

        assert_eq!(config.android_min_sdk(), 26);
        assert_eq!(
            config.android_output(),
            PathBuf::from("dist/android-release")
        );
        assert_eq!(
            config
                .android_architectures()
                .iter()
                .map(|architecture| architecture.canonical_name())
                .collect::<Vec<_>>(),
            vec!["arm64"]
        );
    }

    #[test]
    fn overlay_can_clear_ios_architectures_for_simulator_only_apple_packaging() {
        let config = parse_config_with_overlay(
            r#"
[package]
name = "mylib"

[targets.apple]
ios_architectures = ["arm64"]
simulator_architectures = ["arm64", "x86_64"]
"#,
            r#"
[targets.apple]
ios_architectures = []
"#,
        );

        assert!(config.apple_ios_architectures().is_empty());
        assert_eq!(
            config
                .apple_simulator_architectures()
                .iter()
                .map(|architecture| architecture.canonical_name())
                .collect::<Vec<_>>(),
            vec!["arm64", "x86_64"]
        );
        assert_eq!(
            config
                .apple_targets()
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec!["aarch64-apple-ios-sim", "x86_64-apple-ios"]
        );
    }

    #[test]
    fn overlay_can_override_java_host_targets_while_inheriting_other_fields() {
        let config = parse_config_with_overlay(
            r#"
[package]
name = "mylib"

[targets.android]
min_sdk = 29

[targets.java]
package = "com.example.base"

[targets.java.jvm]
enabled = true
host_targets = ["current", "linux-x86_64"]
"#,
            r#"
[targets.java.jvm]
host_targets = ["linux-x86_64"]
"#,
        );

        assert_eq!(config.android_min_sdk(), 29);
        assert_eq!(config.java_package(), "com.example.base");
        assert_eq!(
            config
                .java_jvm_requested_host_targets()
                .iter()
                .map(|target| target.canonical_name())
                .collect::<Vec<_>>(),
            vec!["linux-x86_64"]
        );
    }

    #[test]
    fn overlay_deep_merges_nested_tables() {
        let config = parse_config_with_overlay(
            r#"
[package]
name = "mylib"

[targets.apple.spm]
distribution = "remote"
repo_url = "https://example.com/base.git"
package_name = "BaseKit"
"#,
            r#"
[targets.apple.spm]
package_name = "OverlayKit"
"#,
        );

        assert_eq!(config.apple_spm_distribution(), SpmDistribution::Remote);
        assert_eq!(
            config.apple_spm_repo_url(),
            Some("https://example.com/base.git")
        );
        assert_eq!(config.apple_spm_package_name(), Some("OverlayKit"));
    }

    #[test]
    fn marks_python_as_experimental_target() {
        assert!(Experimental::is_target_experimental(Target::Python));
    }

    #[test]
    fn does_not_mark_csharp_as_experimental_target() {
        assert!(!Experimental::is_target_experimental(Target::CSharp));
    }

    #[test]
    fn python_should_process_requires_opt_in() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.python]
enabled = true
"#,
        );

        assert!(!config.should_process(Target::Python, false));
        assert!(config.should_process(Target::Python, true));
    }

    #[test]
    fn python_should_process_accepts_config_opt_in() {
        let config = parse_config(
            r#"
experimental = ["python"]

[package]
name = "mylib"

[targets.python]
enabled = true
"#,
        );

        assert!(config.should_process(Target::Python, false));
    }

    #[test]
    fn python_module_name_defaults_to_crate_artifact_name() {
        let config = parse_config(
            r#"
[package]
name = "my-lib"

[targets.python]
enabled = true
"#,
        );

        assert_eq!(config.python_module_name(), "my_lib");
        assert_eq!(
            config.python_wheel_output(),
            PathBuf::from("dist/python/wheelhouse")
        );
        assert_eq!(config.python_wheel_interpreters(), None);
    }

    #[test]
    fn python_wheel_configuration_supports_module_override_and_interpreter_matrix() {
        let config = parse_config(
            r#"
[package]
name = "my-lib"

[targets.python]
enabled = true
module_name = "demo_runtime"

[targets.python.wheel]
output = "dist/python/wheels"
interpreters = ["python3.11", "python3.12"]
"#,
        );

        assert_eq!(config.python_module_name(), "demo_runtime");
        assert_eq!(
            config.python_wheel_output(),
            PathBuf::from("dist/python/wheels")
        );
        assert_eq!(
            config.python_wheel_interpreters(),
            Some(["python3.11".to_string(), "python3.12".to_string()].as_slice())
        );
    }

    #[test]
    fn rejects_empty_python_interpreter_matrix() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "my-lib"

[targets.python]
enabled = true

[targets.python.wheel]
interpreters = []
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message.contains("targets.python.wheel.interpreters must be non-empty")
        ));
    }

    #[test]
    fn rejects_duplicate_python_interpreters() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "my-lib"

[targets.python]
enabled = true

[targets.python.wheel]
interpreters = ["python3.13", "python3.13"]
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message.contains("targets.python.wheel.interpreters contains duplicate interpreter")
        ));
    }

    #[test]
    fn rejects_invalid_python_module_name_override() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "my-lib"

[targets.python]
enabled = true
module_name = "demo-runtime"
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message.contains("targets.python.module_name must be a valid Python identifier")
        ));
    }

    #[test]
    fn accepts_legacy_python_pack_table_alias() {
        let config = parse_config(
            r#"
[package]
name = "my-lib"

[targets.python]
enabled = true

[targets.python.pack]
wheel_output = "dist/python/wheels"
interpreters = ["python3.11"]
"#,
        );

        assert_eq!(
            config.python_wheel_output(),
            PathBuf::from("dist/python/wheels")
        );
        assert_eq!(
            config.python_wheel_interpreters(),
            Some(["python3.11".to_string()].as_slice())
        );
    }

    #[test]
    fn csharp_configuration_defaults_for_packaging() {
        let config = parse_config(
            r#"
[package]
name = "my-lib"

[targets.csharp]
enabled = true
"#,
        );

        assert_eq!(config.csharp_output(), PathBuf::from("dist/csharp"));
        assert_eq!(
            config.csharp_package_output(),
            PathBuf::from("dist/csharp/packages")
        );
        assert_eq!(config.csharp_package_id(), "my-lib");
        assert_eq!(config.csharp_target_framework(), "net10.0");
        assert_eq!(
            config.csharp_requested_runtime_identifiers(),
            crate::target::CSharpRuntimeIdentifier::DEFAULTS
        );
        assert!(config.should_process(Target::CSharp, false));
    }

    #[test]
    fn csharp_configuration_supports_package_and_runtime_matrix() {
        let config = parse_config(
            r#"
[package]
name = "my-lib"

[targets.csharp]
enabled = true
output = "artifacts/csharp"
package_output = "artifacts/nuget"
package_id = "Company.MyLib"
target_framework = "net9.0"
runtime_identifiers = ["current", "linux-x64"]
"#,
        );

        assert_eq!(config.csharp_output(), PathBuf::from("artifacts/csharp"));
        assert_eq!(
            config.csharp_package_output(),
            PathBuf::from("artifacts/nuget")
        );
        assert_eq!(config.csharp_package_id(), "Company.MyLib");
        assert_eq!(config.csharp_target_framework(), "net9.0");
        assert_eq!(
            config.csharp_requested_runtime_identifiers(),
            &[
                crate::target::CSharpRuntimeIdentifier::Current,
                crate::target::CSharpRuntimeIdentifier::LinuxX64
            ]
        );
        assert!(config.should_process(Target::CSharp, false));
    }

    #[test]
    fn rejects_empty_csharp_runtime_identifier_matrix() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "my-lib"

[targets.csharp]
enabled = true
runtime_identifiers = []
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message.contains("targets.csharp.runtime_identifiers must be non-empty")
        ));
    }

    #[test]
    fn rejects_duplicate_csharp_runtime_identifiers() {
        let parsed: Config = toml::from_str(
            r#"
[package]
name = "my-lib"

[targets.csharp]
enabled = true
runtime_identifiers = ["linux-x64", "linux-x64"]
"#,
        )
        .expect("toml parse failed");

        assert!(matches!(
            parsed.validate(),
            Err(ConfigError::Validation(message))
                if message.contains("targets.csharp.runtime_identifiers contains duplicate runtime identifier")
        ));
    }
}
