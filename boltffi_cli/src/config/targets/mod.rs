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
    AndroidConfig, AndroidPackConfig, KotlinApiStyle, KotlinConfig, KotlinDesktopLoader,
    KotlinFactoryStyle,
};
pub use python::PythonConfig;
#[cfg(test)]
pub use python::PythonWheelConfig;
pub use wasm::{WasmConfig, WasmNpmTarget, WasmOptimizeLevel, WasmOptimizeOnMissing, WasmProfile};

use serde::{Deserialize, Serialize};

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
