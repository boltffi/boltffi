use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PythonConfig {
    #[serde(default = "default_python_output")]
    pub output: PathBuf,
    pub module_name: Option<String>,
    pub python_requires: Option<String>,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    #[serde(default, alias = "pack")]
    pub wheel: PythonWheelConfig,
    #[serde(default)]
    pub enabled: bool,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            output: default_python_output(),
            module_name: None,
            python_requires: None,
            scripts: BTreeMap::new(),
            wheel: PythonWheelConfig::default(),
            enabled: false,
            cargo_args: Vec::new(),
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PythonWheelConfig {
    #[serde(alias = "wheel_output")]
    pub output: Option<PathBuf>,
    pub interpreters: Option<Vec<String>>,
}

fn default_python_output() -> PathBuf {
    PathBuf::from("dist/python")
}
