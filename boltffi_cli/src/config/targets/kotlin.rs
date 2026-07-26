use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::config::targets::HeaderConfig;
use crate::config::{DebugSymbolsConfig, ErrorStyle, TypeMapping};
use crate::target::{Architecture, Platform};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct KotlinConfig {
    pub package: Option<String>,
    pub output: Option<PathBuf>,
    pub module_name: Option<String>,
    pub library_name: Option<String>,
    #[serde(default)]
    pub desktop_loader: KotlinDesktopLoader,
    #[serde(default)]
    pub desktop_pack: KotlinDesktopPackConfig,
    #[serde(default)]
    pub api_style: KotlinApiStyle,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub factory_style: KotlinFactoryStyle,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct KotlinDesktopPackConfig {
    #[serde(default)]
    pub enabled: bool,
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
pub enum KotlinFactoryStyle {
    #[default]
    Constructors,
    CompanionMethods,
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
pub struct AndroidConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_android_output")]
    pub output: PathBuf,
    #[serde(default = "default_android_min_sdk")]
    pub min_sdk: u32,
    pub ndk_version: Option<String>,
    #[serde(default, deserialize_with = "AndroidConfig::deserialize_architectures")]
    pub architectures: Option<Vec<Architecture>>,
    #[serde(default)]
    pub kotlin: KotlinConfig,
    #[serde(default)]
    pub header: HeaderConfig,
    #[serde(default)]
    pub pack: AndroidPackConfig,
    #[serde(default)]
    pub debug_symbols: DebugSymbolsConfig,
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output: default_android_output(),
            min_sdk: default_android_min_sdk(),
            ndk_version: None,
            architectures: None,
            kotlin: KotlinConfig::default(),
            header: HeaderConfig::default(),
            pack: AndroidPackConfig::default(),
            debug_symbols: DebugSymbolsConfig::default(),
        }
    }
}

impl AndroidConfig {
    fn deserialize_architectures<'de, D>(
        deserializer: D,
    ) -> Result<Option<Vec<Architecture>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let architectures = Option::<Vec<Architecture>>::deserialize(deserializer)?;
        if let Some(architectures) = architectures.as_deref() {
            Self::ensure_supported_architectures(architectures)?;
        }
        Ok(architectures)
    }

    fn ensure_supported_architectures<E>(architectures: &[Architecture]) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        architectures
            .iter()
            .find(|architecture| !Platform::Android.architectures().contains(architecture))
            .map_or(Ok(()), |architecture| {
                Err(E::custom(format!(
                    "targets.android.architectures does not support {}",
                    architecture.canonical_name()
                )))
            })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AndroidPackConfig {
    pub output: Option<PathBuf>,
}

fn default_android_output() -> PathBuf {
    PathBuf::from("dist/android")
}

pub(super) fn default_android_min_sdk() -> u32 {
    24
}

fn default_true() -> bool {
    true
}
