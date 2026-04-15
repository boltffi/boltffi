use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Ios,
    IosSimulator,
    MacOs,
    Android,
    Wasm,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Architecture {
    Arm64,
    X86_64,
    Armv7,
    X86,
    Wasm32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppleIosArchitecture {
    #[serde(rename = "arm64")]
    Arm64,
}

impl AppleIosArchitecture {
    pub const ALL: &'static [Self] = &[Self::Arm64];

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
        }
    }

    pub const fn rust_target(self) -> RustTarget {
        match self {
            Self::Arm64 => RustTarget::IOS_ARM64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppleArchitecture {
    #[serde(rename = "arm64")]
    Arm64,
    #[serde(rename = "x86_64")]
    X86_64,
}

impl AppleArchitecture {
    pub const ALL: &'static [Self] = &[Self::Arm64, Self::X86_64];

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X86_64 => "x86_64",
        }
    }

    pub const fn simulator_rust_target(self) -> RustTarget {
        match self {
            Self::Arm64 => RustTarget::IOS_SIM_ARM64,
            Self::X86_64 => RustTarget::IOS_SIM_X86_64,
        }
    }

    pub const fn macos_rust_target(self) -> RustTarget {
        match self {
            Self::Arm64 => RustTarget::MACOS_ARM64,
            Self::X86_64 => RustTarget::MACOS_X86_64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AndroidArchitecture {
    #[serde(rename = "arm64")]
    Arm64,
    #[serde(rename = "armv7")]
    Armv7,
    #[serde(rename = "x86_64")]
    X86_64,
    #[serde(rename = "x86")]
    X86,
}

impl AndroidArchitecture {
    pub const ALL: &'static [Self] = &[Self::Arm64, Self::Armv7, Self::X86_64, Self::X86];

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::Armv7 => "armv7",
            Self::X86_64 => "x86_64",
            Self::X86 => "x86",
        }
    }

    pub const fn rust_target(self) -> RustTarget {
        match self {
            Self::Arm64 => RustTarget::ANDROID_ARM64,
            Self::Armv7 => RustTarget::ANDROID_ARMV7,
            Self::X86_64 => RustTarget::ANDROID_X86_64,
            Self::X86 => RustTarget::ANDROID_X86,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NativeHostPlatform {
    DarwinArm64,
    DarwinX86_64,
    LinuxX86_64,
    LinuxAarch64,
    WindowsX86_64,
}

impl NativeHostPlatform {
    pub const ALL: &'static [Self] = &[
        Self::DarwinArm64,
        Self::DarwinX86_64,
        Self::LinuxX86_64,
        Self::LinuxAarch64,
        Self::WindowsX86_64,
    ];

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::DarwinArm64 => "darwin-arm64",
            Self::DarwinX86_64 => "darwin-x86_64",
            Self::LinuxX86_64 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
            Self::WindowsX86_64 => "windows-x86_64",
        }
    }

    pub fn current() -> Option<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Some(Self::DarwinArm64),
            ("macos", "x86_64") => Some(Self::DarwinX86_64),
            ("linux", "x86_64") => Some(Self::LinuxX86_64),
            ("linux", "aarch64") => Some(Self::LinuxAarch64),
            ("windows", "x86_64") => Some(Self::WindowsX86_64),
            _ => None,
        }
    }

    pub fn shared_library_filename(self, artifact_name: &str) -> String {
        match self {
            Self::DarwinArm64 | Self::DarwinX86_64 => format!("lib{artifact_name}.dylib"),
            Self::LinuxX86_64 | Self::LinuxAarch64 => format!("lib{artifact_name}.so"),
            Self::WindowsX86_64 => format!("{artifact_name}.dll"),
        }
    }

    pub fn static_library_filename(self, artifact_name: &str) -> String {
        match self {
            Self::DarwinArm64 | Self::DarwinX86_64 | Self::LinuxX86_64 | Self::LinuxAarch64 => {
                format!("lib{artifact_name}.a")
            }
            Self::WindowsX86_64 => {
                if cfg!(all(target_os = "windows", target_env = "gnu")) {
                    format!("lib{artifact_name}.a")
                } else {
                    format!("{artifact_name}.lib")
                }
            }
        }
    }

    pub fn jni_library_filename(self, artifact_name: &str) -> String {
        match self {
            Self::DarwinArm64 | Self::DarwinX86_64 => format!("lib{artifact_name}_jni.dylib"),
            Self::LinuxX86_64 | Self::LinuxAarch64 => format!("lib{artifact_name}_jni.so"),
            Self::WindowsX86_64 => format!("{artifact_name}_jni.dll"),
        }
    }

    pub fn jni_platform(self) -> &'static str {
        match self {
            Self::DarwinArm64 | Self::DarwinX86_64 => "darwin",
            Self::LinuxX86_64 | Self::LinuxAarch64 => "linux",
            Self::WindowsX86_64 => "win32",
        }
    }

    pub fn rpath_flag(self) -> Option<&'static str> {
        match self {
            Self::DarwinArm64 | Self::DarwinX86_64 => Some("-Wl,-rpath,@loader_path"),
            Self::LinuxX86_64 | Self::LinuxAarch64 => Some("-Wl,-rpath,$ORIGIN"),
            Self::WindowsX86_64 => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DartNativeArchitecture {
    #[serde(rename = "android:arm64")]
    AndroidArm64,
    #[serde(rename = "android:armv7")]
    AndroidArmv7,
    #[serde(rename = "android:x86_64")]
    AndroidX86_64,
    #[serde(rename = "ios:arm64")]
    IosArm64,
    #[serde(rename = "ios_sim:arm64")]
    IosSimArm64,
    #[serde(rename = "ios_sim:x86_64")]
    IosSimX86_64,
    #[serde(rename = "linux:arm64")]
    LinuxArm64,
    #[serde(rename = "linux:x86_64")]
    LinuxX86_64,
    #[serde(rename = "macos:arm64")]
    MacosArm64,
    #[serde(rename = "macos:x86_64")]
    MacosX86_64,
}

impl DartNativeArchitecture {
    pub const ALL: &'static [Self] = &[
        Self::AndroidArm64,
        Self::AndroidArmv7,
        Self::AndroidX86_64,
        Self::IosArm64,
        Self::IosSimArm64,
        Self::IosSimX86_64,
        Self::LinuxArm64,
        Self::LinuxX86_64,
        Self::MacosArm64,
        Self::MacosX86_64,
    ];

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::AndroidArm64 => "android:arm64",
            Self::AndroidArmv7 => "android:armv7",
            Self::AndroidX86_64 => "android:x86_64",
            Self::IosArm64 => "ios:arm64",
            Self::IosSimArm64 => "ios_sim:arm64",
            Self::IosSimX86_64 => "ios_sim:x86_64",
            Self::LinuxArm64 => "linux:arm64",
            Self::LinuxX86_64 => "linux:x86_64",
            Self::MacosArm64 => "macos:arm64",
            Self::MacosX86_64 => "macos:x86_64",
        }
    }

    pub const fn rust_target(self) -> RustTarget {
        match self {
            Self::AndroidArm64 => RustTarget::ANDROID_ARM64,
            Self::AndroidArmv7 => RustTarget::ANDROID_ARMV7,
            Self::AndroidX86_64 => RustTarget::ANDROID_X86_64,
            Self::IosArm64 => RustTarget::IOS_ARM64,
            Self::IosSimArm64 => RustTarget::IOS_SIM_ARM64,
            Self::IosSimX86_64 => RustTarget::IOS_SIM_X86_64,
            Self::LinuxArm64 => RustTarget::LINUX_ARM64,
            Self::LinuxX86_64 => RustTarget::LINUX_X86_64,
            Self::MacosArm64 => RustTarget::MACOS_ARM64,
            Self::MacosX86_64 => RustTarget::MACOS_X86_64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JavaHostTarget {
    #[serde(rename = "current")]
    Current,
    #[serde(rename = "darwin-arm64", alias = "darwin-aarch64")]
    DarwinArm64,
    #[serde(rename = "darwin-x86_64", alias = "darwin-x86-64")]
    DarwinX86_64,
    #[serde(rename = "linux-x86_64", alias = "linux-x86-64")]
    LinuxX86_64,
    #[serde(rename = "linux-aarch64", alias = "linux-arm64")]
    LinuxAarch64,
    #[serde(rename = "windows-x86_64", alias = "windows-x86-64")]
    WindowsX86_64,
}

impl JavaHostTarget {
    pub const DEFAULTS: &'static [Self] = &[Self::Current];

    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Current => "current",
            resolved_target => resolved_target.native_host_platform().canonical_name(),
        }
    }

    pub fn current() -> Option<Self> {
        NativeHostPlatform::current().map(Into::into)
    }

    pub fn resolve_requested(targets: &[Self]) -> Result<Vec<Self>, String> {
        let current_host = Self::current().ok_or_else(Self::unsupported_host_message)?;
        let mut resolved = Vec::new();

        targets.iter().copied().for_each(|target| {
            let target = match target {
                Self::Current => current_host,
                explicit => explicit,
            };

            if !resolved.contains(&target) {
                resolved.push(target);
            }
        });

        Ok(resolved)
    }

    pub fn shared_library_filename(self, artifact_name: &str) -> String {
        self.native_host_platform()
            .shared_library_filename(artifact_name)
    }

    pub fn static_library_filename(self, artifact_name: &str) -> String {
        self.native_host_platform()
            .static_library_filename(artifact_name)
    }

    pub fn jni_library_filename(self, artifact_name: &str) -> String {
        self.native_host_platform()
            .jni_library_filename(artifact_name)
    }

    pub fn jni_platform(self) -> &'static str {
        self.native_host_platform().jni_platform()
    }

    pub fn rpath_flag(self) -> Option<&'static str> {
        self.native_host_platform().rpath_flag()
    }

}

