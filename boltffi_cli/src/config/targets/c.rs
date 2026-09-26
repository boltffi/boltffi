use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[targets.c]` configuration for the experimental C host target.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CConfig {
    #[serde(default = "default_c_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for CConfig {
    fn default() -> Self {
        Self {
            output: default_c_output(),
            enabled: false,
            cargo_args: Vec::new(),
        }
    }
}

fn default_c_output() -> PathBuf {
    PathBuf::from("dist/c")
}
