use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::TypeMapping;

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
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
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
            cargo_args: Vec::new(),
        }
    }
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

fn default_wasm_triple() -> String {
    "wasm32-unknown-unknown".to_string()
}

fn default_wasm_output() -> PathBuf {
    PathBuf::from("dist/wasm")
}

fn default_true() -> bool {
    true
}