/// A JVM packaging target entry, optionally carrying a minimum glibc version
/// for Linux targets.
///
/// Used in `targets.java.jvm.host_targets` in `boltffi.toml`.
///
/// # Syntax
///
/// | Config string           | Meaning                                          |
/// |-------------------------|--------------------------------------------------|
/// | `"linux-x86_64"`        | Linux x86-64, system-default glibc               |
/// | `"linux-x86_64.2.17"`   | Linux x86-64, glibc ≥ 2.17 (`cargo zigbuild`)   |
/// | `"linux-aarch64.2.28"`  | Linux aarch64, glibc ≥ 2.28 (`cargo zigbuild`)  |
/// | `"darwin-arm64"`        | macOS Apple Silicon (no glibc concept)           |
/// | `"current"`             | resolved to the current host at build time       |
///
/// The glibc version suffix is forwarded verbatim to the `--target` argument
/// of the cargo build command (e.g. `cargo zigbuild --target x86_64-unknown-linux-gnu.2.17`).
/// It has no effect on non-Linux targets; specifying it on a non-Linux target is an error.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JavaJvmHostTarget {
    pub target: JavaHostTarget,
    /// Minimum glibc version (e.g. `"2.17"`), Linux only.
    pub glibc_version: Option<String>,
}

impl JavaJvmHostTarget {
    pub const DEFAULTS: &'static [Self] = &[Self {
        target: JavaHostTarget::Current,
        glibc_version: None,
    }];

    pub fn resolve_requested(targets: &[Self]) -> Result<Vec<Self>, String> {
        let current_host = JavaHostTarget::current().ok_or_else(|| {
            "JVM packaging is only supported on darwin-arm64, darwin-x86_64, \
             linux-x86_64, linux-aarch64, and windows-x86_64 hosts"
                .to_string()
        })?;
        let mut resolved: Vec<Self> = Vec::new();

        for entry in targets {
            let resolved_target = match entry.target {
                JavaHostTarget::Current => current_host,
                explicit => explicit,
            };
            let candidate = Self {
                target: resolved_target,
                glibc_version: entry.glibc_version.clone(),
            };
            if !resolved.contains(&candidate) {
                resolved.push(candidate);
            }
        }

        Ok(resolved)
    }
}

