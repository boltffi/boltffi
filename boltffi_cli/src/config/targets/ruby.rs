use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[targets.ruby]` configuration for the experimental Ruby target.
///
/// Unknown keys fail. BoltFFI 0.32 had a `version` key in this table. The
/// source gem takes its version from the starter gemspec, so a `version`
/// key now fails instead of doing nothing.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RubyConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Directory that receives the generated gem sources.
    #[serde(default = "default_ruby_output")]
    pub output: PathBuf,
    /// Gem name. Defaults to `package.name`.
    #[serde(default)]
    pub gem_name: Option<String>,
    /// Ruby module that the extension defines, such as `Demo` or
    /// `MyLib::Native`. Defaults to the package name in `UpperCamelCase`.
    #[serde(default)]
    pub module_name: Option<String>,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for RubyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_ruby_output(),
            gem_name: None,
            module_name: None,
            cargo_args: Vec::new(),
        }
    }
}

fn default_ruby_output() -> PathBuf {
    PathBuf::from("dist/ruby")
}
