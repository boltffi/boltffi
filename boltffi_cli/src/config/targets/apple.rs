use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::config::{DebugSymbolsConfig, ErrorStyle, TypeMapping};
use crate::target::{Architecture, Platform};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SwiftConfig {
    pub module_name: Option<String>,
    pub output: Option<PathBuf>,
    pub ffi_module_name: Option<String>,
    pub tools_version: Option<String>,
    #[serde(default)]
    pub error_style: ErrorStyle,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMapping>,
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
    #[serde(
        default,
        deserialize_with = "AppleConfig::deserialize_ios_architectures"
    )]
    pub ios_architectures: Option<Vec<Architecture>>,
    #[serde(
        default,
        deserialize_with = "AppleConfig::deserialize_simulator_architectures"
    )]
    pub simulator_architectures: Option<Vec<Architecture>>,
    #[serde(
        default,
        deserialize_with = "AppleConfig::deserialize_macos_architectures"
    )]
    pub macos_architectures: Option<Vec<Architecture>>,
    #[serde(default)]
    pub swift: SwiftConfig,
    #[serde(default)]
    pub xcframework: XcframeworkConfig,
    #[serde(default)]
    pub spm: SpmConfig,
    #[serde(default)]
    pub debug_symbols: DebugSymbolsConfig,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
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
            swift: SwiftConfig::default(),
            xcframework: XcframeworkConfig::default(),
            spm: SpmConfig::default(),
            debug_symbols: DebugSymbolsConfig::default(),
            cargo_args: Vec::new(),
        }
    }
}

impl AppleConfig {
    fn deserialize_ios_architectures<'de, D>(
        deserializer: D,
    ) -> Result<Option<Vec<Architecture>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::deserialize_architectures(
            deserializer,
            Platform::Ios,
            "targets.apple.ios_architectures",
        )
    }

    fn deserialize_simulator_architectures<'de, D>(
        deserializer: D,
    ) -> Result<Option<Vec<Architecture>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::deserialize_architectures(
            deserializer,
            Platform::IosSimulator,
            "targets.apple.simulator_architectures",
        )
    }

    fn deserialize_macos_architectures<'de, D>(
        deserializer: D,
    ) -> Result<Option<Vec<Architecture>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::deserialize_architectures(
            deserializer,
            Platform::MacOs,
            "targets.apple.macos_architectures",
        )
    }

    fn deserialize_architectures<'de, D>(
        deserializer: D,
        platform: Platform,
        field: &'static str,
    ) -> Result<Option<Vec<Architecture>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let architectures = Option::<Vec<Architecture>>::deserialize(deserializer)?;
        if let Some(architectures) = architectures.as_deref() {
            Self::ensure_supported_architectures(platform, field, architectures)?;
        }
        Ok(architectures)
    }

    fn ensure_supported_architectures<E>(
        platform: Platform,
        field: &'static str,
        architectures: &[Architecture],
    ) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        architectures
            .iter()
            .find(|architecture| !platform.architectures().contains(architecture))
            .map_or(Ok(()), |architecture| {
                Err(E::custom(format!(
                    "{field} does not support {}",
                    architecture.canonical_name()
                )))
            })
    }
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

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SpmLayout {
    Bundled,
    Split,
    #[default]
    FfiOnly,
}

fn default_apple_output() -> PathBuf {
    PathBuf::from("dist/apple")
}

fn default_apple_deployment_target() -> String {
    "16.0".to_string()
}

fn default_true() -> bool {
    true
}