#[cfg(test)]
impl JavaJvmHostTarget {
    pub fn new(target: JavaHostTarget) -> Self {
        Self {
            target,
            glibc_version: None,
        }
    }

    pub fn canonical_name(&self) -> String {
        let base = self.target.canonical_name();
        match &self.glibc_version {
            Some(v) => format!("{base}.{v}"),
            None => base.to_string(),
        }
    }
}

impl Serialize for JavaJvmHostTarget {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let base = self.target.canonical_name();
        match &self.glibc_version {
            Some(v) => serializer.serialize_str(&format!("{base}.{v}")),
            None => serializer.serialize_str(base),
        }
    }
}

impl<'de> Deserialize<'de> for JavaJvmHostTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;

        // Try to split a glibc version suffix: "linux-x86_64.2.17" → ("linux-x86_64", "2.17")
        // None of the JavaHostTarget canonical names contain a '.', so the first '.' in the
        // string marks the boundary between the target name and the glibc version.
        if let Some((base, glibc)) = split_glibc_version_suffix(&s) {
            let target = JavaHostTarget::from_canonical(base).ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "unknown host target '{base}' in '{s}'; \
                     expected one of: current, darwin-arm64, darwin-x86_64, \
                     linux-x86_64, linux-aarch64, windows-x86_64"
                ))
            })?;
            if !matches!(
                target,
                JavaHostTarget::LinuxX86_64 | JavaHostTarget::LinuxAarch64
            ) {
                return Err(serde::de::Error::custom(format!(
                    "glibc version suffix '.{glibc}' is only valid for Linux targets, \
                     but '{base}' is not a Linux target"
                )));
            }
            return Ok(Self {
                target,
                glibc_version: Some(glibc.to_string()),
            });
        }

        let target = JavaHostTarget::from_canonical(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown host target '{s}'; \
                 expected one of: current, darwin-arm64, darwin-x86_64, \
                 linux-x86_64, linux-aarch64, windows-x86_64, \
                 or a Linux target with a glibc version suffix (e.g. linux-x86_64.2.17)"
            ))
        })?;

        Ok(Self {
            target,
            glibc_version: None,
        })
    }
}

