use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::{CliError, Result};
use crate::target::{Architecture, RustTarget};

#[derive(Debug, thiserror::Error)]
pub enum AndroidToolchainError {
    #[error("android ndk not found (set ANDROID_NDK_HOME or ANDROID_HOME/ANDROID_SDK_ROOT)")]
    NdkNotFound,

    #[error("invalid android ndk at {path}")]
    NdkInvalid { path: PathBuf },

    #[error("android ndk toolchain not found at {path}")]
    ToolchainNotFound { path: PathBuf },
}

#[derive(Debug, Clone)]
pub struct AndroidNdk {
    root: PathBuf,
    bin_dir: PathBuf,
    sysroot_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AndroidAbi {
    Arm64V8a,
    ArmeabiV7a,
    X86_64,
    X86,
}

#[derive(Debug, Clone)]
pub struct AndroidToolchain {
    ndk: AndroidNdk,
    min_sdk: u32,
}

impl AndroidToolchain {
    pub fn discover(min_sdk: u32, ndk_version_hint: Option<&str>) -> Result<Self> {
        Ok(Self {
            ndk: AndroidNdk::discover(ndk_version_hint)?,
            min_sdk,
        })
    }

    pub fn configure_cargo_for_target(
        &self,
        cargo: &mut Command,
        target: &RustTarget,
    ) -> Result<()> {
        cargo.envs(self.cargo_environment(target)?);
        Ok(())
    }

    pub fn cargo_environment(&self, target: &RustTarget) -> Result<Vec<(OsString, OsString)>> {
        let abi = AndroidAbi::from_architecture(target.architecture()).ok_or_else(|| {
            CliError::CommandFailed {
                command: format!("unsupported android target {}", target.triple()),
                status: None,
            }
        })?;

        let linker = self.ndk.clang_for_abi(abi, self.min_sdk);
        let ar = self.ndk.llvm_ar();
        let triple_env_upper = cargo_env_triple_upper(target.triple());
        let triple_env_lower = cargo_env_triple_lower(target.triple());

        let sysroot = self.ndk.sysroot();
        let include_target = android_sysroot_target(target.triple());

        let bindgen_key = format!("BINDGEN_EXTRA_CLANG_ARGS_{}", triple_env_lower);
        let bindgen_args = format!(
            "--sysroot={} -isystem {}/usr/include -isystem {}/usr/include/{}",
            sysroot.display(),
            sysroot.display(),
            sysroot.display(),
            include_target,
        );
        let bindgen_clang_args = bindgen_args.replace('\\', "/");

        let merged_args = match std::env::var(&bindgen_key) {
            Ok(existing) if !existing.trim().is_empty() => {
                format!("{bindgen_clang_args} {existing}")
            }
            _ => bindgen_clang_args,
        };

        Ok(vec![
            (
                format!("CARGO_TARGET_{}_LINKER", triple_env_upper).into(),
                linker.clone().into_os_string(),
            ),
            (
                format!("CARGO_TARGET_{}_AR", triple_env_upper).into(),
                ar.clone().into_os_string(),
            ),
            (
                format!("CC_{}", triple_env_lower).into(),
                linker.into_os_string(),
            ),
            (
                format!("AR_{}", triple_env_lower).into(),
                ar.into_os_string(),
            ),
            (bindgen_key.into(), merged_args.into()),
        ])
    }

    pub fn clang_for_target(&self, target: &RustTarget) -> Result<PathBuf> {
        let abi = AndroidAbi::from_architecture(target.architecture()).ok_or_else(|| {
            CliError::CommandFailed {
                command: format!("unsupported android target {}", target.triple()),
                status: None,
            }
        })?;
        Ok(self.ndk.clang_for_abi(abi, self.min_sdk))
    }

    #[allow(dead_code)]
    pub fn llvm_ar(&self) -> PathBuf {
        self.ndk.llvm_ar()
    }
}

impl AndroidNdk {
    pub fn discover(ndk_version_hint: Option<&str>) -> Result<Self> {
        let root = resolve_ndk_root(ndk_version_hint).ok_or(AndroidToolchainError::NdkNotFound)?;
        let prebuilt_dir = resolve_prebuilt_dir(&root)?;
        let bin_dir = prebuilt_dir.join("bin");
        let sysroot_dir = prebuilt_dir.join("sysroot");

        Ok(Self {
            root,
            sysroot_dir,
            bin_dir,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn sysroot(&self) -> &Path {
        &self.sysroot_dir
    }

    pub fn clang_for_abi(&self, abi: AndroidAbi, min_sdk: u32) -> PathBuf {
        let name = format!("{}{}-clang", abi.clang_prefix(), min_sdk);
        self.bin_dir.join(with_platform_ext(&name, "cmd"))
    }

    pub fn llvm_ar(&self) -> PathBuf {
        self.bin_dir.join(with_platform_ext("llvm-ar", "exe"))
    }
}

impl AndroidAbi {
    #[allow(dead_code)]
    pub fn directory_name(&self) -> &'static str {
        match self {
            AndroidAbi::Arm64V8a => "arm64-v8a",
            AndroidAbi::ArmeabiV7a => "armeabi-v7a",
            AndroidAbi::X86_64 => "x86_64",
            AndroidAbi::X86 => "x86",
        }
    }

    fn clang_prefix(&self) -> &'static str {
        match self {
            AndroidAbi::Arm64V8a => "aarch64-linux-android",
            AndroidAbi::ArmeabiV7a => "armv7a-linux-androideabi",
            AndroidAbi::X86_64 => "x86_64-linux-android",
            AndroidAbi::X86 => "i686-linux-android",
        }
    }

    fn from_architecture(architecture: Architecture) -> Option<Self> {
        match architecture {
            Architecture::Arm64 => Some(AndroidAbi::Arm64V8a),
            Architecture::Armv7 => Some(AndroidAbi::ArmeabiV7a),
            Architecture::X86_64 => Some(AndroidAbi::X86_64),
            Architecture::X86 => Some(AndroidAbi::X86),
            Architecture::Wasm32 => None,
        }
    }
}

fn cargo_env_triple_upper(triple: &str) -> String {
    triple.replace('-', "_").to_uppercase()
}

fn cargo_env_triple_lower(triple: &str) -> String {
    triple.replace('-', "_").to_lowercase()
}

fn with_platform_ext(name: &str, windows_ext: &str) -> String {
    if cfg!(windows) {
        format!("{}.{}", name, windows_ext)
    } else {
        name.to_string()
    }
}

fn resolve_ndk_root(ndk_version_hint: Option<&str>) -> Option<PathBuf> {
    resolve_ndk_root_from_env_var("ANDROID_NDK_HOME", ndk_version_hint)
        .or_else(|| resolve_ndk_root_from_android_home(ndk_version_hint))
}

fn resolve_ndk_root_from_env_var(env_var: &str, ndk_version_hint: Option<&str>) -> Option<PathBuf> {
    std::env::var_os(env_var)
        .map(PathBuf::from)
        .and_then(|path| resolve_ndk_root_from_candidate(&path, ndk_version_hint))
}

fn resolve_ndk_root_from_android_home(ndk_version_hint: Option<&str>) -> Option<PathBuf> {
    let android_home = std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)?;

