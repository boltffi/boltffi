use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::target::CSharpRuntimeIdentifier;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CSharpConfig {
    #[serde(default = "default_csharp_output")]
    pub output: PathBuf,
    pub namespace: Option<String>,
    pub package_id: Option<String>,
    pub target_framework: Option<String>,
    pub package_output: Option<PathBuf>,
    pub runtime_identifiers: Option<Vec<CSharpRuntimeIdentifier>>,
    #[serde(default)]
    pub nuget: CSharpNugetConfig,
    #[serde(default)]
    pub enabled: bool,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for CSharpConfig {
    fn default() -> Self {
        Self {
            output: default_csharp_output(),
            namespace: None,
            package_id: None,
            target_framework: None,
            package_output: None,
            runtime_identifiers: None,
            nuget: CSharpNugetConfig::default(),
            enabled: false,
            cargo_args: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CSharpNugetConfig {
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub owners: Option<Vec<String>>,
    pub project_url: Option<String>,
    pub repository_url: Option<String>,
    pub repository_type: Option<String>,
    pub license_expression: Option<String>,
    pub icon: Option<PathBuf>,
    pub readme: Option<PathBuf>,
    pub tags: Option<Vec<String>>,
    pub release_notes: Option<String>,
    pub require_license_acceptance: Option<bool>,
}

fn default_csharp_output() -> PathBuf {
    PathBuf::from("dist/csharp")
}