/// Split `"linux-x86_64.2.17"` into `("linux-x86_64", "2.17")`.
///
/// Returns `None` if no valid glibc version suffix is found.
/// A valid suffix starts with `'.'` followed by digits and optional further `'.digits'` groups.
/// None of the `JavaHostTarget` canonical names contain `'.'`, so the first `'.'`
/// in the string is always the separator.
fn split_glibc_version_suffix(s: &str) -> Option<(&str, &str)> {
    let dot = s.find('.')?;
    let version = &s[dot + 1..];
    // Version must be non-empty, start with a digit, and contain only digits and dots.
    if version.is_empty()
        || !version.starts_with(|c: char| c.is_ascii_digit())
        || !version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return None;
    }
    Some((&s[..dot], version))
}

impl JavaHostTarget {
    /// Parse a host target from its canonical string name (same set accepted by serde).
    pub fn from_canonical(s: &str) -> Option<Self> {
        match s {
            "current" => Some(Self::Current),
            "darwin-arm64" | "darwin-aarch64" => Some(Self::DarwinArm64),
            "darwin-x86_64" | "darwin-x86-64" => Some(Self::DarwinX86_64),
            "linux-x86_64" | "linux-x86-64" => Some(Self::LinuxX86_64),
            "linux-aarch64" | "linux-arm64" => Some(Self::LinuxAarch64),
            "windows-x86_64" | "windows-x86-64" => Some(Self::WindowsX86_64),
            _ => None,
        }
    }

    fn native_host_platform(self) -> NativeHostPlatform {
        match self {
            Self::Current => unreachable!("resolved host target required"),
            Self::DarwinArm64 => NativeHostPlatform::DarwinArm64,
            Self::DarwinX86_64 => NativeHostPlatform::DarwinX86_64,
            Self::LinuxX86_64 => NativeHostPlatform::LinuxX86_64,
            Self::LinuxAarch64 => NativeHostPlatform::LinuxAarch64,
            Self::WindowsX86_64 => NativeHostPlatform::WindowsX86_64,
        }
    }
}

