use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::DebugSymbolsConfig;
use crate::config::targets::kotlin::default_android_min_sdk;
use crate::target::JavaHostTarget;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct JavaConfig {
    pub package: Option<String>,
    pub module_name: Option<String>,
    pub min_version: Option<u8>,
    #[serde(default)]
    pub jvm: JavaJvmConfig,
    #[serde(default)]
    pub android: JavaAndroidConfig,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
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
