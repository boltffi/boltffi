use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KotlinMultiplatformConfig {
    #[serde(default = "default_kotlin_multiplatform_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    /// Allows KMP generation to omit unsupported declarations instead of
    /// failing. This is intentionally explicit because it changes the public
    /// API shape of the generated commonMain surface.
    #[serde(default)]
    pub preview_prune_unsupported: bool,
    pub package: Option<String>,
    pub module_name: Option<String>,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for KotlinMultiplatformConfig {
    fn default() -> Self {
        Self {
            output: default_kotlin_multiplatform_output(),
            enabled: false,
            preview_prune_unsupported: false,
            package: None,
            module_name: None,
            cargo_args: Vec::new(),
        }
    }
}

fn default_kotlin_multiplatform_output() -> PathBuf {
    PathBuf::from("dist/kotlin-multiplatform")
}