    let ndk_bundle = android_home.join("ndk-bundle");
    resolve_ndk_root_from_candidate(&ndk_bundle, ndk_version_hint).or_else(|| {
        let ndk_dir = android_home.join("ndk");
        resolve_ndk_root_from_candidate(&ndk_dir, ndk_version_hint)
    })
}

fn resolve_ndk_root_from_candidate(
    candidate: &Path,
    ndk_version_hint: Option<&str>,
) -> Option<PathBuf> {
    is_ndk_root(candidate)
        .then(|| candidate.to_path_buf())
        .or_else(|| select_ndk_version_dir(candidate, ndk_version_hint))
}

fn is_ndk_root(path: &Path) -> bool {
    path.join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .exists()
}

fn select_ndk_version_dir(parent: &Path, ndk_version_hint: Option<&str>) -> Option<PathBuf> {
    let entries = std::fs::read_dir(parent).ok()?;

    let versioned_dirs: Vec<(PathBuf, NdkVersion, String)> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .filter(|path| is_ndk_root(path))
        .filter_map(|path| {
            let dir_name = path
                .file_name()
                .and_then(OsStr::to_str)
                .map(|name| name.to_string())?;
            Some((path, NdkVersion::parse(&dir_name)?, dir_name))
        })
        .collect();

    ndk_version_hint
        .and_then(|hint| {
            NdkVersion::parse(hint).map(|hint_version| (hint_version, hint.to_string()))
        })
        .and_then(|(hint_version, hint_string)| {
            versioned_dirs
                .iter()
                .find(|(_, version, _)| *version == hint_version)
                .map(|(path, _, _)| path.clone())
                .or_else(|| {
                    versioned_dirs
                        .iter()
                        .find(|(_, _, name)| name == &hint_string)
                        .map(|(path, _, _)| path.clone())
                })
        })
        .or_else(|| {
            versioned_dirs
                .into_iter()
                .max_by_key(|(_, version, _)| version.clone())
                .map(|(path, _, _)| path)
        })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NdkVersion {
    parts: Vec<u32>,
}

impl NdkVersion {
    fn parse(input: &str) -> Option<Self> {
        let parts: Vec<u32> = input
            .split('.')
            .filter_map(|part| part.parse::<u32>().ok())
            .collect();
        (!parts.is_empty()).then_some(Self { parts })
    }
}

fn resolve_prebuilt_dir(ndk_root: &Path) -> Result<PathBuf> {
    let prebuilt_dir = ndk_root.join("toolchains").join("llvm").join("prebuilt");
    if !prebuilt_dir.exists() {
        return Err(AndroidToolchainError::NdkInvalid {
            path: ndk_root.to_path_buf(),
        }
        .into());
    }

    let preferred = preferred_prebuilt_tags();
    let matching = preferred
        .iter()
        .map(|tag| prebuilt_dir.join(tag))
        .find(|path| path.join("bin").exists())
        .or_else(|| {
            std::fs::read_dir(&prebuilt_dir)
                .ok()
                .into_iter()
                .flat_map(|entries| {
                    entries.filter_map(|entry| entry.ok().map(|entry| entry.path()))
                })
                .find(|path| path.join("bin").exists())
        })
        .ok_or_else(|| {
            crate::cli::CliError::from(AndroidToolchainError::ToolchainNotFound {
                path: ndk_root.to_path_buf(),
            })
        })?;

    Ok(matching)
}

fn preferred_prebuilt_tags() -> Vec<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => vec!["darwin-arm64", "darwin-x86_64"],
        ("macos", "x86_64") => vec!["darwin-x86_64"],
        ("linux", "x86_64") => vec!["linux-x86_64"],
        ("linux", "aarch64") => vec!["linux-aarch64", "linux-x86_64"],
        ("windows", "x86_64") => vec!["windows-x86_64"],
        _ => Vec::new(),
    }
}

fn android_sysroot_target(triple: &str) -> &str {
    match triple {
        "armv7-linux-androideabi" => "arm-linux-androideabi",
        _ => triple,
    }
}
