use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct DebugSymbolsConfig {
    pub enabled: bool,
    pub output: Option<PathBuf>,
    pub format: DebugSymbolsFormat,
    pub bundle: DebugSymbolsBundle,
    pub standalone_archive: bool,
}

impl Default for DebugSymbolsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: None,
            format: DebugSymbolsFormat::default(),
            bundle: DebugSymbolsBundle::default(),
            standalone_archive: true,
        }
    }
}

impl DebugSymbolsConfig {
    pub fn archive_enabled(&self) -> bool {
        self.enabled && self.standalone_archive
    }
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