impl From<NativeHostPlatform> for JavaHostTarget {
    fn from(value: NativeHostPlatform) -> Self {
        match value {
            NativeHostPlatform::DarwinArm64 => Self::DarwinArm64,
            NativeHostPlatform::DarwinX86_64 => Self::DarwinX86_64,
            NativeHostPlatform::LinuxX86_64 => Self::LinuxX86_64,
            NativeHostPlatform::LinuxAarch64 => Self::LinuxAarch64,
            NativeHostPlatform::WindowsX86_64 => Self::WindowsX86_64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CSharpRuntimeIdentifier {
    #[serde(rename = "current")]
    Current,
    #[serde(rename = "osx-arm64", alias = "darwin-arm64", alias = "osx-aarch64")]
    OsxArm64,
    #[serde(rename = "osx-x64", alias = "darwin-x86_64", alias = "osx-x86_64")]
    OsxX64,
    #[serde(rename = "linux-x64", alias = "linux-x86_64")]
    LinuxX64,
    #[serde(rename = "linux-arm64", alias = "linux-aarch64")]
    LinuxArm64,
    #[serde(rename = "win-x64", alias = "windows-x86_64", alias = "win-x86_64")]
    WinX64,
}

impl CSharpRuntimeIdentifier {
    pub const DEFAULTS: &'static [Self] = &[Self::Current];
    pub const EXPLICIT_TARGETS: &'static [Self] = &[
        Self::OsxArm64,
        Self::OsxX64,
        Self::LinuxX64,
        Self::LinuxArm64,
        Self::WinX64,
    ];

    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::OsxArm64 => "osx-arm64",
            Self::OsxX64 => "osx-x64",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
            Self::WinX64 => "win-x64",
        }
    }

    pub fn current() -> Option<Self> {
        NativeHostPlatform::current().map(Into::into)
    }

    pub fn resolve_requested(targets: &[Self]) -> Result<Vec<Self>, String> {
        let current_host = Self::current().ok_or_else(Self::unsupported_host_message)?;
        let mut resolved = Vec::new();

        targets.iter().copied().for_each(|target| {
            let target = match target {
                Self::Current => current_host,
                explicit => explicit,
            };

            if !resolved.contains(&target) {
                resolved.push(target);
            }
        });

        Ok(resolved)
    }

    pub fn native_host_platform(self) -> NativeHostPlatform {
        match self {
            Self::Current => unreachable!("resolved C# runtime identifier required"),
            Self::OsxArm64 => NativeHostPlatform::DarwinArm64,
            Self::OsxX64 => NativeHostPlatform::DarwinX86_64,
            Self::LinuxX64 => NativeHostPlatform::LinuxX86_64,
            Self::LinuxArm64 => NativeHostPlatform::LinuxAarch64,
            Self::WinX64 => NativeHostPlatform::WindowsX86_64,
        }
    }

    pub fn native_library_filename(self, artifact_name: &str) -> String {
        self.native_host_platform()
            .shared_library_filename(artifact_name)
    }

    fn unsupported_host_message() -> String {
        "C# packaging is only supported on osx-arm64, osx-x64, linux-x64, linux-arm64, and win-x64 hosts".to_string()
    }
}

