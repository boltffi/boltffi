use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RubyGemPlatform {
    #[serde(rename = "current")]
    Current,
    #[serde(rename = "arm64-darwin")]
    Arm64Darwin,
    #[serde(rename = "x86_64-darwin")]
    X86_64Darwin,
    #[serde(rename = "x86_64-linux-gnu")]
    X86_64LinuxGnu,
    #[serde(rename = "aarch64-linux-gnu")]
    Aarch64LinuxGnu,
    #[serde(rename = "x86_64-linux-musl")]
    X86_64LinuxMusl,
    #[serde(rename = "aarch64-linux-musl")]
    Aarch64LinuxMusl,
}

impl std::str::FromStr for RubyGemPlatform {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "current" => Ok(Self::Current),
            "arm64-darwin" => Ok(Self::Arm64Darwin),
            "x86_64-darwin" => Ok(Self::X86_64Darwin),
            "x86_64-linux-gnu" => Ok(Self::X86_64LinuxGnu),
            "aarch64-linux-gnu" => Ok(Self::Aarch64LinuxGnu),
            "x86_64-linux-musl" => Ok(Self::X86_64LinuxMusl),
            "aarch64-linux-musl" => Ok(Self::Aarch64LinuxMusl),
            _ => Err(format!(
                "invalid Ruby platform: '{}'. Supported: current, arm64-darwin, x86_64-darwin, x86_64-linux-gnu, aarch64-linux-gnu, x86_64-linux-musl, aarch64-linux-musl",
                s
            )),
        }
    }
}

/// Ruby gem archive packaging.
///
/// `boltffi pack ruby` builds the Rust staticlib with BoltFFI's IR expansion
/// environment so the archive symbols match generated `ext/<crate>/boltffi.h`.
/// Projects that bypass this command and build the staticlib manually must set
/// the same expansion environment or intentionally patch generated C/header
/// code to the legacy `#[boltffi::export]` symbol names.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RubyGemPackConfig {
    pub output: Option<PathBuf>,
    pub targets: Option<Vec<RubyGemPlatform>>,
    pub glibc_version: Option<String>,
    pub cargo_zigbuild: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RubyConfig {
    #[serde(default)]
    pub enabled: bool,
    pub output: Option<PathBuf>,  // default: "dist/ruby"
    pub gem_name: Option<String>, // default: crate name with - separators
    /// Future: emit ZJIT leaf-function hints in the C extension.
    /// Default false until FFX metadata is implemented.
    #[serde(default)]
    pub zjit_hints: bool,
    /// Emit `rb_ext_ractor_safe(true)` in `Init_`, declaring the extension safe
    /// to call from any Ractor. Opt-in: it asserts the bound Rust crate has no
    /// unsynchronized global mutable state, which BoltFFI cannot verify.
    #[serde(default)]
    pub ractor_safe: bool,
    /// Extra Ruby source files (`.rb`) to include in the generated package.
    /// Each path is relative to the crate root and is copied into the output
    /// under `lib/<crate_stem>/`. The generator adds `require_relative` lines
    /// to `lib/<crate>.rb` and `spec.files` entries to the gemspec
    /// automatically.
    ///
    /// All extra files land under `lib/` so the gemspec's `require_paths`
    /// stays at the RubyGems default `["lib"]`.
    /// See: <https://guides.rubygems.org/specification-reference/#require_paths=>
    #[serde(default)]
    pub extra_files: Vec<PathBuf>,
    #[serde(default, alias = "pack")]
    pub gem: RubyGemPackConfig,
}
