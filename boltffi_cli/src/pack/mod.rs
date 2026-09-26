pub mod android;
pub mod apple;
pub mod c;
pub mod csharp;
pub mod dart;
pub mod java;
pub mod kmp;
pub mod python;
pub(crate) mod scratch;
pub mod symbols;
pub mod wasm;

use std::path::PathBuf;
use std::process::Command;

use console::style;

use crate::cli::{CliError, Result};
use crate::commands::generate::GenerateTarget;
use crate::config::{Config, TargetSection};
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

/// Build args from config for `section`, then the CLI `--cargo-arg`s.
pub(crate) fn resolve_build_cargo_args(
    config: &Config,
    section: TargetSection,
    cli_cargo_args: &[String],
) -> Vec<String> {
    config
        .cargo_args_for_target(section, &["build"])
        .into_iter()
        .chain(cli_cargo_args.iter().cloned())
        .collect()
}

/// `--cargo-arg`s for a `generate` step that `pack` runs for `section`. `generate` adds the config
/// args itself, but `generate header` reads no target table, so it also gets `section`'s.
pub(crate) fn pack_generate_cargo_args(
    config: &Config,
    section: TargetSection,
    target: &GenerateTarget,
    cli_cargo_args: &[String],
) -> Vec<String> {
    let target_cargo_args = match target {
        GenerateTarget::Header => config.targets.cargo_args(section),
        _ => &[],
    };
    target_cargo_args
        .iter()
        .chain(cli_cargo_args)
        .cloned()
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

    use super::{missing_built_libraries, pack_generate_cargo_args, resolve_build_cargo_args};
    use crate::commands::generate::bindings::generation_cargo_args;
    use crate::commands::generate::{GenerateOptions, GenerateTarget};
    use crate::config::{Config, TargetSection};
    use crate::target::{BuiltLibrary, RustTarget};

    #[test]
    fn appends_cli_cargo_args_after_target_cargo_args() {
        let config: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[cargo]
global_args = ["--locked"]

[targets.android]
cargo_args = ["--no-default-features", "--features=kotlin"]
"#,
        )
        .expect("toml parse failed");

        assert_eq!(
            resolve_build_cargo_args(
                &config,
                TargetSection::Android,
                &["--features=extra".to_string()]
            ),
            vec![
                "--locked".to_string(),
                "--no-default-features".to_string(),
                "--features=kotlin".to_string(),
                "--features=extra".to_string(),
            ]
        );
    }

    #[test]
    fn pack_generate_steps_receive_config_cargo_args_once() {
        let config: Config = toml::from_str(
            r#"
[package]
name = "mylib"

[cargo]
global_args = ["--locked"]

[targets.android]
cargo_args = ["--features=android"]

[targets.wasm]
cargo_args = ["--features=wasm"]

[targets.dart]
cargo_args = ["--features=dart"]

[targets.c]
cargo_args = ["--features=c"]
"#,
        )
        .expect("toml parse failed");
        let cli_cargo_args = vec!["--features=extra".to_string()];

        [
            (
                TargetSection::Android,
                GenerateTarget::Kotlin,
                Some(TargetSection::Android),
            ),
            (TargetSection::Android, GenerateTarget::Header, None),
            (
                TargetSection::Wasm,
                GenerateTarget::Typescript,
                Some(TargetSection::Wasm),
            ),
            (
                TargetSection::Dart,
                GenerateTarget::Dart,
                Some(TargetSection::Dart),
            ),
            (TargetSection::C, GenerateTarget::C, Some(TargetSection::C)),
        ]
        .into_iter()
        .for_each(|(pack_section, target, generate_section)| {
            let options = GenerateOptions {
                cargo_args: pack_generate_cargo_args(
                    &config,
                    pack_section,
                    &target,
                    &cli_cargo_args,
                ),
                target,
                output: None,
                experimental: false,
                deny_skipped: false,
            };

            assert_eq!(
                generation_cargo_args(&config, generate_section, &options),
                resolve_build_cargo_args(&config, pack_section, &cli_cargo_args),
                "{pack_section:?}"
            );
        });
    }

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
