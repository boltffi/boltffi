use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cargo::config::{
    cargo_config_file_candidates, configured_build_target as cargo_configured_build_target,
    extract_cargo_config_args, resolve_cargo_config_path,
};
use crate::cli::{CliError, Result};
use crate::target::{JavaHostTarget, NativeHostPlatform};

#[cfg(test)]
use crate::cargo::config::{
    cargo_config_file_candidates_with_inputs, cargo_config_search_roots,
    parse_build_target_from_config_file, parse_build_target_from_inline_config,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfiguredValue {
    source: String,
    value: String,
}

#[derive(Debug, Default)]
struct CargoTargetCfg {
    flags: HashSet<String>,
    values: HashMap<String, HashSet<String>>,
}

impl CargoTargetCfg {
    fn for_target(rust_target_triple: &str) -> Option<Self> {
        let output = Command::new("rustc")
            .args(["--print", "cfg", "--target", rust_target_triple])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        Some(Self::from_rustc_output(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    fn from_rustc_output(output: &str) -> Self {
        let mut target_cfg = Self::default();

        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if let Some((name, value)) = line.split_once('=') {
                target_cfg
                    .values
                    .entry(name.trim().to_string())
                    .or_default()
                    .insert(trim_wrapping_quotes(value.trim()).to_string());
            } else {
                target_cfg.flags.insert(line.to_string());
            }
        }

        target_cfg
    }

    fn matches_name(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    fn matches_key_value(&self, key: &str, value: &str) -> bool {
        self.values
            .get(key)
            .is_some_and(|values| values.contains(value))
    }
}

#[derive(Debug, Clone)]
pub struct NativeHostToolchain {
    rust_target_triple: String,
    cargo_linker_env: Option<(String, String)>,
    jni_compiler_program: PathBuf,
    jni_compiler_args: Vec<String>,
    jni_rustflag_linker_args: Vec<String>,
}

impl NativeHostToolchain {
    pub fn discover(
        toolchain_selector: Option<&str>,
        cargo_args: &[String],
        target: JavaHostTarget,
        current_host: JavaHostTarget,
    ) -> Result<Self> {
        Self::discover_for_platform(toolchain_selector, cargo_args, target, current_host, "JVM")
    }

    pub fn discover_csharp(
        toolchain_selector: Option<&str>,
        cargo_args: &[String],
        target: NativeHostPlatform,
        current_host: NativeHostPlatform,
    ) -> Result<Self> {
        Self::discover_for_platform(
            toolchain_selector,
            cargo_args,
            target.into(),
            current_host.into(),
            "C#",
        )
    }

    fn discover_for_platform(
        toolchain_selector: Option<&str>,
        cargo_args: &[String],
        target: JavaHostTarget,
        current_host: JavaHostTarget,
        platform_name: &str,
    ) -> Result<Self> {
        ensure_supported_native_host_pair(current_host, target, platform_name)?;
        let rust_target_triple =
            resolve_rust_target_triple(target, current_host, toolchain_selector, cargo_args)?;
        ensure_rust_target_installed(
            toolchain_selector,
            &rust_target_triple,
            target,
            current_host,
        )?;
        let rustflag_linker_args =
            configured_target_rustflag_linker_args(cargo_args, &rust_target_triple);

        match (current_host, target) {
            (
                JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64,
                JavaHostTarget::DarwinArm64,
            )
            | (
                JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64,
                JavaHostTarget::DarwinX86_64,
            ) => {
                let linker_program =
                    which::which("clang").map_err(|_| CliError::CommandFailed {
                        command: "clang not found in PATH for JVM desktop linking".to_string(),
                        status: None,
                    })?;
                let sdk_root = apple_sdk_root()?;
                let linker_args = vec![
                    "-target".to_string(),
                    rust_target_triple.clone(),
                    "-isysroot".to_string(),
                    sdk_root.display().to_string(),
                ];
                Ok(Self {
                    rust_target_triple: rust_target_triple.clone(),
                    cargo_linker_env: None,
                    jni_compiler_program: linker_program,
                    jni_compiler_args: linker_args,
                    jni_rustflag_linker_args: rustflag_linker_args,
                })
            }
            (
                JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64,
                JavaHostTarget::LinuxX86_64,
            ) => {
                let (cargo_linker_program, jni_compiler_program, jni_compiler_args) =
                    resolve_linux_cross_toolchain(cargo_args, &rust_target_triple)?;
                let cargo_linker_program =
                    if linux_cross_linker_args(&cargo_linker_program, &rust_target_triple)
                        .is_empty()
                    {
                        cargo_linker_program
                    } else {
                        write_linux_cross_linker_wrapper(
                            &cargo_linker_program,
                            &rust_target_triple,
                        )?
                    };
                Ok(Self {
                    rust_target_triple: rust_target_triple.clone(),
                    cargo_linker_env: Some((
                        cargo_linker_env_key(&rust_target_triple),
                        cargo_linker_program.display().to_string(),
                    )),
                    jni_compiler_program,
                    jni_compiler_args,
                    jni_rustflag_linker_args: rustflag_linker_args,
                })
            }
            (JavaHostTarget::LinuxX86_64, JavaHostTarget::LinuxX86_64)
            | (JavaHostTarget::LinuxAarch64, JavaHostTarget::LinuxAarch64) => {
                let (linker_program, linker_args) =
                    resolve_linux_host_linker(toolchain_selector, cargo_args, &rust_target_triple)?;
                Ok(Self {
                    rust_target_triple,
                    cargo_linker_env: None,
                    jni_compiler_program: linker_program,
                    jni_compiler_args: linker_args,
                    jni_rustflag_linker_args: rustflag_linker_args,
                })
            }
            (JavaHostTarget::WindowsX86_64, JavaHostTarget::WindowsX86_64) => {
                let (linker_program, linker_args) = resolve_windows_host_linker(
                    toolchain_selector,
                    cargo_args,
                    &rust_target_triple,
                )?;
                Ok(Self {
                    rust_target_triple,
                    cargo_linker_env: None,
                    jni_compiler_program: linker_program,
                    jni_compiler_args: linker_args,
                    jni_rustflag_linker_args: rustflag_linker_args,
                })
            }
            _ => unreachable!("unsupported host/target pairs should fail before toolchain probing"),
        }
    }

    pub fn rust_target_triple(&self) -> &str {
        &self.rust_target_triple
    }
    pub fn configure_cargo_build(&self, command: &mut Command) {
        if let Some((key, value)) = self.cargo_linker_env.as_ref() {
            command.env(key, value);
        }
    }

    pub fn linker_command(&self) -> Command {
        let mut command = Command::new(&self.jni_compiler_program);
        command.args(&self.jni_compiler_args);
        command
    }

    pub fn jni_compiler_program(&self) -> &Path {
        &self.jni_compiler_program
    }

    pub fn uses_msvc_compiler(&self) -> bool {
        linker_program_name(&self.jni_compiler_program).is_some_and(|name| {
            name.eq_ignore_ascii_case("clang-cl") || name.eq_ignore_ascii_case("cl")
        })
    }

    pub fn jni_rustflag_linker_args(&self) -> &[String] {
        &self.jni_rustflag_linker_args
    }
}

fn ensure_supported_native_host_pair(
    current_host: JavaHostTarget,
    target: JavaHostTarget,
    platform_name: &str,
) -> Result<()> {
    let supported = matches!(
        (current_host, target),
        (
            JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64,
            JavaHostTarget::DarwinArm64,
        ) | (
            JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64,
            JavaHostTarget::DarwinX86_64,
        ) | (
            JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64,
            JavaHostTarget::LinuxX86_64,
        ) | (JavaHostTarget::LinuxX86_64, JavaHostTarget::LinuxX86_64)
            | (JavaHostTarget::LinuxAarch64, JavaHostTarget::LinuxAarch64)
            | (JavaHostTarget::WindowsX86_64, JavaHostTarget::WindowsX86_64)
    );

    if supported {
        return Ok(());
    }

    Err(CliError::CommandFailed {
        command: format!(
            "{platform_name} host target '{}' is not supported from current host '{}' in Phase 4",
            target.canonical_name(),
            current_host.canonical_name()
        ),
        status: None,
    })
}

fn resolve_rust_target_triple(
    target: JavaHostTarget,
    current_host: JavaHostTarget,
    toolchain_selector: Option<&str>,
    cargo_args: &[String],
) -> Result<String> {
    match target {
        JavaHostTarget::Current => unreachable!("resolved host target required"),
        JavaHostTarget::DarwinArm64 => Ok("aarch64-apple-darwin".to_string()),
        JavaHostTarget::DarwinX86_64 => Ok("x86_64-apple-darwin".to_string()),
        JavaHostTarget::LinuxX86_64 | JavaHostTarget::LinuxAarch64 => {
            resolve_linux_rust_target_triple(target, current_host, toolchain_selector, cargo_args)
        }
        JavaHostTarget::WindowsX86_64 => {
            resolve_windows_rust_target_triple(target, current_host, toolchain_selector, cargo_args)
        }
    }
}

fn resolve_linux_rust_target_triple(
    target: JavaHostTarget,
    current_host: JavaHostTarget,
    toolchain_selector: Option<&str>,
    cargo_args: &[String],
) -> Result<String> {
    if current_host != target {
        if let Some(target_triple) = configured_linux_build_target(cargo_args, target)? {
            return Ok(target_triple);
        }

        return Ok(default_linux_rust_target_triple(target).to_string());
    }

    if let Some(target_triple) = current_host_linux_build_target(cargo_args, target) {
        return Ok(target_triple);
    }

    let host_triple = rustc_host_triple(toolchain_selector)?;
    validate_linux_rust_target_triple(&host_triple, target)
}

fn default_linux_rust_target_triple(target: JavaHostTarget) -> &'static str {
    match target {
        JavaHostTarget::LinuxX86_64 => "x86_64-unknown-linux-gnu",
        JavaHostTarget::LinuxAarch64 => "aarch64-unknown-linux-gnu",
        _ => unreachable!("linux target required"),
    }
}

fn resolve_windows_rust_target_triple(
    target: JavaHostTarget,
    current_host: JavaHostTarget,
    toolchain_selector: Option<&str>,
    cargo_args: &[String],
) -> Result<String> {
    if current_host != target
        && let Some(target_triple) = configured_windows_build_target(cargo_args)?
    {
        return Ok(target_triple);
    }

    if let Some(target_triple) = current_host_windows_build_target(cargo_args) {
        return Ok(target_triple);
    }

    let host_triple = rustc_host_triple(toolchain_selector)?;
    validate_windows_rust_target_triple(&host_triple)
}

fn validate_windows_rust_target_triple(target_triple: &str) -> Result<String> {
    match target_triple {
        "x86_64-pc-windows-msvc" | "x86_64-pc-windows-gnu" | "x86_64-pc-windows-gnullvm" => {
            Ok(target_triple.to_string())
        }
        _ => Err(CliError::CommandFailed {
            command: format!(
                "Windows JVM target '{}' is not a supported windows-x86_64 target",
                target_triple
            ),
            status: None,
        }),
    }
}

fn configured_linux_build_target(
    cargo_args: &[String],
    target: JavaHostTarget,
) -> Result<Option<String>> {
    let current_directory = std::env::current_dir().ok();
    let Some(target_triple) =
        cargo_configured_build_target(cargo_args, current_directory.as_deref())
    else {
        return Ok(None);
    };

    if !target_triple.contains("linux") {
        return Ok(None);
    }

    validate_linux_rust_target_triple(&target_triple, target).map(Some)
}

fn current_host_linux_build_target(
    cargo_args: &[String],
    target: JavaHostTarget,
) -> Option<String> {
    let current_directory = std::env::current_dir().ok();
    let target_triple = cargo_configured_build_target(cargo_args, current_directory.as_deref())?;
    if !target_triple.contains("linux") || target_triple.contains("android") {
        return None;
    }

    validate_linux_rust_target_triple(&target_triple, target).ok()
}

fn validate_linux_rust_target_triple(
    target_triple: &str,
    target: JavaHostTarget,
) -> Result<String> {
    let is_supported = match target {
        JavaHostTarget::LinuxX86_64 => {
            target_triple.starts_with("x86_64-")
                && target_triple.contains("linux")
                && !target_triple.contains("android")
        }
        JavaHostTarget::LinuxAarch64 => {
            target_triple.starts_with("aarch64-")
                && target_triple.contains("linux")
                && !target_triple.contains("android")
        }
        _ => unreachable!("linux target required"),
    };

    if is_supported {
        return Ok(target_triple.to_string());
    }

    Err(CliError::CommandFailed {
        command: format!(
            "Linux JVM target '{}' is not a supported {} target",
            target_triple,
            target.canonical_name()
        ),
        status: None,
    })
}

fn configured_windows_build_target(cargo_args: &[String]) -> Result<Option<String>> {
    let current_directory = std::env::current_dir().ok();
    let Some(target_triple) =
        cargo_configured_build_target(cargo_args, current_directory.as_deref())
    else {
        return Ok(None);
    };

    if !target_triple.contains("windows") {
        return Ok(None);
    }

    validate_windows_rust_target_triple(&target_triple).map(Some)
}

fn current_host_windows_build_target(cargo_args: &[String]) -> Option<String> {
    let current_directory = std::env::current_dir().ok();
    let target_triple = cargo_configured_build_target(cargo_args, current_directory.as_deref())?;
    if !target_triple.contains("windows") {
        return None;
    }

    validate_windows_rust_target_triple(&target_triple).ok()
}

fn rustc_host_triple(toolchain_selector: Option<&str>) -> Result<String> {
    let mut command = Command::new("rustc");
    if let Some(toolchain_selector) = toolchain_selector {
        command.arg(toolchain_selector);
    }
    command.arg("-vV");

    let output = command.output().map_err(|source| CliError::CommandFailed {
        command: format!("rustc -vV: {source}"),
        status: None,
    })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: "rustc -vV".to_string(),
            status: output.status.code(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_rustc_host_triple(&stdout).ok_or_else(|| CliError::CommandFailed {
        command: "rustc -vV did not report a host triple".to_string(),
        status: None,
    })
}

fn parse_rustc_host_triple(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
}

fn ensure_rust_target_installed(
    toolchain_selector: Option<&str>,
    target_triple: &str,
    target: JavaHostTarget,
    current_host: JavaHostTarget,
) -> Result<()> {
    let Some(rustup_path) = which::which("rustup").ok() else {
        return fallback_without_rustup(target, current_host);
    };

    let mut command = Command::new(rustup_path);
    command.arg("target").arg("list").arg("--installed");
    if let Some(toolchain) = rustup_toolchain_name(toolchain_selector) {
        command.arg("--toolchain").arg(toolchain);
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(_) => return fallback_without_rustup(target, current_host),
    };

    if !output.status.success() {
        return fallback_without_rustup(target, current_host);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let installed = stdout.lines().map(str::trim).collect::<Vec<_>>();
    validate_installed_rustup_target(&installed, target_triple)
}

fn fallback_without_rustup(target: JavaHostTarget, current_host: JavaHostTarget) -> Result<()> {
    let _ = (target, current_host);
    // rustup is optional here. For the current host we should not block packaging,
    // and for cross-host targets Cargo will still fail with a clear missing-target
    // error if the requested target or linker is unavailable.
    Ok(())
}

fn validate_installed_rustup_target(installed: &[&str], target_triple: &str) -> Result<()> {
    if installed.contains(&target_triple) {
        return Ok(());
    }

    Err(CliError::CommandFailed {
        command: format!(
            "rustup target '{}' is not installed; run `rustup target add {}`",
            target_triple, target_triple
        ),
        status: None,
    })
}

fn rustup_toolchain_name(toolchain_selector: Option<&str>) -> Option<&str> {
    toolchain_selector.and_then(|selector| selector.strip_prefix('+'))
}

fn cargo_linker_env_key(target_triple: &str) -> String {
    format!(
        "CARGO_TARGET_{}_LINKER",
        target_triple.replace('-', "_").to_uppercase()
    )
}

fn resolve_linux_host_linker(
    toolchain_selector: Option<&str>,
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Result<(PathBuf, Vec<String>)> {
    if let Some(linker_program) = resolve_target_compiler_from_values(
        configured_target_linker_values(cargo_args, rust_target_triple),
        "Linux desktop",
    )? {
        let host_triple = rustc_host_triple(toolchain_selector)?;
        let linker_args = linux_host_linker_args(&linker_program, rust_target_triple, &host_triple);
        return Ok((linker_program, linker_args));
    }

    let linker_program = which::which("clang").map_err(|_| CliError::CommandFailed {
        command: "clang not found in PATH for JVM desktop linking".to_string(),
        status: None,
    })?;
    let host_triple = rustc_host_triple(toolchain_selector)?;
    let linker_args = linux_host_linker_args(&linker_program, rust_target_triple, &host_triple);
    Ok((linker_program, linker_args))
}

fn resolve_linux_cross_toolchain(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Result<(PathBuf, PathBuf, Vec<String>)> {
    let configured_values =
        configured_linux_x86_64_cross_linker_values(cargo_args, rust_target_triple);
    let cargo_linker_program =
        resolve_target_linker_from_values(configured_values.clone(), "Linux x86_64")?;
    let jni_compiler_program =
        resolve_target_compiler_from_values(configured_values, "Linux x86_64")?
            .or_else(|| discover_default_linux_x86_64_cross_compiler(rust_target_triple))
            .ok_or_else(|| missing_linux_x86_64_cross_linker_error(rust_target_triple))?;

    if let Some(cargo_linker_program) = cargo_linker_program.as_ref() {
        validate_linux_cross_linker_program(cargo_linker_program, rust_target_triple)?;
    }
    validate_linux_cross_linker_program(&jni_compiler_program, rust_target_triple)?;

    let cargo_linker_program = cargo_linker_program.unwrap_or_else(|| jni_compiler_program.clone());
    let jni_compiler_args = linux_cross_linker_args(&jni_compiler_program, rust_target_triple);
    Ok((
        cargo_linker_program,
        jni_compiler_program,
        jni_compiler_args,
    ))
}

fn resolve_windows_host_linker(
    toolchain_selector: Option<&str>,
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Result<(PathBuf, Vec<String>)> {
    let host_triple = rustc_host_triple(toolchain_selector)?;
    resolve_windows_host_linker_with_host_triple(host_triple, cargo_args, rust_target_triple)
}

fn resolve_windows_host_linker_with_host_triple(
    host_triple: String,
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Result<(PathBuf, Vec<String>)> {
    if let Some(linker_program) = resolve_target_compiler_from_values(
        configured_target_linker_values(cargo_args, rust_target_triple),
        "Windows desktop",
    )? {
        let linker_args =
            windows_host_linker_args(&linker_program, rust_target_triple, &host_triple);
        return Ok((linker_program, linker_args));
    }

    let linker_program =
        discover_default_windows_jni_compiler(rust_target_triple).ok_or_else(|| {
            CliError::CommandFailed {
                command:
                    "no supported Windows C compiler driver found in PATH for JVM desktop linking"
                        .to_string(),
                status: None,
            }
        })?;
    let linker_args = windows_host_linker_args(&linker_program, rust_target_triple, &host_triple);
    Ok((linker_program, linker_args))
}

fn discover_default_windows_jni_compiler(rust_target_triple: &str) -> Option<PathBuf> {
    default_windows_jni_compiler_candidates(rust_target_triple)
        .into_iter()
        .find_map(|candidate| which::which(candidate).ok())
}

fn default_windows_jni_compiler_candidates(rust_target_triple: &str) -> Vec<&'static str> {
    if rust_target_triple == "x86_64-pc-windows-msvc" {
        vec!["clang-cl", "cl", "clang"]
    } else {
        vec![
            "x86_64-w64-mingw32-gcc",
            "x86_64-w64-mingw32-clang",
            "gcc",
            "clang",
        ]
    }
}

fn discover_default_linux_x86_64_cross_compiler(rust_target_triple: &str) -> Option<PathBuf> {
    if rust_target_triple != default_linux_rust_target_triple(JavaHostTarget::LinuxX86_64) {
        return None;
    }

    ["x86_64-linux-gnu-clang", "x86_64-linux-gnu-gcc"]
        .into_iter()
        .find_map(|candidate| which::which(candidate).ok())
}

fn resolve_target_linker_from_values<I>(
    configured_values: I,
    target_label: &str,
) -> Result<Option<PathBuf>>
where
    I: IntoIterator<Item = ConfiguredValue>,
{
    if let Some(value) = configured_values.into_iter().next() {
        match resolve_target_compiler_value(&value.value) {
            Some(resolved) => return Ok(Some(resolved)),
            None => {
                return Err(CliError::CommandFailed {
                    command: format!(
                        "configured {target_label} linker from {} does not resolve to an executable: {}",
                        value.source, value.value
                    ),
                    status: None,
                });
            }
        }
    }

    Ok(None)
}

fn resolve_target_compiler_from_values<I>(
    configured_values: I,
    target_label: &str,
) -> Result<Option<PathBuf>>
where
    I: IntoIterator<Item = ConfiguredValue>,
{
    for value in configured_values {
        match resolve_target_compiler_value(&value.value) {
            Some(resolved) => {
                if !is_linker_only_tool(&resolved) {
                    return Ok(Some(resolved));
                }
            }
            None => {
                return Err(CliError::CommandFailed {
                    command: format!(
                        "configured {target_label} linker from {} does not resolve to an executable: {}",
                        value.source, value.value
                    ),
                    status: None,
                });
            }
        }
    }

    Ok(None)
}

fn configured_target_linker_values(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Vec<ConfiguredValue> {
    let cargo_env_key = cargo_linker_env_key(rust_target_triple);
    cargo_inline_configured_linker_values(cargo_args, rust_target_triple)
        .into_iter()
        .chain(
            std::env::var(&cargo_env_key)
                .ok()
                .into_iter()
                .map(|value| ConfiguredValue {
                    source: cargo_env_key.clone(),
                    value,
                }),
        )
        .chain(cargo_config_file_linker_values(
            cargo_args,
            rust_target_triple,
        ))
        .collect()
}

fn configured_target_rustflag_linker_args(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Vec<String> {
    let cargo_encoded_rustflags = std::env::var("CARGO_ENCODED_RUSTFLAGS").ok();
    let rustflags_env = std::env::var("RUSTFLAGS").ok();
    let cargo_env_key = cargo_rustflags_env_key(rust_target_triple);
    let cargo_target_rustflags = std::env::var(&cargo_env_key).ok();
    configured_target_rustflag_linker_args_with_sources(
        cargo_args,
        rust_target_triple,
        cargo_encoded_rustflags,
        rustflags_env,
        cargo_target_rustflags,
    )
}

fn configured_target_rustflag_linker_args_with_sources(
    cargo_args: &[String],
    rust_target_triple: &str,
    cargo_encoded_rustflags: Option<String>,
    rustflags_env: Option<String>,
    cargo_target_rustflags: Option<String>,
) -> Vec<String> {
    let mut rustflags = if let Some(encoded_rustflags) = cargo_encoded_rustflags {
        split_encoded_rustflags(&encoded_rustflags)
    } else if let Some(rustflags_env) = rustflags_env {
        split_shell_words(&rustflags_env)
    } else {
        Vec::new()
    };
    rustflags.extend(cargo_inline_configured_rustflags(
        cargo_args,
        rust_target_triple,
    ));
    rustflags.extend(
        cargo_target_rustflags
            .into_iter()
            .flat_map(|value| split_shell_words(&value)),
    );
    rustflags.extend(cargo_config_file_rustflags(cargo_args, rust_target_triple));

    rustflags_to_linker_args(&rustflags)
}

fn cargo_rustflags_env_key(target_triple: &str) -> String {
    format!(
        "CARGO_TARGET_{}_RUSTFLAGS",
        target_triple.replace('-', "_").to_uppercase()
    )
}

fn split_encoded_rustflags(input: &str) -> Vec<String> {
    input
        .split('\u{1f}')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn linux_cross_linker_args(linker_program: &Path, rust_target_triple: &str) -> Vec<String> {
    linker_program
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| is_clang_driver_name(name))
        .map(|_| vec!["--target".to_string(), rust_target_triple.to_string()])
        .unwrap_or_default()
}

fn linux_host_linker_args(
    linker_program: &Path,
    rust_target_triple: &str,
    host_triple: &str,
) -> Vec<String> {
    if rust_target_triple != host_triple {
        linux_cross_linker_args(linker_program, rust_target_triple)
    } else {
        Vec::new()
    }
}

fn windows_host_linker_args(
    linker_program: &Path,
    rust_target_triple: &str,
    host_triple: &str,
) -> Vec<String> {
    if rust_target_triple != host_triple {
        clang_driver_target_args(linker_program, rust_target_triple)
    } else {
        Vec::new()
    }
}

fn clang_driver_target_args(linker_program: &Path, rust_target_triple: &str) -> Vec<String> {
    linker_program
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| is_clang_driver_name(name))
        .map(|_| vec!["--target".to_string(), rust_target_triple.to_string()])
        .unwrap_or_default()
}

fn is_clang_driver_name(name: &str) -> bool {
    let normalized = name.strip_suffix(".exe").unwrap_or(name);
    matches!(normalized, "clang" | "clang-cl") || normalized.starts_with("clang-")
}

fn validate_linux_cross_linker_program(
    linker_program: &Path,
    rust_target_triple: &str,
) -> Result<()> {
    let Some(name) = linker_program_name(linker_program) else {
        return Ok(());
    };

    if is_host_only_linux_cross_driver_name(name) {
        return Err(CliError::CommandFailed {
            command: format!(
                "configured Linux x86_64 linker '{}' is a host-only driver and cannot target '{}'; use clang with --target support or a target-prefixed cross linker",
                linker_program.display(),
                rust_target_triple,
            ),
            status: None,
        });
    }

    Ok(())
}

fn is_host_only_linux_cross_driver_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(normalized.as_str(), "cc" | "gcc") || normalized.starts_with("gcc-")
}

fn linker_program_name(linker_program: &Path) -> Option<&str> {
    linker_program
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.strip_suffix(".exe").unwrap_or(name))
}

fn write_linux_cross_linker_wrapper(
    linker_program: &Path,
    rust_target_triple: &str,
) -> Result<PathBuf> {
    let wrapper_dir = std::env::temp_dir().join("boltffi-jvm-linkers");
    std::fs::create_dir_all(&wrapper_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: wrapper_dir.clone(),
        source,
    })?;

    let linker_name = linker_program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("linker");
    let wrapper_path = wrapper_dir.join(format!("{linker_name}-{rust_target_triple}-wrapper.sh"));
    let script = format!(
        "#!/bin/sh\nexec \"{}\" --target {} \"$@\"\n",
        linker_program.display(),
        rust_target_triple
    );
    std::fs::write(&wrapper_path, script).map_err(|source| CliError::WriteFailed {
        path: wrapper_path.clone(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(&wrapper_path)
            .map_err(|source| CliError::ReadFailed {
                path: wrapper_path.clone(),
                source,
            })?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&wrapper_path, permissions).map_err(|source| {
            CliError::WriteFailed {
                path: wrapper_path.clone(),
                source,
            }
        })?;
    }

    Ok(wrapper_path)
}

fn apple_sdk_root() -> Result<PathBuf> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .map_err(|source| CliError::CommandFailed {
            command: format!("xcrun --sdk macosx --show-sdk-path: {source}"),
            status: None,
        })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: "xcrun --sdk macosx --show-sdk-path".to_string(),
            status: output.status.code(),
        });
    }

    let sdk_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sdk_root.is_empty() {
        return Err(CliError::CommandFailed {
            command: "xcrun --sdk macosx --show-sdk-path returned an empty SDK path".to_string(),
            status: None,
        });
    }

    Ok(PathBuf::from(sdk_root))
}

fn configured_linux_x86_64_cross_linker_values(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Vec<ConfiguredValue> {
    let cargo_env_key = cargo_linker_env_key(rust_target_triple);
    configured_linux_x86_64_cross_linker_values_with_sources(
        cargo_args,
        rust_target_triple,
        (rust_target_triple == default_linux_rust_target_triple(JavaHostTarget::LinuxX86_64))
            .then(|| std::env::var("BOLTFFI_JAVA_LINKER_X86_64_UNKNOWN_LINUX_GNU").ok())
            .flatten(),
        cargo_env_key.clone(),
        std::env::var(&cargo_env_key).ok(),
    )
}

fn configured_linux_x86_64_cross_linker_values_with_sources(
    cargo_args: &[String],
    rust_target_triple: &str,
    boltffi_linker: Option<String>,
    cargo_env_key: String,
    cargo_env_linker: Option<String>,
) -> Vec<ConfiguredValue> {
    cargo_inline_configured_linker_values(cargo_args, rust_target_triple)
        .into_iter()
        .chain(cargo_env_linker.into_iter().map(|value| ConfiguredValue {
            source: cargo_env_key.clone(),
            value,
        }))
        .chain(cargo_config_file_linker_values(
            cargo_args,
            rust_target_triple,
        ))
        .chain(boltffi_linker.into_iter().map(|value| ConfiguredValue {
            source: "BOLTFFI_JAVA_LINKER_X86_64_UNKNOWN_LINUX_GNU".to_string(),
            value,
        }))
        .collect()
}

#[cfg(test)]
fn resolve_linux_x86_64_cross_linker_from_values<I>(configured_values: I) -> Result<Option<PathBuf>>
where
    I: IntoIterator<Item = ConfiguredValue>,
{
    resolve_target_linker_from_values(configured_values, "Linux x86_64")
}

fn resolve_target_compiler_value(value: &str) -> Option<PathBuf> {
    let candidate = value.trim();
    if candidate.is_empty() {
        return None;
    }

    if linker_value_looks_like_path(candidate) {
        let path = PathBuf::from(candidate);
        return path_is_executable(&path).then_some(path);
    }

    if candidate.contains(char::is_whitespace) {
        return None;
    }

    which::which(candidate).ok()
}

fn linker_value_looks_like_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\')
}

fn is_linker_only_tool(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let normalized = name.to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "ld" | "ld.lld"
            | "ld.gold"
            | "rust-lld"
            | "mold"
            | "link"
            | "link.exe"
            | "lld-link"
            | "lld-link.exe"
    ) || normalized.ends_with("-ld")
        || normalized.ends_with("-ld.lld")
        || normalized.ends_with("-ld.gold")
        || normalized.ends_with("-rust-lld")
        || normalized.ends_with("-mold")
}

fn path_is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn missing_linux_x86_64_cross_linker_error(rust_target_triple: &str) -> CliError {
    let cargo_env_key = cargo_linker_env_key(rust_target_triple);
    let mut command = format!(
        "missing Linux x86_64 desktop linker for JVM packaging target '{}'; set {} or cargo target.{}.linker",
        rust_target_triple, cargo_env_key, rust_target_triple
    );

    if rust_target_triple == default_linux_rust_target_triple(JavaHostTarget::LinuxX86_64) {
        command.push_str(
            ", set BOLTFFI_JAVA_LINKER_X86_64_UNKNOWN_LINUX_GNU, or install x86_64-linux-gnu-clang",
        );
    }

    CliError::CommandFailed {
        command,
        status: None,
    }
}

fn cargo_inline_configured_linker_values(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Vec<ConfiguredValue> {
    let mut configured_values = Vec::new();
    let current_directory = std::env::current_dir().ok();

    for config_arg in extract_cargo_config_args(cargo_args).into_iter().rev() {
        if let Some(linker) =
            parse_target_linker_from_inline_config(&config_arg, rust_target_triple)
        {
            configured_values.push(ConfiguredValue {
                source: format!("cargo --config {}", config_arg),
                value: linker,
            });
            continue;
        }

        let config_path = resolve_cargo_config_path(&config_arg, current_directory.as_deref());
        if let Some(linker) = parse_target_linker_from_config_file(&config_path, rust_target_triple)
        {
            configured_values.push(ConfiguredValue {
                source: config_path.display().to_string(),
                value: linker,
            });
        }
    }

    configured_values
}

fn cargo_inline_configured_rustflags(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Vec<String> {
    let mut rustflags = Vec::new();
    let current_directory = std::env::current_dir().ok();

    for config_arg in extract_cargo_config_args(cargo_args).into_iter().rev() {
        if let Some(values) =
            parse_target_rustflags_from_inline_config(&config_arg, rust_target_triple)
        {
            rustflags.extend(values);
            continue;
        }

        let config_path = resolve_cargo_config_path(&config_arg, current_directory.as_deref());
        if let Some(values) =
            parse_target_rustflags_from_config_file(&config_path, rust_target_triple)
        {
            rustflags.extend(values);
        }
    }

    rustflags
}

fn cargo_config_file_linker_values(
    cargo_args: &[String],
    rust_target_triple: &str,
) -> Vec<ConfiguredValue> {
    let current_directory = std::env::current_dir().ok();
    cargo_config_file_linker_values_with_candidates(
        cargo_config_file_candidates(cargo_args, current_directory.as_deref()),
        rust_target_triple,
    )
}

fn cargo_config_file_rustflags(cargo_args: &[String], rust_target_triple: &str) -> Vec<String> {
    let mut rustflags = Vec::new();
    let current_directory = std::env::current_dir().ok();

    for config_path in cargo_config_file_candidates(cargo_args, current_directory.as_deref()) {
        if let Some(values) =
            parse_target_rustflags_from_config_file(&config_path, rust_target_triple)
        {
            rustflags.extend(values);
        }
    }

    rustflags
}

fn parse_target_rustflags_from_inline_config(
    config_arg: &str,
    rust_target_triple: &str,
) -> Option<Vec<String>> {
    let prefixes = [
        format!("target.{rust_target_triple}.rustflags="),
        format!("target.'{rust_target_triple}'.rustflags="),
        format!("target.\"{rust_target_triple}\".rustflags="),
    ];

    prefixes.into_iter().find_map(|prefix| {
        let value = config_arg.strip_prefix(&prefix)?;
        let parsed: toml::Value = toml::from_str(&format!("value = {value}")).ok()?;
        parse_rustflags_config_value(parsed.get("value")?)
    })
}

fn parse_target_rustflags_from_config_file(
    config_path: &Path,
    rust_target_triple: &str,
) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    parse_rustflags_config_value(target_config_value(
        &value,
        rust_target_triple,
        "rustflags",
    )?)
}

fn parse_rustflags_config_value(value: &toml::Value) -> Option<Vec<String>> {
    match value {
        toml::Value::String(value) => Some(split_shell_words(value)),
        toml::Value::Array(values) => values
            .iter()
            .map(|value| value.as_str().map(str::to_string))
            .collect(),
        _ => None,
    }
}

fn split_shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '\\' if !in_single => {
                let should_escape = chars.peek().is_some_and(|next| {
                    if in_double {
                        matches!(next, '\\' | '"')
                    } else {
                        next.is_whitespace() || matches!(next, '\\' | '\'' | '"')
                    }
                });

                if should_escape {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(ch);
                }
            }
            ch if ch.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn rustflags_to_linker_args(rustflags: &[String]) -> Vec<String> {
    let mut linker_args = Vec::new();
    let mut index = 0;

    while index < rustflags.len() {
        let flag = &rustflags[index];

        if flag == "-C" {
            if let Some(value) = rustflags.get(index + 1) {
                linker_args.extend(parse_codegen_linker_args(value));
                index += 2;
                continue;
            }
        } else if let Some(value) = flag.strip_prefix("-Clink-arg=") {
            linker_args.push(value.to_string());
            index += 1;
            continue;
        } else if let Some(value) = flag.strip_prefix("-Clink-args=") {
            linker_args.extend(split_shell_words(value));
            index += 1;
            continue;
        } else if flag == "-L" {
            if let Some(value) = rustflags.get(index + 1) {
                linker_args.extend(convert_rustc_link_search_flag(value));
                index += 2;
                continue;
            }
        } else if let Some(value) = flag.strip_prefix("-L") {
            if !value.is_empty() {
                linker_args.extend(convert_rustc_link_search_flag(value));
                index += 1;
                continue;
            }
        } else if flag == "-l" {
            if let Some(value) = rustflags.get(index + 1) {
                linker_args.push("-l".to_string());
                linker_args.push(value.clone());
                index += 2;
                continue;
            }
        } else if flag.starts_with("-l") {
            linker_args.push(flag.clone());
            index += 1;
            continue;
        }

        index += 1;
    }

    linker_args
}

fn parse_codegen_linker_args(value: &str) -> Vec<String> {
    if let Some(link_arg) = value.strip_prefix("link-arg=") {
        return vec![link_arg.to_string()];
    }

    if let Some(link_args) = value.strip_prefix("link-args=") {
        return split_shell_words(link_args);
    }

    Vec::new()
}

fn convert_rustc_link_search_flag(value: &str) -> Vec<String> {
    if let Some(path) = value
        .strip_prefix("native=")
        .or_else(|| value.strip_prefix("dependency="))
        .or_else(|| value.strip_prefix("crate="))
        .or_else(|| value.strip_prefix("all="))
    {
        return vec![format!("-L{path}")];
    }

    if let Some(path) = value.strip_prefix("framework=") {
        return vec![format!("-F{path}")];
    }

    vec![format!("-L{value}")]
}

fn cargo_config_file_linker_values_with_candidates(
    config_paths: Vec<PathBuf>,
    rust_target_triple: &str,
) -> Vec<ConfiguredValue> {
    let mut configured_values = Vec::new();

    for config_path in config_paths {
        if let Some(linker) = parse_target_linker_from_config_file(&config_path, rust_target_triple)
        {
            configured_values.push(ConfiguredValue {
                source: config_path.display().to_string(),
                value: linker,
            });
        }
    }

    configured_values
}

fn parse_target_linker_from_inline_config(
    config_arg: &str,
    rust_target_triple: &str,
) -> Option<String> {
    let prefixes = [
        format!("target.{rust_target_triple}.linker="),
        format!("target.'{rust_target_triple}'.linker="),
        format!("target.\"{rust_target_triple}\".linker="),
    ];

    prefixes.into_iter().find_map(|prefix| {
        config_arg
            .strip_prefix(&prefix)
            .map(trim_wrapping_quotes)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn parse_target_linker_from_config_file(
    config_path: &Path,
    rust_target_triple: &str,
) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    let linker = target_linker_from_config_value(&value, rust_target_triple)?;
    resolve_config_relative_linker(config_path, linker)
}

fn cargo_config_base_dir(config_path: &Path) -> Option<PathBuf> {
    let config_parent = config_path.parent()?;
    let is_dot_cargo_dir = config_parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == ".cargo");

    if is_dot_cargo_dir {
        config_parent.parent().map(Path::to_path_buf)
    } else {
        Some(config_parent.to_path_buf())
    }
}

fn target_linker_from_config_value<'a>(
    value: &'a toml::Value,
    rust_target_triple: &str,
) -> Option<&'a str> {
    target_config_value(value, rust_target_triple, "linker")?
        .as_str()
        .filter(|value| !value.is_empty())
}

fn target_config_value<'a>(
    value: &'a toml::Value,
    rust_target_triple: &str,
    config_key: &str,
) -> Option<&'a toml::Value> {
    let target_table = value.get("target")?.as_table()?;

    if let Some(config_value) = target_table
        .get(rust_target_triple)
        .and_then(|value| value.get(config_key))
    {
        return Some(config_value);
    }

    let target_cfg = CargoTargetCfg::for_target(rust_target_triple)?;
    target_table.iter().find_map(|(key, value)| {
        cargo_cfg_key_matches(key, &target_cfg)
            .then(|| value.get(config_key))
            .flatten()
    })
}

