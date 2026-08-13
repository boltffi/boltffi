use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[targets.c]` configuration for the experimental C host target.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CConfig {
    #[serde(default = "default_c_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for CConfig {
    fn default() -> Self {
        Self {
            output: default_c_output(),
            enabled: false,
        }
    }
}

fn default_c_output() -> PathBuf {
    PathBuf::from("dist/c")
}
