use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DartWebConfig {
    #[serde(default = "default_dart_web_output")]
    pub output: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    /// Dart module file name (`{module_name}.dart`). Defaults to the
    /// normalized package name, matching `targets.wasm.typescript`'s own
    /// `module_name` default — this target's generated Dart calls
    /// straight into that module's JS output, so the two should agree
    /// unless a project has a real reason to diverge.
    pub module_name: Option<String>,
}

impl Default for DartWebConfig {
    fn default() -> Self {
        Self {
            output: default_dart_web_output(),
            enabled: false,
            module_name: None,
        }
    }
}

fn default_dart_web_output() -> PathBuf {
    PathBuf::from("dist/dart_web")
}