fn resolve_config_relative_linker(config_path: &Path, linker: &str) -> Option<String> {
    let linker_path = PathBuf::from(linker);
    if linker_path.is_absolute() || !linker_value_looks_like_path(linker) {
        return Some(linker.to_string());
    }

    let base_dir = cargo_config_base_dir(config_path)?;
    Some(base_dir.join(linker_path).display().to_string())
}

fn cargo_cfg_key_matches(key: &str, target_cfg: &CargoTargetCfg) -> bool {
    key.strip_prefix("cfg(")
        .and_then(|expression| expression.strip_suffix(')'))
        .is_some_and(|expression| cargo_cfg_expression_matches(expression, target_cfg))
}

fn cargo_cfg_expression_matches(expression: &str, target_cfg: &CargoTargetCfg) -> bool {
    let expression = expression.trim();
    if expression.is_empty() {
        return false;
    }

    if let Some(arguments) = cargo_cfg_function_arguments(expression, "all") {
        return arguments
            .iter()
            .all(|argument| cargo_cfg_expression_matches(argument, target_cfg));
    }

    if let Some(arguments) = cargo_cfg_function_arguments(expression, "any") {
        return arguments
            .iter()
            .any(|argument| cargo_cfg_expression_matches(argument, target_cfg));
    }

    if let Some(arguments) = cargo_cfg_function_arguments(expression, "not") {
        return arguments.len() == 1 && !cargo_cfg_expression_matches(arguments[0], target_cfg);
    }

    if let Some((key, value)) = expression.split_once('=') {
        return target_cfg.matches_key_value(key.trim(), trim_wrapping_quotes(value.trim()));
    }

    target_cfg.matches_name(expression)
}