impl From<NativeHostPlatform> for CSharpRuntimeIdentifier {
    fn from(value: NativeHostPlatform) -> Self {
        match value {
            NativeHostPlatform::DarwinArm64 => Self::OsxArm64,
            NativeHostPlatform::DarwinX86_64 => Self::OsxX64,
            NativeHostPlatform::LinuxX86_64 => Self::LinuxX64,
            NativeHostPlatform::LinuxAarch64 => Self::LinuxArm64,
            NativeHostPlatform::WindowsX86_64 => Self::WinX64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RustTarget {
    triple: &'static str,
    platform: Platform,
    architecture: Architecture,
}

impl RustTarget {
    pub const IOS_ARM64: Self = Self {
        triple: "aarch64-apple-ios",
        platform: Platform::Ios,
        architecture: Architecture::Arm64,
    };

    pub const IOS_SIM_ARM64: Self = Self {
        triple: "aarch64-apple-ios-sim",
        platform: Platform::IosSimulator,
        architecture: Architecture::Arm64,
    };

    pub const IOS_SIM_X86_64: Self = Self {
        triple: "x86_64-apple-ios",
        platform: Platform::IosSimulator,
        architecture: Architecture::X86_64,
    };

    pub const MACOS_ARM64: Self = Self {
        triple: "aarch64-apple-darwin",
        platform: Platform::MacOs,
        architecture: Architecture::Arm64,
    };

    pub const MACOS_X86_64: Self = Self {
        triple: "x86_64-apple-darwin",
        platform: Platform::MacOs,
        architecture: Architecture::X86_64,
    };

    pub const ANDROID_ARM64: Self = Self {
        triple: "aarch64-linux-android",
        platform: Platform::Android,
        architecture: Architecture::Arm64,
    };

    pub const ANDROID_ARMV7: Self = Self {
        triple: "armv7-linux-androideabi",
        platform: Platform::Android,
        architecture: Architecture::Armv7,
    };

    pub const ANDROID_X86_64: Self = Self {
        triple: "x86_64-linux-android",
        platform: Platform::Android,
        architecture: Architecture::X86_64,
    };

    pub const ANDROID_X86: Self = Self {
        triple: "i686-linux-android",
        platform: Platform::Android,
        architecture: Architecture::X86,
    };

    pub const WASM32_UNKNOWN_UNKNOWN: Self = Self {
        triple: "wasm32-unknown-unknown",
        platform: Platform::Wasm,
        architecture: Architecture::Wasm32,
    };

    pub const LINUX_X86_64: Self = Self {
        triple: "x86_64-unknown-linux-gnu",
        platform: Platform::Linux,
        architecture: Architecture::X86_64,
    };

    pub const LINUX_ARM64: Self = Self {
        triple: "aarch64-unknown-linux-gnu",
        platform: Platform::Linux,
        architecture: Architecture::Arm64,
    };

    pub const ALL_IOS: &'static [Self] =
        &[Self::IOS_ARM64, Self::IOS_SIM_ARM64, Self::IOS_SIM_X86_64];

    pub const ALL_MACOS: &'static [Self] = &[Self::MACOS_ARM64, Self::MACOS_X86_64];

    pub const ALL_ANDROID: &'static [Self] = &[
        Self::ANDROID_ARM64,
        Self::ANDROID_ARMV7,
        Self::ANDROID_X86_64,
        Self::ANDROID_X86,
    ];

    pub const fn from_android_architecture(architecture: AndroidArchitecture) -> Self {
        architecture.rust_target()
    }

    pub fn triple(&self) -> &'static str {
        self.triple
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn architecture(&self) -> Architecture {
        self.architecture
    }

    pub fn library_path_for_profile(
        &self,
        target_dir: &Path,
        lib_name: &str,
        profile_directory_name: &str,
    ) -> PathBuf {
        let artifact_name = match self.platform {
            Platform::Wasm => format!("{}.wasm", lib_name),
            Platform::Ios | Platform::IosSimulator | Platform::MacOs => {
                format!("lib{}.a", lib_name)
            }
            // Android packages a JNI-facing shared object by linking the Rust static archive
            // into the generated JNI glue. Using the Rust cdylib here leaves a DT_NEEDED
            // entry on the build-machine path, which breaks on-device loading.
            Platform::Android => format!("lib{}.a", lib_name),
            Platform::Linux => format!("lib{}.so", lib_name),
        };

        target_dir
            .join(self.triple)
            .join(profile_directory_name)
            .join(artifact_name)
    }
}

pub fn resolve_android_targets(architectures: &[AndroidArchitecture]) -> Vec<RustTarget> {
    architectures
        .iter()
        .copied()
        .map(RustTarget::from_android_architecture)
        .collect()
}

pub fn resolve_apple_ios_targets(architectures: &[AppleIosArchitecture]) -> Vec<RustTarget> {
    architectures
        .iter()
        .copied()
        .map(AppleIosArchitecture::rust_target)
        .collect()
}

pub fn resolve_apple_simulator_targets(architectures: &[AppleArchitecture]) -> Vec<RustTarget> {
    architectures
        .iter()
        .copied()
        .map(AppleArchitecture::simulator_rust_target)
        .collect()
}

pub fn resolve_apple_macos_targets(architectures: &[AppleArchitecture]) -> Vec<RustTarget> {
    architectures
        .iter()
        .copied()
        .map(AppleArchitecture::macos_rust_target)
        .collect()
}

pub fn resolve_java_host_targets(
    targets: &[JavaJvmHostTarget],
) -> Result<Vec<JavaJvmHostTarget>, String> {
    JavaJvmHostTarget::resolve_requested(targets)
}

pub fn resolve_dart_native_targets(architectures: &[DartNativeArchitecture]) -> Vec<RustTarget> {
    architectures
        .iter()
        .copied()
        .map(DartNativeArchitecture::rust_target)
        .collect()
}

impl Platform {
    pub fn is_apple(&self) -> bool {
        matches!(
            self,
            Platform::Ios | Platform::IosSimulator | Platform::MacOs
        )
    }
}

impl Architecture {
    pub fn android_abi(&self) -> &'static str {
        match self {
            Architecture::Arm64 => "arm64-v8a",
            Architecture::Armv7 => "armeabi-v7a",
            Architecture::X86_64 => "x86_64",
            Architecture::X86 => "x86",
            Architecture::Wasm32 => unreachable!("wasm targets do not map to android abi"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuiltLibrary {
    pub target: RustTarget,
    pub path: PathBuf,
}

impl BuiltLibrary {
    pub fn discover_for_targets(
        target_dir: &Path,
        lib_name: &str,
        profile_directory_name: &str,
        targets: &[RustTarget],
    ) -> Vec<Self> {
        targets
            .iter()
            .filter_map(|target| {
                let path =
                    target.library_path_for_profile(target_dir, lib_name, profile_directory_name);
                path.exists().then_some(BuiltLibrary {
                    target: *target,
                    path,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::target::{DartNativeArchitecture, resolve_dart_native_targets};

    use super::{
        AndroidArchitecture, AppleArchitecture, AppleIosArchitecture, BuiltLibrary, JavaHostTarget,
        JavaJvmHostTarget, Platform, RustTarget, resolve_android_targets,
        resolve_apple_ios_targets, resolve_apple_macos_targets, resolve_apple_simulator_targets,
        resolve_java_host_targets,
    };
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn apple_targets_use_static_libraries() {
        let library_path =
            RustTarget::IOS_ARM64.library_path_for_profile(Path::new("target"), "demo", "debug");

        assert_eq!(RustTarget::IOS_ARM64.platform(), Platform::Ios);
        assert!(library_path.ends_with("target/aarch64-apple-ios/debug/libdemo.a"));
    }

    #[test]
    fn android_targets_use_static_libraries_for_packaging() {
        let library_path = RustTarget::ANDROID_ARM64.library_path_for_profile(
            Path::new("target"),
            "demo",
            "debug",
        );

        assert_eq!(RustTarget::ANDROID_ARM64.platform(), Platform::Android);
        assert!(library_path.ends_with("target/aarch64-linux-android/debug/libdemo.a"));
    }

    #[test]
    fn resolves_android_architectures_to_targets() {
        let targets = resolve_android_targets(&[
            AndroidArchitecture::Arm64,
            AndroidArchitecture::Armv7,
            AndroidArchitecture::X86_64,
        ]);

        assert_eq!(
            targets
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec![
                "aarch64-linux-android",
                "armv7-linux-androideabi",
                "x86_64-linux-android",
            ]
        );
    }

    #[test]
    fn resolves_apple_ios_architectures_to_targets() {
        let targets = resolve_apple_ios_targets(&[AppleIosArchitecture::Arm64]);

        assert_eq!(
            targets
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec!["aarch64-apple-ios"]
        );
    }

    #[test]
    fn resolves_apple_simulator_architectures_to_targets() {
        let targets =
            resolve_apple_simulator_targets(&[AppleArchitecture::Arm64, AppleArchitecture::X86_64]);

        assert_eq!(
            targets
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec!["aarch64-apple-ios-sim", "x86_64-apple-ios"]
        );
    }

    #[test]
    fn resolves_apple_macos_architectures_to_targets() {
        let targets =
            resolve_apple_macos_targets(&[AppleArchitecture::Arm64, AppleArchitecture::X86_64]);

        assert_eq!(
            targets
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec!["aarch64-apple-darwin", "x86_64-apple-darwin"]
        );
    }

    #[test]
    fn resolves_current_java_host_target() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let resolved =
            resolve_java_host_targets(&[JavaJvmHostTarget::new(JavaHostTarget::Current)])
                .expect("expected current host resolution");

        assert_eq!(resolved, vec![JavaJvmHostTarget::new(current_host)]);
    }

    #[test]
    fn dedupes_current_against_explicit_java_host_target() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let resolved = resolve_java_host_targets(&[
            JavaJvmHostTarget::new(JavaHostTarget::Current),
            JavaJvmHostTarget::new(current_host),
        ])
        .expect("expected deduped host targets");

        assert_eq!(resolved, vec![JavaJvmHostTarget::new(current_host)]);
    }

    #[test]
    fn allows_explicit_cross_host_java_targets_after_resolution() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let explicit_other_host = [
            JavaHostTarget::DarwinArm64,
            JavaHostTarget::DarwinX86_64,
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::LinuxAarch64,
            JavaHostTarget::WindowsX86_64,
        ]
        .into_iter()
        .find(|target| *target != current_host)
        .expect("alternate host target");

        let resolved = resolve_java_host_targets(&[
            JavaJvmHostTarget::new(JavaHostTarget::Current),
            JavaJvmHostTarget::new(explicit_other_host),
        ])
        .expect("resolved host targets");

        assert_eq!(
            resolved,
            vec![
                JavaJvmHostTarget::new(current_host),
                JavaJvmHostTarget::new(explicit_other_host)
            ]
        );
    }

    #[test]
    fn discovers_only_requested_targets() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-target-test-{unique}"));
        let arm64_path =
            RustTarget::ANDROID_ARM64.library_path_for_profile(&temp_root, "demo", "debug");
        let x86_path =
            RustTarget::ANDROID_X86.library_path_for_profile(&temp_root, "demo", "debug");

        fs::create_dir_all(arm64_path.parent().expect("arm64 parent")).expect("create arm64 dir");
        fs::create_dir_all(x86_path.parent().expect("x86 parent")).expect("create x86 dir");
        fs::write(&arm64_path, []).expect("write arm64 artifact");
        fs::write(&x86_path, []).expect("write x86 artifact");

        let discovered = BuiltLibrary::discover_for_targets(
            &temp_root,
            "demo",
            "debug",
            &[RustTarget::ANDROID_ARM64],
        );

        assert_eq!(discovered.len(), 1);
        assert_eq!(
            discovered[0].target.triple(),
            RustTarget::ANDROID_ARM64.triple()
        );

        fs::remove_dir_all(&temp_root).expect("cleanup temp target dir");
    }

    #[test]
    fn resolves_dart_native_architectures_to_targets() {
        let targets = resolve_dart_native_targets(&[
            DartNativeArchitecture::AndroidArm64,
            DartNativeArchitecture::AndroidArmv7,
            DartNativeArchitecture::AndroidX86_64,
            DartNativeArchitecture::IosArm64,
            DartNativeArchitecture::IosSimArm64,
            DartNativeArchitecture::IosSimX86_64,
            DartNativeArchitecture::LinuxArm64,
            DartNativeArchitecture::LinuxX86_64,
            DartNativeArchitecture::MacosArm64,
            DartNativeArchitecture::MacosX86_64,
        ]);

        assert_eq!(
            targets
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>(),
            vec![
                "aarch64-linux-android",
                "armv7-linux-androideabi",
                "x86_64-linux-android",
                "aarch64-apple-ios",
                "aarch64-apple-ios-sim",
                "x86_64-apple-ios",
                "aarch64-unknown-linux-gnu",
                "x86_64-unknown-linux-gnu",
                "aarch64-apple-darwin",
                "x86_64-apple-darwin"
            ]
        );
    }
}
