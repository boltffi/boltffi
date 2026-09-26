pub mod apple;
pub mod c;
pub mod c_header;
pub mod csharp;
pub mod dart;
pub mod java;
pub mod kmp;
pub mod kotlin;
pub mod python;
pub mod wasm;

pub use apple::{
    AppleConfig, SpmConfig, SpmDistribution, SpmLayout, SwiftConfig, XcframeworkConfig,
};
pub use c::CConfig;
pub use c_header::HeaderConfig;
pub use csharp::CSharpConfig;
#[cfg(test)]
pub use csharp::CSharpNugetConfig;
pub use dart::DartConfig;
pub use java::JavaConfig;
#[cfg(test)]
pub use java::JavaJvmConfig;
pub use kmp::KotlinMultiplatformConfig;
pub use kotlin::{
    AndroidConfig, AndroidLinkConfig, AndroidPackConfig, KotlinApiStyle, KotlinConfig,
    KotlinDesktopLoader, KotlinFactoryStyle,
};
pub use python::PythonConfig;
#[cfg(test)]
pub use python::PythonWheelConfig;
pub use wasm::{WasmConfig, WasmNpmTarget, WasmOptimizeLevel, WasmOptimizeOnMissing, WasmProfile};

use serde::{Deserialize, Serialize};

/// A `[targets.<name>]` table whose `cargo_args` apply when that target builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSection {
    Apple,
    Android,
    KotlinMultiplatform,
    Wasm,
    Java,
    Dart,
    Python,
    CSharp,
    C,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TargetsConfig {
    #[serde(default)]
    pub apple: AppleConfig,
    #[serde(default)]
    pub android: AndroidConfig,
    #[serde(default)]
    pub kotlin_multiplatform: KotlinMultiplatformConfig,
    #[serde(default)]
    pub wasm: WasmConfig,
    #[serde(default)]
    pub java: JavaConfig,
    #[serde(default)]
    pub dart: DartConfig,
    #[serde(default)]
    pub python: PythonConfig,
    #[serde(default)]
    pub csharp: CSharpConfig,
    #[serde(default)]
    pub c: CConfig,
}

impl TargetsConfig {
    pub fn cargo_args(&self, section: TargetSection) -> &[String] {
        match section {
            TargetSection::Apple => &self.apple.cargo_args,
            TargetSection::Android => &self.android.cargo_args,
            TargetSection::KotlinMultiplatform => &self.kotlin_multiplatform.cargo_args,
            TargetSection::Wasm => &self.wasm.cargo_args,
            TargetSection::Java => &self.java.cargo_args,
            TargetSection::Dart => &self.dart.cargo_args,
            TargetSection::Python => &self.python.cargo_args,
            TargetSection::CSharp => &self.csharp.cargo_args,
            TargetSection::C => &self.c.cargo_args,
        }
    }
}