fn cargo_cfg_function_arguments<'a>(expression: &'a str, name: &str) -> Option<Vec<&'a str>> {
    let remainder = expression.strip_prefix(name)?.trim_start();
    let inner = remainder.strip_prefix('(')?.strip_suffix(')')?;
    split_cargo_cfg_arguments(inner)
}

fn split_cargo_cfg_arguments(input: &str) -> Option<Vec<&str>> {
    if input.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut arguments = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }

            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                let argument = input[start..index].trim();
                if argument.is_empty() {
                    return None;
                }
                arguments.push(argument);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    if in_string || escaped || depth != 0 {
        return None;
    }

    let argument = input[start..].trim();
    if argument.is_empty() {
        return None;
    }
    arguments.push(argument);
    Some(arguments)
}

fn trim_wrapping_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|trimmed| trimmed.strip_suffix('\''))
        })
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{
        CargoTargetCfg, ConfiguredValue, NativeHostToolchain, cargo_cfg_expression_matches,
        cargo_config_base_dir, cargo_config_file_candidates_with_inputs,
        cargo_config_file_linker_values_with_candidates, cargo_config_search_roots,
        cargo_configured_build_target, cargo_inline_configured_linker_values, cargo_linker_env_key,
        configured_linux_build_target, configured_linux_x86_64_cross_linker_values_with_sources,
        configured_target_linker_values, configured_target_rustflag_linker_args,
        configured_target_rustflag_linker_args_with_sources, configured_windows_build_target,
        default_windows_jni_compiler_candidates, ensure_supported_native_host_pair,
        extract_cargo_config_args, fallback_without_rustup, linux_cross_linker_args,
        linux_host_linker_args, parse_build_target_from_config_file,
        parse_build_target_from_inline_config, parse_rustc_host_triple,
        parse_target_linker_from_config_file, parse_target_linker_from_inline_config,
        parse_target_rustflags_from_config_file, resolve_linux_cross_toolchain,
        resolve_linux_rust_target_triple, resolve_linux_x86_64_cross_linker_from_values,
        resolve_target_compiler_from_values, resolve_target_linker_from_values,
        resolve_windows_host_linker_with_host_triple, rustflags_to_linker_args,
        rustup_toolchain_name, split_shell_words, trim_wrapping_quotes,
        validate_installed_rustup_target, validate_linux_rust_target_triple,
        validate_windows_rust_target_triple, windows_host_linker_args,
        write_linux_cross_linker_wrapper,
    };
    use crate::cli::CliError;
    use crate::target::JavaHostTarget;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn extracts_rustup_toolchain_name_from_selector() {
        assert_eq!(rustup_toolchain_name(Some("+nightly")), Some("nightly"));
        assert_eq!(rustup_toolchain_name(Some("+stable")), Some("stable"));
        assert_eq!(rustup_toolchain_name(None), None);
    }

    #[test]
    fn parses_host_triple_from_rustc_verbose_output() {
        let triple = parse_rustc_host_triple(
            "rustc 1.90.0\nbinary: rustc\nhost: x86_64-pc-windows-msvc\nrelease: 1.90.0\n",
        )
        .expect("host triple");

        assert_eq!(triple, "x86_64-pc-windows-msvc");
    }

    #[test]
    fn builds_cargo_linker_env_key_from_target_triple() {
        assert_eq!(
            cargo_linker_env_key("x86_64-unknown-linux-gnu"),
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"
        );
    }

    #[test]
    fn accepts_configured_linux_linker_paths_with_spaces() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi linux linker test {unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("clang");
        fs::write(&linker_path, []).expect("write fake linker");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&linker_path)
                .expect("read fake linker metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&linker_path, permissions).expect("mark fake linker executable");
        }

        let resolved = resolve_linux_x86_64_cross_linker_from_values([ConfiguredValue {
            source: "test".to_string(),
            value: linker_path.display().to_string(),
        }])
        .expect("resolved linker");

        assert_eq!(resolved, Some(linker_path));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rejects_non_executable_linux_linker_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-nonexec-linker-test-{unique}"));
        let linker_path = temp_root.join("not-a-linker");
        fs::create_dir_all(&linker_path).expect("create non-executable linker path");

        let error = resolve_linux_x86_64_cross_linker_from_values([ConfiguredValue {
            source: "test".to_string(),
            value: linker_path.display().to_string(),
        }])
        .expect_err("non-executable linker path should fail");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(command.contains("does not resolve to an executable"));
                assert!(command.contains(&linker_path.display().to_string()));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn accepts_linker_only_linux_tools_for_cargo_linking() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-linker-only-tool-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("ld.lld");
        fs::write(&linker_path, []).expect("write fake linker-only tool");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&linker_path)
                .expect("read fake linker-only metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&linker_path, permissions)
                .expect("mark fake linker-only tool executable");
        }

        let resolved = resolve_linux_x86_64_cross_linker_from_values([ConfiguredValue {
            source: "test".to_string(),
            value: linker_path.display().to_string(),
        }])
        .expect("linker-only tool should still be usable for cargo");

        assert_eq!(resolved, Some(linker_path));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn skips_linker_only_linux_tools_when_selecting_jni_compiler() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-linker-only-compiler-skip-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_only_path = temp_root.join("ld.lld");
        let compiler_path = temp_root.join("clang");
        fs::write(&linker_only_path, []).expect("write fake linker-only tool");
        fs::write(&compiler_path, []).expect("write fake compiler");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            for path in [&linker_only_path, &compiler_path] {
                let mut permissions = fs::metadata(path)
                    .expect("read fake tool metadata")
                    .permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(path, permissions).expect("mark fake tool executable");
            }
        }

        let resolved = resolve_target_compiler_from_values(
            [
                ConfiguredValue {
                    source: "test".to_string(),
                    value: linker_only_path.display().to_string(),
                },
                ConfiguredValue {
                    source: "fallback".to_string(),
                    value: compiler_path.display().to_string(),
                },
            ],
            "Linux x86_64",
        )
        .expect("compiler selection should succeed");

        assert_eq!(resolved, Some(compiler_path));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn extracts_cargo_config_args_from_passthrough_args() {
        let config_args = extract_cargo_config_args(&[
            "--locked".to_string(),
            "--config".to_string(),
            "target.x86_64-unknown-linux-gnu.linker=\"zig\"".to_string(),
            "--config=custom-config.toml".to_string(),
        ]);

        assert_eq!(
            config_args,
            vec![
                "target.x86_64-unknown-linux-gnu.linker=\"zig\"".to_string(),
                "custom-config.toml".to_string(),
            ]
        );
    }

    #[test]
    fn parses_target_linker_from_inline_cargo_config() {
        let linker = parse_target_linker_from_inline_config(
            "target.x86_64-unknown-linux-gnu.linker=\"zig\"",
            "x86_64-unknown-linux-gnu",
        )
        .expect("inline linker");

        assert_eq!(linker, "zig");
    }

    #[test]
    fn parses_build_target_from_inline_cargo_config() {
        let target =
            parse_build_target_from_inline_config("build.target=\"x86_64-pc-windows-gnu\"")
                .expect("inline target");

        assert_eq!(target, "x86_64-pc-windows-gnu");
    }

    #[test]
    fn parses_target_linker_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-cargo-config-test-{unique}"));
        let cargo_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_dir).expect("create .cargo dir");
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"[target.x86_64-unknown-linux-gnu]
linker = "toolchains/linux-linker"
"#,
        )
        .expect("write cargo config");

        let linker = parse_target_linker_from_config_file(&config_path, "x86_64-unknown-linux-gnu")
            .expect("config linker");

        assert_eq!(
            linker,
            temp_root
                .join("toolchains/linux-linker")
                .display()
                .to_string()
        );

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_cfg_target_linker_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-cargo-cfg-config-test-{unique}"));
        let cargo_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_dir).expect("create .cargo dir");
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"[target.'cfg(all(unix, target_os = "linux", target_env = "gnu"))']
linker = "toolchains/linux-linker"
"#,
        )
        .expect("write cargo config");

        let linker = parse_target_linker_from_config_file(&config_path, "x86_64-unknown-linux-gnu")
            .expect("cfg config linker");

        assert_eq!(
            linker,
            temp_root
                .join("toolchains/linux-linker")
                .display()
                .to_string()
        );

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_cfg_target_rustflags_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-cargo-cfg-rustflags-test-{unique}"));
        let cargo_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_dir).expect("create .cargo dir");
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"[target.'cfg(all(unix, target_os = "linux", target_env = "gnu"))']
rustflags = ["-Clink-arg=--sysroot=/sdk", "-L", "native=/libs", "-lssl"]
"#,
        )
        .expect("write cargo config");

        let rustflags =
            parse_target_rustflags_from_config_file(&config_path, "x86_64-unknown-linux-gnu")
                .expect("cfg config rustflags");

        assert_eq!(
            rustflags_to_linker_args(&rustflags),
            vec![
                "--sysroot=/sdk".to_string(),
                "-L/libs".to_string(),
                "-lssl".to_string(),
            ]
        );

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn resolves_forward_slash_relative_linker_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-cargo-config-forward-slash-test-{unique}"));
        let cargo_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_dir).expect("create .cargo dir");
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"[target.x86_64-pc-windows-msvc]
linker = "toolchains/clang-cl.exe"
"#,
        )
        .expect("write cargo config");

        let linker = parse_target_linker_from_config_file(&config_path, "x86_64-pc-windows-msvc")
            .expect("config linker");

        assert_eq!(
            linker,
            temp_root
                .join("toolchains/clang-cl.exe")
                .display()
                .to_string()
        );

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn parses_build_target_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-build-target-config-test-{unique}"));
        let cargo_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_dir).expect("create .cargo dir");
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"[build]
target = "x86_64-pc-windows-gnu"
"#,
        )
        .expect("write cargo config");

        let target = parse_build_target_from_config_file(&config_path).expect("config target");

        assert_eq!(target, "x86_64-pc-windows-gnu");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn collects_cargo_inline_configured_linker_values() {
        let values = cargo_inline_configured_linker_values(
            &[
                "--config=/tmp/.cargo/config.toml".to_string(),
                "--config=target.x86_64-unknown-linux-gnu.linker='zig'".to_string(),
            ],
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].value, "zig");
        assert!(values[0].source.contains("cargo --config"));
    }

    #[test]
    fn collects_cargo_config_file_linker_values() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-cargo-config-values-{unique}"));
        let cargo_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_dir).expect("create .cargo dir");
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            r#"[target.x86_64-unknown-linux-gnu]
linker = "x86_64-linux-gnu-clang"
"#,
        )
        .expect("write cargo config");

        let values = cargo_config_file_linker_values_with_candidates(
            vec![config_path.clone()],
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].value, "x86_64-linux-gnu-clang");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn matches_nested_cargo_cfg_expressions() {
        let target_cfg = CargoTargetCfg::from_rustc_output(
            r#"target_arch="x86_64"
target_env="gnu"
target_os="linux"
unix
"#,
        );

        assert!(cargo_cfg_expression_matches(
            r#"all(unix, any(target_os = "linux", target_os = "macos"), not(target_env = "msvc"))"#,
            &target_cfg
        ));
        assert!(!cargo_cfg_expression_matches(
            r#"all(unix, target_os = "windows")"#,
            &target_cfg
        ));
    }

    #[test]
    fn collects_generic_target_linker_values_from_inline_config() {
        let values = configured_target_linker_values(
            &["--config=target.x86_64-unknown-linux-musl.linker='clang'".to_string()],
            "x86_64-unknown-linux-musl",
        );

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].value, "clang");
        assert!(values[0].source.contains("cargo --config"));
    }

    #[test]
    fn collects_target_rustflag_linker_args_from_inline_config() {
        let linker_args = configured_target_rustflag_linker_args(
            &["--config=target.x86_64-unknown-linux-musl.rustflags=['-C','link-arg=--sysroot=/sdk','-L','native=/libs']".to_string()],
            "x86_64-unknown-linux-musl",
        );

        assert_eq!(
            linker_args,
            vec!["--sysroot=/sdk".to_string(), "-L/libs".to_string()]
        );
    }

    #[test]
    fn prefers_cargo_encoded_rustflags_for_jni_linking() {
        let linker_args = configured_target_rustflag_linker_args_with_sources(
            &[
                "--config=target.x86_64-unknown-linux-musl.rustflags=['-L','native=/config','-C','link-arg=--sysroot=/config-sdk']".to_string(),
            ],
            "x86_64-unknown-linux-musl",
            Some("-Lnative=/encoded\u{1f}-lssl".to_string()),
            Some("-Lnative=/global -lcrypto".to_string()),
            Some("-Lnative=/target-env -lstatic=target".to_string()),
        );

        assert_eq!(
            linker_args,
            vec![
                "-L/encoded".to_string(),
                "-lssl".to_string(),
                "-L/config".to_string(),
                "--sysroot=/config-sdk".to_string(),
                "-L/target-env".to_string(),
                "-lstatic=target".to_string(),
            ]
        );
    }

    #[test]
    fn replays_global_rustflags_for_jni_linking() {
        let linker_args = configured_target_rustflag_linker_args_with_sources(
            &[
                "--config=target.x86_64-unknown-linux-musl.rustflags=['-L','native=/config','-C','link-arg=--sysroot=/config-sdk']".to_string(),
            ],
            "x86_64-unknown-linux-musl",
            None,
            Some("-Lnative=/global -lcrypto -C link-arg=--sysroot=/sdk".to_string()),
            Some("-Lnative=/target-env -lstatic=target".to_string()),
        );

        assert_eq!(
            linker_args,
            vec![
                "-L/global".to_string(),
                "-lcrypto".to_string(),
                "--sysroot=/sdk".to_string(),
                "-L/config".to_string(),
                "--sysroot=/config-sdk".to_string(),
                "-L/target-env".to_string(),
                "-lstatic=target".to_string(),
            ]
        );
    }

    #[test]
    fn prefers_inline_cargo_linker_over_cargo_target_env() {
        let values = configured_linux_x86_64_cross_linker_values_with_sources(
            &["--config=target.x86_64-unknown-linux-gnu.linker='zig'".to_string()],
            "x86_64-unknown-linux-gnu",
            None,
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER".to_string(),
            Some("x86_64-linux-gnu-clang".to_string()),
        );

        assert_eq!(values.len(), 2);
        assert_eq!(values[0].value, "zig");
        assert!(values[0].source.contains("cargo --config"));
        assert_eq!(
            values[1].source,
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"
        );
    }

    #[test]
    fn prefers_cargo_linker_sources_over_boltffi_linux_linker_override() {
        let values = configured_linux_x86_64_cross_linker_values_with_sources(
            &["--config=target.x86_64-unknown-linux-gnu.linker='zig'".to_string()],
            "x86_64-unknown-linux-gnu",
            Some("x86_64-linux-gnu-clang".to_string()),
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER".to_string(),
            None,
        );

        assert_eq!(values.len(), 2);
        assert_eq!(values[0].value, "zig");
        assert!(values[0].source.contains("cargo --config"));
        assert_eq!(
            values[1].source,
            "BOLTFFI_JAVA_LINKER_X86_64_UNKNOWN_LINUX_GNU"
        );
    }

    #[test]
    fn resolves_cargo_configured_build_target_from_config_args() {
        let target = cargo_configured_build_target(
            &["--config=build.target='x86_64-pc-windows-gnu'".to_string()],
            Some(Path::new("/tmp/workspace")),
        )
        .expect("configured build target");

        assert_eq!(target, "x86_64-pc-windows-gnu");
    }

    #[test]
    fn ignores_non_windows_cargo_build_target_for_windows_jvm_packaging() {
        let target = configured_windows_build_target(&[
            "--config=build.target='x86_64-unknown-linux-gnu'".to_string(),
        ])
        .expect("non-windows target should be ignored");

        assert_eq!(target, None);
    }

    #[test]
    fn honors_cross_host_linux_build_target_from_cargo_config() {
        let target = resolve_linux_rust_target_triple(
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::DarwinArm64,
            None,
            &["--config=build.target='x86_64-unknown-linux-musl'".to_string()],
        )
        .expect("cross-host linux build target");

        assert_eq!(target, "x86_64-unknown-linux-musl");
    }

    #[test]
    fn honors_compatible_cargo_build_target_for_current_linux_jvm_host_resolution() {
        let target = resolve_linux_rust_target_triple(
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::LinuxX86_64,
            None,
            &["--config=build.target='x86_64-unknown-linux-musl'".to_string()],
        )
        .expect("current linux host should honor compatible cargo build target");

        assert_eq!(target, "x86_64-unknown-linux-musl");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn ignores_cargo_build_target_for_current_linux_jvm_host_resolution() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let configured_target = match current_host {
            JavaHostTarget::LinuxX86_64 => "aarch64-unknown-linux-gnu",
            JavaHostTarget::LinuxAarch64 => "x86_64-unknown-linux-gnu",
            other => panic!("unexpected current host for linux test: {other:?}"),
        };
        let target = resolve_linux_rust_target_triple(
            current_host,
            current_host,
            None,
            &[format!("--config=build.target='{configured_target}'")],
        )
        .expect("current host should ignore mismatched cargo build target");

        assert_eq!(
            target,
            super::rustc_host_triple(None).expect("rustc host triple")
        );
    }

    #[test]
    fn honors_compatible_cargo_build_target_for_current_windows_jvm_host_resolution() {
        let target = super::resolve_windows_rust_target_triple(
            JavaHostTarget::WindowsX86_64,
            JavaHostTarget::WindowsX86_64,
            None,
            &["--config=build.target='x86_64-pc-windows-gnu'".to_string()],
        )
        .expect("current windows host should honor compatible cargo build target");

        assert_eq!(target, "x86_64-pc-windows-gnu");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn ignores_cargo_build_target_for_current_windows_jvm_host_resolution() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let target = super::resolve_windows_rust_target_triple(
            current_host,
            current_host,
            None,
            &["--config=build.target='x86_64-unknown-linux-gnu'".to_string()],
        )
        .expect("current host should ignore non-windows cargo build target");

        assert_eq!(
            target,
            super::rustc_host_triple(None).expect("rustc host triple")
        );
    }

    #[test]
    fn uses_configured_windows_gnu_linker_for_jni_host_linking() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-windows-gnu-linker-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("x86_64-w64-mingw32-gcc");
        fs::write(&linker_path, []).expect("write fake mingw compiler");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&linker_path)
                .expect("read fake mingw compiler metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&linker_path, permissions)
                .expect("mark fake mingw compiler executable");
        }

        let (linker_program, linker_args) = resolve_windows_host_linker_with_host_triple(
            "x86_64-pc-windows-msvc".to_string(),
            &[format!(
                "--config=target.x86_64-pc-windows-gnu.linker='{}'",
                linker_path.display()
            )],
            "x86_64-pc-windows-gnu",
        )
        .expect("configured windows gnu linker");

        assert_eq!(linker_program, linker_path);
        assert!(linker_args.is_empty());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn uses_configured_windows_msvc_linker_for_jni_host_linking() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-windows-msvc-linker-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("clang");
        fs::write(&linker_path, []).expect("write fake clang compiler");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&linker_path)
                .expect("read fake clang metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&linker_path, permissions)
                .expect("mark fake clang compiler executable");
        }

        let (linker_program, linker_args) = resolve_windows_host_linker_with_host_triple(
            "x86_64-pc-windows-msvc".to_string(),
            &[format!(
                "--config=target.x86_64-pc-windows-msvc.linker='{}'",
                linker_path.display()
            )],
            "x86_64-pc-windows-msvc",
        )
        .expect("configured windows msvc linker");

        assert_eq!(linker_program, linker_path);
        assert!(linker_args.is_empty());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rejects_unsupported_jvm_host_pairs_before_toolchain_probing() {
        let error = ensure_supported_native_host_pair(
            JavaHostTarget::DarwinArm64,
            JavaHostTarget::WindowsX86_64,
            "JVM",
        )
        .expect_err("unsupported host pair should fail");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(command.contains("windows-x86_64"));
                assert!(command.contains("darwin-arm64"));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn accepts_linux_musl_build_target_for_current_host_jvm_packaging() {
        let target = configured_linux_build_target(
            &["--config=build.target='x86_64-unknown-linux-musl'".to_string()],
            JavaHostTarget::LinuxX86_64,
        )
        .expect("linux musl target should be accepted");

        assert_eq!(target, Some("x86_64-unknown-linux-musl".to_string()));
    }

    #[test]
    fn rejects_mismatched_linux_build_target_for_current_host_jvm_packaging() {
        let error = validate_linux_rust_target_triple(
            "aarch64-unknown-linux-musl",
            JavaHostTarget::LinuxX86_64,
        )
        .expect_err("mismatched linux target should fail");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(command.contains("aarch64-unknown-linux-musl"));
                assert!(command.contains("linux-x86_64"));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_android_linux_build_target_for_desktop_jvm_packaging() {
        let error = configured_linux_build_target(
            &["--config=build.target='x86_64-linux-android'".to_string()],
            JavaHostTarget::LinuxX86_64,
        )
        .expect_err("android target should be rejected");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(command.contains("x86_64-linux-android"));
                assert!(command.contains("linux-x86_64"));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn includes_global_cargo_config_candidates() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-global-cargo-config-test-{unique}"));
        let cargo_home = temp_root.join("cargo-home");
        let home_dir = temp_root.join("home");
        fs::create_dir_all(&cargo_home).expect("create cargo home");
        fs::create_dir_all(home_dir.join(".cargo")).expect("create home .cargo");
        fs::write(cargo_home.join("config.toml"), []).expect("write cargo home config");
        fs::write(home_dir.join(".cargo").join("config"), []).expect("write home cargo config");

        let candidates = cargo_config_file_candidates_with_inputs(
            Vec::new(),
            None,
            Some(cargo_home.clone()),
            Some(home_dir.clone()),
        );

        assert!(candidates.contains(&cargo_home.join("config.toml")));
        assert!(candidates.contains(&home_dir.join(".cargo").join("config")));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn resolves_relative_cargo_home_for_global_config_candidates() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-relative-cargo-home-test-{unique}"));
        let current_dir = temp_root.join("workspace");
        let cargo_home = PathBuf::from("relative-cargo-home");
        let resolved_cargo_home = current_dir.join(&cargo_home);
        fs::create_dir_all(&resolved_cargo_home).expect("create relative cargo home");
        fs::write(resolved_cargo_home.join("config.toml"), []).expect("write cargo home config");

        let candidates = cargo_config_file_candidates_with_inputs(
            vec![current_dir.clone()],
            Some(current_dir),
            Some(cargo_home),
            None,
        );

        assert!(candidates.contains(&resolved_cargo_home.join("config.toml")));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn includes_selected_manifest_directory_in_cargo_config_search_roots() {
        let current_dir = PathBuf::from("/tmp/runner");
        let roots = cargo_config_search_roots(
            &[
                "--manifest-path".to_string(),
                "../workspace/member/Cargo.toml".to_string(),
            ],
            Some(current_dir.as_path()),
        );

        assert_eq!(roots[0], PathBuf::from("/tmp/workspace/member"));
        assert_eq!(roots[1], current_dir);
    }

    #[test]
    fn resolves_cargo_config_base_dir_for_dot_cargo_files() {
        let base_dir = cargo_config_base_dir(Path::new("/tmp/workspace/.cargo/config.toml"))
            .expect("base dir");

        assert_eq!(base_dir, PathBuf::from("/tmp/workspace"));
    }

    #[test]
    fn trims_wrapping_quotes_from_config_values() {
        assert_eq!(trim_wrapping_quotes("\"zig\""), "zig");
        assert_eq!(trim_wrapping_quotes("'zig'"), "zig");
        assert_eq!(trim_wrapping_quotes("zig"), "zig");
    }

    #[test]
    fn detects_target_args_for_generic_linux_cross_linkers() {
        assert_eq!(
            linux_cross_linker_args(Path::new("/usr/bin/clang"), "x86_64-unknown-linux-gnu"),
            vec![
                "--target".to_string(),
                "x86_64-unknown-linux-gnu".to_string()
            ]
        );
        assert_eq!(
            linux_cross_linker_args(Path::new("/usr/bin/clang-18"), "x86_64-unknown-linux-gnu"),
            vec![
                "--target".to_string(),
                "x86_64-unknown-linux-gnu".to_string()
            ]
        );
        assert!(
            linux_cross_linker_args(Path::new("/usr/bin/gcc-14"), "x86_64-unknown-linux-gnu")
                .is_empty()
        );
        assert!(
            linux_cross_linker_args(Path::new("/usr/bin/cc"), "x86_64-unknown-linux-gnu")
                .is_empty()
        );
        assert!(
            linux_cross_linker_args(
                Path::new("/usr/bin/x86_64-linux-gnu-clang"),
                "x86_64-unknown-linux-gnu"
            )
            .is_empty()
        );
    }

    #[test]
    fn adds_target_args_for_nondefault_linux_host_targets() {
        assert_eq!(
            linux_host_linker_args(
                Path::new("/usr/bin/clang"),
                "x86_64-unknown-linux-musl",
                "x86_64-unknown-linux-gnu"
            ),
            vec![
                "--target".to_string(),
                "x86_64-unknown-linux-musl".to_string()
            ]
        );
        assert!(
            linux_host_linker_args(
                Path::new("/usr/bin/clang"),
                "x86_64-unknown-linux-musl",
                "x86_64-unknown-linux-musl"
            )
            .is_empty()
        );
    }

    #[test]
    fn detects_clang_cl_exe_as_clang_driver() {
        assert_eq!(
            windows_host_linker_args(
                Path::new("C:/LLVM/bin/clang-cl.exe"),
                "x86_64-pc-windows-msvc",
                "aarch64-pc-windows-msvc"
            ),
            vec!["--target".to_string(), "x86_64-pc-windows-msvc".to_string()]
        );
    }

    #[test]
    fn uses_explicit_windows_jni_compiler_candidates_per_driver_family() {
        assert_eq!(
            default_windows_jni_compiler_candidates("x86_64-pc-windows-msvc"),
            vec!["clang-cl", "cl", "clang"]
        );
        assert_eq!(
            default_windows_jni_compiler_candidates("x86_64-pc-windows-gnu"),
            vec![
                "x86_64-w64-mingw32-gcc",
                "x86_64-w64-mingw32-clang",
                "gcc",
                "clang"
            ]
        );
    }

    #[test]
    fn extracts_linker_args_from_rustflags() {
        let linker_args = rustflags_to_linker_args(&[
            "-C".to_string(),
            "link-arg=--sysroot=/sdk".to_string(),
            "-Clink-args=-Wl,--as-needed -pthread".to_string(),
            "-Lnative=/libs".to_string(),
            "-lssl".to_string(),
        ]);

        assert_eq!(
            linker_args,
            vec![
                "--sysroot=/sdk".to_string(),
                "-Wl,--as-needed".to_string(),
                "-pthread".to_string(),
                "-L/libs".to_string(),
                "-lssl".to_string(),
            ]
        );
    }

    #[test]
    fn preserves_backslashes_when_splitting_windows_rustflags() {
        assert_eq!(
            split_shell_words(r#"-Lnative=C:\deps\lib -Clink-arg=/LIBPATH:C:\sdk\lib"#),
            vec![
                r#"-Lnative=C:\deps\lib"#.to_string(),
                r#"-Clink-arg=/LIBPATH:C:\sdk\lib"#.to_string(),
            ]
        );
    }

    #[test]
    fn writes_wrapper_for_generic_linux_cross_linkers() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-linux-wrapper-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("clang");
        fs::write(&linker_path, []).expect("write fake linker");

        let wrapper_path =
            write_linux_cross_linker_wrapper(&linker_path, "x86_64-unknown-linux-gnu")
                .expect("wrapper path");
        let wrapper = fs::read_to_string(&wrapper_path).expect("read wrapper");

        assert!(wrapper.contains("--target x86_64-unknown-linux-gnu"));
        assert!(wrapper.contains(&linker_path.display().to_string()));

        fs::remove_file(&wrapper_path).expect("cleanup wrapper");
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rejects_invalid_higher_precedence_configured_linux_linker() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-invalid-linker-precedence-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let fallback_linker = temp_root.join("x86_64-linux-gnu-clang");
        fs::write(&fallback_linker, []).expect("write fake linker");

        let error = resolve_linux_x86_64_cross_linker_from_values([
            ConfiguredValue {
                source: ".cargo/config.toml".to_string(),
                value: "/missing/repo-local-linker".to_string(),
            },
            ConfiguredValue {
                source: "~/.cargo/config.toml".to_string(),
                value: fallback_linker.display().to_string(),
            },
        ])
        .expect_err("higher-precedence invalid linker should fail");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(command.contains(".cargo/config.toml"));
                assert!(command.contains("/missing/repo-local-linker"));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rejects_host_only_gcc_for_linux_cross_toolchain() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-host-gcc-cross-linker-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("gcc");
        fs::write(&linker_path, []).expect("write fake gcc");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&linker_path)
                .expect("read fake gcc metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&linker_path, permissions).expect("mark fake gcc executable");
        }

        let error = resolve_linux_cross_toolchain(
            &[format!(
                "--config=target.x86_64-unknown-linux-gnu.linker='{}'",
                linker_path.display()
            )],
            "x86_64-unknown-linux-gnu",
        )
        .expect_err("host-only gcc should be rejected for Linux cross toolchains");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(command.contains("host-only driver"));
                assert!(command.contains(&linker_path.display().to_string()));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn falls_back_from_windows_link_exe_to_compiler_driver() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-windows-link-exe-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let linker_path = temp_root.join("link.exe");
        let compiler_path = temp_root.join("clang-cl.exe");
        fs::write(&linker_path, []).expect("write fake link.exe");
        fs::write(&compiler_path, []).expect("write fake clang-cl.exe");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            for path in [&linker_path, &compiler_path] {
                let mut permissions = fs::metadata(path)
                    .expect("read fake tool metadata")
                    .permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(path, permissions).expect("mark fake tool executable");
            }
        }

        let cargo_linker = resolve_target_linker_from_values(
            [ConfiguredValue {
                source: "config".to_string(),
                value: linker_path.display().to_string(),
            }],
            "Windows desktop",
        )
        .expect("cargo linker should resolve");
        let jni_compiler = resolve_target_compiler_from_values(
            [
                ConfiguredValue {
                    source: "config".to_string(),
                    value: linker_path.display().to_string(),
                },
                ConfiguredValue {
                    source: "fallback".to_string(),
                    value: compiler_path.display().to_string(),
                },
            ],
            "Windows desktop",
        )
        .expect("jni compiler should resolve");

        assert_eq!(cargo_linker, Some(linker_path));
        assert_eq!(jni_compiler, Some(compiler_path));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn uses_configured_cl_exe_as_msvc_jni_compiler() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-cl-exe-jni-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let compiler_path = temp_root.join("cl.exe");
        fs::write(&compiler_path, []).expect("write fake cl.exe");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&compiler_path)
                .expect("read fake cl.exe metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&compiler_path, permissions).expect("mark fake cl.exe executable");
        }

        let (linker_program, linker_args) = resolve_windows_host_linker_with_host_triple(
            "x86_64-pc-windows-msvc".to_string(),
            &[format!(
                "--config=target.x86_64-pc-windows-msvc.linker='{}'",
                compiler_path.display()
            )],
            "x86_64-pc-windows-msvc",
        )
        .expect("cl.exe should be accepted");

        assert_eq!(linker_program, compiler_path.clone());
        assert!(linker_args.is_empty());

        let toolchain = NativeHostToolchain {
            rust_target_triple: "x86_64-pc-windows-msvc".to_string(),
            cargo_linker_env: None,
            jni_compiler_program: compiler_path,
            jni_compiler_args: Vec::new(),
            jni_rustflag_linker_args: Vec::new(),
        };
        assert!(toolchain.uses_msvc_compiler());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn missing_rustup_does_not_block_current_host_packaging() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        fallback_without_rustup(current_host, current_host).expect("current host should pass");
    }

    #[test]
    fn missing_rustup_defers_cross_host_validation_to_cargo() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let other_host = [
            JavaHostTarget::DarwinArm64,
            JavaHostTarget::DarwinX86_64,
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::LinuxAarch64,
            JavaHostTarget::WindowsX86_64,
        ]
        .into_iter()
        .find(|target| *target != current_host)
        .expect("alternate host");

        fallback_without_rustup(other_host, current_host)
            .expect("cross-host fallback should defer to cargo");
    }

    #[test]
    fn rejects_missing_rustup_target_when_rustup_succeeds() {
        let error =
            validate_installed_rustup_target(&["aarch64-apple-darwin"], "x86_64-unknown-linux-gnu")
                .expect_err("missing target should fail");

        match error {
            CliError::CommandFailed { command, status } => {
                assert!(
                    command.contains("rustup target 'x86_64-unknown-linux-gnu' is not installed")
                );
                assert!(command.contains("rustup target add x86_64-unknown-linux-gnu"));
                assert_eq!(status, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn validates_configured_windows_gnu_target_triple() {
        let target = validate_windows_rust_target_triple("x86_64-pc-windows-gnu")
            .expect("windows gnu target");

        assert_eq!(target, "x86_64-pc-windows-gnu");
    }

    #[test]
    fn validates_configured_windows_gnullvm_target_triple() {
        let target = validate_windows_rust_target_triple("x86_64-pc-windows-gnullvm")
            .expect("windows gnullvm target");

        assert_eq!(target, "x86_64-pc-windows-gnullvm");
    }
}
