pub mod android;
pub mod apple;
pub mod csharp;
pub mod dart;
pub mod java;
pub mod python;
pub mod symbols;
pub mod wasm;

use std::path::PathBuf;
use std::process::Command;

use console::style;

use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::target::{BuiltLibrary, RustTarget};

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("no built libraries found for {platform}")]
    NoLibrariesFound { platform: String },

    #[error("missing built libraries for {platform}: {targets:?}")]
    MissingBuiltLibraries {
        platform: String,
        targets: Vec<String>,
    },

    #[error("xcframework creation failed")]
    XcframeworkFailed { source: std::io::Error },

    #[error("lipo failed for simulator fat library")]
    LipoFailed { source: std::io::Error },

    #[error("zip creation failed")]
    ZipFailed { source: std::io::Error },

    #[error("build failed for targets: {targets:?}")]
    BuildFailed { targets: Vec<String> },
}

pub(crate) fn resolve_build_cargo_args(config: &Config, cli_cargo_args: &[String]) -> Vec<String> {
    config
        .cargo_args_for_command("build")
        .into_iter()
        .chain(cli_cargo_args.iter().cloned())
        .collect()
}

pub(crate) fn discover_built_libraries_for_targets(
    crate_artifact_name: &str,
    profile_directory_name: &str,
    targets: &[RustTarget],
) -> Result<Vec<BuiltLibrary>> {
    let target_directory = cargo_target_directory()?;
    Ok(BuiltLibrary::discover_for_targets(
        &target_directory,
        crate_artifact_name,
        profile_directory_name,
        targets,
    ))
}

pub(crate) fn missing_built_libraries(
    targets: &[RustTarget],
    libraries: &[BuiltLibrary],
) -> Vec<String> {
    targets
        .iter()
        .filter(|target| libraries.iter().all(|library| library.target != **target))
        .map(|target| target.triple().to_string())
        .collect()
}

pub(crate) fn print_cargo_line(line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("Fresh") {
        return;
    }

    if trimmed.starts_with("Compiling") {
        println!("      {}", style(trimmed).green());
    } else if trimmed.starts_with("Finished") {
        println!("      {}", style(trimmed).green().bold());
    } else if trimmed.starts_with("warning:") {
        println!("      {}", style(trimmed).yellow());
    } else if trimmed.starts_with("error") {
        println!("      {}", style(trimmed).red().bold());
    } else if trimmed.starts_with("Checking") {
        println!("      {}", style(trimmed).green());
    } else if trimmed.starts_with("Building") {
        println!("      {}", style(trimmed).cyan());
    } else {
        println!("      {}", style(trimmed).dim());
    }
}

pub(crate) fn print_verbose_detail(line: &str) {
    println!("      {}", style(line).dim());
}

pub(crate) fn format_command_for_log(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|value| shell_escape_for_log(&value.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_escape_for_log(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | ':' | '=')
    }) {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn cargo_target_directory() -> Result<PathBuf> {
    let crate_directory = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;
    let output = Command::new("cargo")
        .current_dir(&crate_directory)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|source| CliError::CommandFailed {
            command: format!("cargo metadata: {source}"),
            status: None,
        })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: "cargo metadata --format-version 1 --no-deps".to_string(),
            status: output.status.code(),
        });
    }

    parse_target_directory(&output.stdout)
}

fn parse_target_directory(metadata: &[u8]) -> Result<PathBuf> {
    #[derive(serde::Deserialize)]
    struct CargoTargetDirectory {
        target_directory: PathBuf,
    }

    serde_json::from_slice::<CargoTargetDirectory>(metadata)
        .map(|metadata| metadata.target_directory)
        .map_err(|source| CliError::CommandFailed {
            command: format!("parse cargo metadata: {source}"),
            status: None,
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::missing_built_libraries;
    use crate::target::{BuiltLibrary, RustTarget};

    #[test]
    fn reports_missing_built_libraries_for_unbuilt_configured_targets() {
        let libraries = vec![BuiltLibrary {
            target: RustTarget::ANDROID_ARM64,
            path: PathBuf::from("/tmp/libdemo.a"),
        }];

        let missing = missing_built_libraries(
            &[RustTarget::ANDROID_ARM64, RustTarget::ANDROID_X86_64],
            &libraries,
        );

        assert_eq!(missing, vec!["x86_64-linux-android".to_string()]);
    }
}
