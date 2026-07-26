use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use object::{Object, ObjectSection, ObjectSymbol};
use serde::Serialize;

use crate::build::CargoBuildProfile;
use crate::cargo::Cargo;
use crate::cargo::config::{
    cargo_config_file_candidates, extract_cargo_config_args, parse_debuginfo_toml_value,
    parse_inline_config_value, parse_profile_debug_from_inline_config, resolve_cargo_config_path,
};
use crate::cli::{CliError, Result};
use crate::pack::PackError;
use crate::target::{Architecture, Platform};

#[derive(Debug, Clone)]
pub(crate) struct DebugSymbolArtifact {
    pub(crate) source_path: PathBuf,
    pub(crate) archive_path: PathBuf,
    pub(crate) kind: DebugSymbolArtifactKind,
    pub(crate) target_triple: Option<String>,
    pub(crate) platform: Option<Platform>,
    pub(crate) architecture: Option<Architecture>,
    pub(crate) abi: Option<String>,
    pub(crate) host_target: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DebugSymbolArtifactKind {
    Static,
    Shared,
    Jni,
    DebugInfo,
}

#[derive(Debug, Serialize)]
struct DebugSymbolManifest<'a> {
    version: u8,
    platform: &'a str,
    bundle: &'a str,
    artifacts: Vec<DebugSymbolManifestEntry>,
}

#[derive(Debug, Serialize)]
struct DebugSymbolManifestEntry {
    path: String,
    kind: &'static str,
    target_triple: Option<String>,
    platform: Option<&'static str>,
    architecture: Option<&'static str>,
    abi: Option<String>,
    host_target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileBooleanSetting {
    Debug,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EffectiveProfileSettings {
    has_debuginfo: bool,
    strips_symbols: bool,
    externalizes_debug_info: bool,
}

struct ProfileResolutionContext<'a> {
    working_directory: &'a Path,
    explicit_config_args: Vec<String>,
    cargo_config_files: Vec<PathBuf>,
    manifest: &'a toml::Value,
    selected_package_name: Option<&'a str>,
}

struct ExternalizedDebugInfoCheck<'a> {
    cargo: &'a Cargo,
    metadata: &'a crate::cargo::CargoMetadata,
    working_directory: &'a Path,
    cargo_args: &'a [String],
    build_profile: &'a CargoBuildProfile,
    rust_target_triples: &'a [String],
    selected_package_name: Option<&'a str>,
}

impl ProfileBooleanSetting {
    fn env_suffix(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Strip => "STRIP",
        }
    }

    fn toml_key(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Strip => "strip",
        }
    }

    fn parse_inline_value(self, config_argument: &str, profile_name: &str) -> Option<bool> {
        match self {
            Self::Debug => parse_profile_debug_from_inline_config(config_argument, profile_name),
            Self::Strip => parse_profile_strip_from_inline_config(config_argument, profile_name),
        }
    }

    fn parse_toml_value(self, value: &toml::Value) -> Option<bool> {
        match self {
            Self::Debug => parse_debuginfo_toml_value(value),
            Self::Strip => parse_strip_toml_value(value),
        }
    }

    fn parse_env_value(self, value: &str) -> Option<bool> {
        match self {
            Self::Debug => parse_env_debuginfo_value(value),
            Self::Strip => parse_env_strip_value(value),
        }
    }
}

pub(crate) fn write_debug_symbols_zip(
    output_dir: &Path,
    archive_name: &str,
    platform: &str,
    bundle: &str,
    artifacts: &[DebugSymbolArtifact],
) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_dir.to_path_buf(),
        source,
    })?;

    let archive_path = output_dir.join(archive_name);
    let bundle_root_name = archive_name.strip_suffix(".zip").unwrap_or(archive_name);
    let staging_root = output_dir.join(format!(".{bundle_root_name}.staging"));
    let bundle_root = staging_root.join(bundle_root_name);

    if staging_root.exists() {
        std::fs::remove_dir_all(&staging_root).map_err(|source| CliError::WriteFailed {
            path: staging_root.clone(),
            source,
        })?;
    }

    std::fs::create_dir_all(&bundle_root).map_err(|source| CliError::CreateDirectoryFailed {
        path: bundle_root.clone(),
        source,
    })?;

    for artifact in artifacts {
        let dest_path = bundle_root.join(&artifact.archive_path);
        copy_debug_symbol_artifact(&artifact.source_path, &dest_path)?;
    }

    let manifest_path = bundle_root.join("symbols.json");
    let manifest = DebugSymbolManifest {
        version: 1,
        platform,
        bundle,
        artifacts: artifacts.iter().map(build_manifest_entry).collect(),
    };
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(|source| CliError::CommandFailed {
            command: format!("serialize debug symbols manifest: {source}"),
            status: None,
        })?;
    std::fs::write(&manifest_path, manifest_json).map_err(|source| CliError::WriteFailed {
        path: manifest_path,
        source,
    })?;

    create_zip(&bundle_root, &archive_path)?;

    std::fs::remove_dir_all(&staging_root).map_err(|source| CliError::WriteFailed {
        path: staging_root,
        source,
    })?;

    Ok(archive_path)
}

pub(crate) fn ensure_debug_symbols_profile_has_debuginfo(
    cargo_args: &[String],
    build_profile: &CargoBuildProfile,
    config_path: &str,
    rust_target_triples: &[String],
) -> Result<()> {
    let cargo = Cargo::current(cargo_args)?;
    let metadata = cargo.metadata()?;
    let working_directory = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;
    let settings = resolve_effective_profile_settings(
        &cargo,
        &metadata,
        &working_directory,
        cargo_args,
        build_profile,
        rust_target_triples,
        None,
    )?;

    if !settings.has_debuginfo {
        let profile_name = build_profile.cargo_profile_name();
        return Err(CliError::CommandFailed {
            command: format!(
                "{config_path}.enabled requires Cargo debuginfo for the '{profile_name}' profile; enable [profile.{profile_name}].debug = true or an equivalent Cargo profile override"
            ),
            status: None,
        });
    }

    if settings.strips_symbols {
        let profile_name = build_profile.cargo_profile_name();
        return Err(CliError::CommandFailed {
            command: format!(
                "{config_path}.enabled requires unstripped Cargo artifacts for the '{profile_name}' profile; disable [profile.{profile_name}].strip or an equivalent Cargo profile override"
            ),
            status: None,
        });
    }

    if settings.externalizes_debug_info
        && has_unsupported_externalized_debug_info_targets(
            ExternalizedDebugInfoCheck {
                cargo: &cargo,
                metadata: &metadata,
                working_directory: &working_directory,
                cargo_args,
                build_profile,
                rust_target_triples,
                selected_package_name: None,
            },
            config_path,
        )?
    {
        let profile_name = build_profile.cargo_profile_name();
        return Err(CliError::CommandFailed {
            command: format!(
                "{config_path}.enabled requires embedded Cargo debuginfo for the '{profile_name}' profile; disable [profile.{profile_name}].split-debuginfo or an equivalent Cargo/rustc override"
            ),
            status: None,
        });
    }

    Ok(())
}

fn copy_debug_symbol_artifact(source_path: &Path, dest_path: &Path) -> Result<()> {
    if source_path.is_dir() {
        copy_debug_symbol_directory(source_path, dest_path)
    } else {
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        std::fs::copy(source_path, dest_path).map_err(|source| CliError::CopyFailed {
            from: source_path.to_path_buf(),
            to: dest_path.to_path_buf(),
            source,
        })?;

        Ok(())
    }
}

fn copy_debug_symbol_directory(source_path: &Path, dest_path: &Path) -> Result<()> {
    std::fs::create_dir_all(dest_path).map_err(|source| CliError::CreateDirectoryFailed {
        path: dest_path.to_path_buf(),
        source,
    })?;

    for entry in walkdir::WalkDir::new(source_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let relative = entry
            .path()
            .strip_prefix(source_path)
            .expect("walkdir entry should stay under source path");
        if relative.as_os_str().is_empty() {
            continue;
        }

        let dest_entry = dest_path.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest_entry).map_err(|source| {
                CliError::CreateDirectoryFailed {
                    path: dest_entry,
                    source,
                }
            })?;
        } else {
            if let Some(parent) = dest_entry.parent() {
                std::fs::create_dir_all(parent).map_err(|source| {
                    CliError::CreateDirectoryFailed {
                        path: parent.to_path_buf(),
                        source,
                    }
                })?;
            }

            std::fs::copy(entry.path(), &dest_entry).map_err(|source| CliError::CopyFailed {
                from: entry.path().to_path_buf(),
                to: dest_entry,
                source,
            })?;
        }
    }

    Ok(())
}

fn has_unsupported_externalized_debug_info_targets(
    check: ExternalizedDebugInfoCheck<'_>,
    config_path: &str,
) -> Result<bool> {
    if check.rust_target_triples.is_empty() {
        return Ok(false);
    }

    let manifest_path = check
        .metadata
        .workspace_root
        .as_ref()
        .map(|root| root.join("Cargo.toml"))
        .unwrap_or(check.cargo.manifest_path()?);
    let manifest = load_profile_manifest(&manifest_path)?;
    let context = ProfileResolutionContext {
        working_directory: check.working_directory,
        explicit_config_args: extract_cargo_config_args(check.cargo_args),
        cargo_config_files: cargo_config_file_candidates(
            check.cargo_args,
            Some(check.working_directory),
        ),
        manifest: &manifest,
        selected_package_name: check.selected_package_name,
    };
    let profile_name = check.build_profile.cargo_profile_name();
    let base_has_debuginfo = resolve_profile_boolean_setting(
        &context,
        profile_name,
        ProfileBooleanSetting::Debug,
        &mut Vec::new(),
    )?
    .unwrap_or_else(|| default_profile_has_debuginfo(check.build_profile));
    let profile_split_debuginfo =
        resolve_profile_split_debuginfo_setting(&context, profile_name, &mut Vec::new())?;

    Ok(matrix_has_unsupported_externalized_debug_info_targets(
        config_path,
        check
            .rust_target_triples
            .iter()
            .filter_map(|rust_target_triple| {
                resolve_target_externalizes_debug_info(
                    base_has_debuginfo,
                    profile_split_debuginfo,
                    rustflag_profile_overrides(&context, rust_target_triple.as_str()),
                    rust_target_triple,
                )
                .then_some(rust_target_triple.as_str())
            }),
    ))
}

fn supports_externalized_debug_info_target(config_path: &str, rust_target_triple: &str) -> bool {
    rust_target_triple.contains("-apple-")
        && matches!(
            config_path,
            "targets.apple.debug_symbols" | "targets.java.jvm.debug_symbols"
        )
}

fn matrix_has_unsupported_externalized_debug_info_targets<'a>(
    config_path: &str,
    externalized_target_triples: impl IntoIterator<Item = &'a str>,
) -> bool {
    externalized_target_triples
        .into_iter()
        .any(|rust_target_triple| {
            !supports_externalized_debug_info_target(config_path, rust_target_triple)
        })
}

pub(crate) fn ensure_existing_debug_symbol_artifacts_are_usable(
    artifact_paths: &[PathBuf],
    config_path: &str,
) -> Result<()> {
    for artifact_path in artifact_paths {
        if !artifact_has_debug_info_and_symbols(artifact_path)? {
            return Err(CliError::CommandFailed {
                command: format!(
                    "{config_path}.enabled requires unstripped artifacts with embedded debuginfo; '{}' is missing debug info or symbols",
                    artifact_path.display()
                ),
                status: None,
            });
        }
    }

    Ok(())
}

fn build_manifest_entry(artifact: &DebugSymbolArtifact) -> DebugSymbolManifestEntry {
    DebugSymbolManifestEntry {
        path: normalized_archive_path(&artifact.archive_path),
        kind: artifact.kind.as_str(),
        target_triple: artifact.target_triple.clone(),
        platform: artifact.platform.map(Platform::canonical_name),
        architecture: artifact.architecture.map(Architecture::canonical_name),
        abi: artifact.abi.clone(),
        host_target: artifact.host_target.clone(),
    }
}

fn resolve_effective_profile_settings(
    cargo: &Cargo,
    metadata: &crate::cargo::CargoMetadata,
    working_directory: &Path,
    cargo_args: &[String],
    build_profile: &CargoBuildProfile,
    rust_target_triples: &[String],
    selected_package_name: Option<&str>,
) -> Result<EffectiveProfileSettings> {
    let manifest_path = metadata
        .workspace_root
        .as_ref()
        .map(|root| root.join("Cargo.toml"))
        .unwrap_or(cargo.manifest_path()?);
    let manifest = load_profile_manifest(&manifest_path)?;
    let context = ProfileResolutionContext {
        working_directory,
        explicit_config_args: extract_cargo_config_args(cargo_args),
        cargo_config_files: cargo_config_file_candidates(cargo_args, Some(working_directory)),
        manifest: &manifest,
        selected_package_name,
    };
    let profile_name = build_profile.cargo_profile_name();
    let base_has_debuginfo = resolve_profile_boolean_setting(
        &context,
        profile_name,
        ProfileBooleanSetting::Debug,
        &mut Vec::new(),
    )?
    .unwrap_or_else(|| default_profile_has_debuginfo(build_profile));
    let profile_split_debuginfo =
        resolve_profile_split_debuginfo_setting(&context, profile_name, &mut Vec::new())?;
    let base_settings = EffectiveProfileSettings {
        has_debuginfo: base_has_debuginfo,
        strips_symbols: resolve_profile_boolean_setting(
            &context,
            profile_name,
            ProfileBooleanSetting::Strip,
            &mut Vec::new(),
        )?
        .unwrap_or(false),
        externalizes_debug_info: profile_split_debuginfo
            .unwrap_or_else(|| default_profile_externalizes_debug_info(base_has_debuginfo, None)),
    };

    if rust_target_triples.is_empty() {
        return Ok(base_settings);
    }

    Ok(EffectiveProfileSettings {
        has_debuginfo: rust_target_triples.iter().all(|rust_target_triple| {
            rustflag_profile_overrides(&context, rust_target_triple)
                .debug
                .unwrap_or(base_settings.has_debuginfo)
        }),
        strips_symbols: rust_target_triples.iter().any(|rust_target_triple| {
            rustflag_profile_overrides(&context, rust_target_triple)
                .strip
                .unwrap_or(base_settings.strips_symbols)
        }),
        externalizes_debug_info: rust_target_triples.iter().any(|rust_target_triple| {
            resolve_target_externalizes_debug_info(
                base_settings.has_debuginfo,
                profile_split_debuginfo,
                rustflag_profile_overrides(&context, rust_target_triple),
                rust_target_triple,
            )
        }),
    })
}

fn load_profile_manifest(manifest_path: &Path) -> Result<toml::Value> {
    let content =
        std::fs::read_to_string(manifest_path).map_err(|source| CliError::ReadFailed {
            path: manifest_path.to_path_buf(),
            source,
        })?;
    toml::from_str(&content).map_err(|source| CliError::CommandFailed {
        command: format!(
            "parse Cargo profile configuration from {}: {source}",
            manifest_path.display()
        ),
        status: None,
    })
}

fn resolve_profile_boolean_setting(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
    setting: ProfileBooleanSetting,
    visited: &mut Vec<String>,
) -> Result<Option<bool>> {
    if visited
        .iter()
        .any(|visited_name| visited_name == profile_name)
    {
        return Ok(None);
    }

    visited.push(profile_name.to_string());

    let direct_setting =
        profile_boolean_setting_from_explicit_cargo_config_args(context, profile_name, setting)
            .or_else(|| profile_boolean_setting_from_env(profile_name, setting))
            .or_else(|| {
                profile_boolean_setting_from_cargo_config_files(context, profile_name, setting)
            })
            .or_else(|| {
                profile_boolean_setting_from_toml(
                    context.manifest,
                    profile_name,
                    context.selected_package_name,
                    setting,
                )
            });

    let resolved = if direct_setting.is_some() {
        direct_setting
    } else if let Some(inherits) = profile_inherits_from_sources(context, profile_name) {
        resolve_profile_boolean_setting(context, &inherits, setting, visited)?
    } else if profile_name != "release" && profile_name != "dev" {
        resolve_profile_boolean_setting(context, "release", setting, visited)?
    } else {
        default_profile_boolean_setting(context, setting, profile_name)?
    };

    visited.pop();
    Ok(resolved)
}

fn default_profile_boolean_setting(
    context: &ProfileResolutionContext<'_>,
    setting: ProfileBooleanSetting,
    profile_name: &str,
) -> Result<Option<bool>> {
    match setting {
        ProfileBooleanSetting::Debug => Ok(match profile_name {
            "dev" => Some(true),
            "release" => Some(false),
            _ => None,
        }),
        ProfileBooleanSetting::Strip => default_profile_strip_setting(context, profile_name),
    }
}

fn default_profile_strip_setting(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
) -> Result<Option<bool>> {
    match profile_name {
        "dev" => Ok(Some(false)),
        "release" => Ok(Some(
            !resolve_profile_boolean_setting(
                context,
                "release",
                ProfileBooleanSetting::Debug,
                &mut Vec::new(),
            )?
            .unwrap_or(false),
        )),
        _ => Ok(None),
    }
}

fn resolve_profile_split_debuginfo_setting(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
    visited: &mut Vec<String>,
) -> Result<Option<bool>> {
    if visited
        .iter()
        .any(|visited_name| visited_name == profile_name)
    {
        return Ok(None);
    }

    visited.push(profile_name.to_string());

    let direct_setting =
        profile_split_debuginfo_from_explicit_cargo_config_args(context, profile_name)
            .or_else(|| profile_split_debuginfo_from_env(profile_name))
            .or_else(|| profile_split_debuginfo_from_cargo_config_files(context, profile_name))
            .or_else(|| {
                profile_split_debuginfo_from_toml(
                    context.manifest,
                    profile_name,
                    context.selected_package_name,
                )
            });

    let resolved = if direct_setting.is_some() {
        direct_setting
    } else if let Some(inherits) = profile_inherits_from_sources(context, profile_name) {
        resolve_profile_split_debuginfo_setting(context, &inherits, visited)?
    } else if profile_name != "release" && profile_name != "dev" {
        resolve_profile_split_debuginfo_setting(context, "release", visited)?
    } else {
        None
    };

    visited.pop();
    Ok(resolved)
}

fn artifact_has_debug_info_and_symbols(path: &Path) -> Result<bool> {
    let data = std::fs::read(path).map_err(|source| CliError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    inspect_binary_for_debug_info_and_symbols(path, &data)
}

fn inspect_binary_for_debug_info_and_symbols(path: &Path, data: &[u8]) -> Result<bool> {
    use object::read::FileKind;

    match FileKind::parse(data).map_err(|source| CliError::CommandFailed {
        command: format!("parse binary kind for {}: {source}", path.display()),
        status: None,
    })? {
        FileKind::Archive => inspect_archive_members_for_debug_info_and_symbols(path, data),
        _ => inspect_object_file_for_debug_info_and_symbols(path, data)
            .map(|(has_debug_info, has_symbols)| has_debug_info && has_symbols),
    }
}

fn inspect_archive_members_for_debug_info_and_symbols(path: &Path, data: &[u8]) -> Result<bool> {
    let archive = object::read::archive::ArchiveFile::parse(data).map_err(|source| {
        CliError::CommandFailed {
            command: format!("parse archive {}: {source}", path.display()),
            status: None,
        }
    })?;

    let mut has_debug_info = false;
    let mut has_symbols = false;

    for member in archive.members() {
        let member = member.map_err(|source| CliError::CommandFailed {
            command: format!("read archive member from {}: {source}", path.display()),
            status: None,
        })?;
        let member_data = member
            .data(data)
            .map_err(|source| CliError::CommandFailed {
                command: format!("read archive member data from {}: {source}", path.display()),
                status: None,
            })?;
        let Ok(file_kind) = object::read::FileKind::parse(member_data) else {
            continue;
        };
        if matches!(file_kind, object::read::FileKind::Archive) {
            continue;
        }

        let member_result = inspect_object_file_for_debug_info_and_symbols(path, member_data)?;
        has_debug_info |= member_result.0;
        has_symbols |= member_result.1;
        if has_debug_info && has_symbols {
            return Ok(true);
        }
    }

    Ok(false)
}

fn inspect_object_file_for_debug_info_and_symbols(
    path: &Path,
    data: &[u8],
) -> Result<(bool, bool)> {
    let object = object::File::parse(data).map_err(|source| CliError::CommandFailed {
        command: format!("parse object file {}: {source}", path.display()),
        status: None,
    })?;

    let has_debug_info = object.sections().any(|section| {
        section.name().ok().is_some_and(|name| {
            matches!(
                name,
                ".debug_info"
                    | ".debug_line"
                    | ".zdebug_info"
                    | ".zdebug_line"
                    | "__debug_info"
                    | "__debug_line"
                    | "__DWARF"
            )
        })
    });
    let has_symbols = object.symbols().any(|symbol| {
        !symbol.is_undefined()
            && symbol
                .name()
                .ok()
                .is_some_and(|name| !name.is_empty() && name != "__mh_execute_header")
    });

    Ok((has_debug_info, has_symbols))
}

fn profile_package_setting_from_toml<'a>(
    value: &'a toml::Value,
    profile_name: &str,
    package_name: &str,
    key: &str,
) -> Option<&'a toml::Value> {
    value
        .get("profile")?
        .get(profile_name)?
        .get("package")?
        .get(package_name)?
        .get(key)
}

fn profile_package_boolean_setting_from_toml(
    value: &toml::Value,
    profile_name: &str,
    package_name: &str,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    profile_package_setting_from_toml(value, profile_name, package_name, setting.toml_key())
        .and_then(|raw_value| setting.parse_toml_value(raw_value))
}

fn profile_package_split_debuginfo_from_toml(
    value: &toml::Value,
    profile_name: &str,
    package_name: &str,
) -> Option<bool> {
    profile_package_setting_from_toml(value, profile_name, package_name, "split-debuginfo")
        .and_then(parse_split_debuginfo_toml_value)
}

fn profile_package_boolean_setting_from_inline_config(
    config_argument: &str,
    profile_name: &str,
    package_name: &str,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    parse_inline_config_value(
        config_argument,
        &[
            "profile",
            profile_name,
            "package",
            package_name,
            setting.toml_key(),
        ],
    )
    .and_then(|value| setting.parse_toml_value(&value))
}

fn profile_package_split_debuginfo_from_inline_config(
    config_argument: &str,
    profile_name: &str,
    package_name: &str,
) -> Option<bool> {
    parse_inline_config_value(
        config_argument,
        &[
            "profile",
            profile_name,
            "package",
            package_name,
            "split-debuginfo",
        ],
    )
    .and_then(|value| parse_split_debuginfo_toml_value(&value))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RustflagProfileOverrides {
    debug: Option<bool>,
    strip: Option<bool>,
    split_debuginfo: Option<bool>,
}

#[derive(Debug, Default)]
struct CargoTargetCfg {
    flags: HashSet<String>,
    values: HashMap<String, HashSet<String>>,
}

fn rustflag_profile_overrides(
    context: &ProfileResolutionContext<'_>,
    rust_target_triple: &str,
) -> RustflagProfileOverrides {
    parse_rustflag_profile_overrides(&configured_rustflags_for_target(
        context,
        rust_target_triple,
    ))
}

fn configured_rustflags_for_target(
    context: &ProfileResolutionContext<'_>,
    rust_target_triple: &str,
) -> Vec<String> {
    let mut rustflags = Vec::new();

    for config_path in context.cargo_config_files.iter().rev() {
        rustflags.extend(
            parse_build_rustflags_from_config_file(config_path)
                .into_iter()
                .flatten(),
        );
        rustflags.extend(
            parse_target_rustflags_from_config_file(config_path, rust_target_triple)
                .into_iter()
                .flatten(),
        );
    }

    if let Ok(encoded_rustflags) = std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        rustflags.extend(split_encoded_rustflags(&encoded_rustflags));
    } else if let Ok(rustflags_env) = std::env::var("RUSTFLAGS") {
        rustflags.extend(split_shell_words(&rustflags_env));
    }

    if let Ok(build_rustflags) = std::env::var("CARGO_BUILD_RUSTFLAGS") {
        rustflags.extend(split_shell_words(&build_rustflags));
    }

    let cargo_target_rustflags_env = format!(
        "CARGO_TARGET_{}_RUSTFLAGS",
        rust_target_triple.replace('-', "_").to_ascii_uppercase()
    );
    if let Ok(target_rustflags) = std::env::var(&cargo_target_rustflags_env) {
        rustflags.extend(split_shell_words(&target_rustflags));
    }

    for config_arg in &context.explicit_config_args {
        rustflags.extend(
            parse_build_rustflags_from_inline_config(config_arg)
                .into_iter()
                .flatten(),
        );
        rustflags.extend(
            parse_target_rustflags_from_inline_config(config_arg, rust_target_triple)
                .into_iter()
                .flatten(),
        );
    }

    rustflags
}

fn parse_rustflag_profile_overrides(rustflags: &[String]) -> RustflagProfileOverrides {
    let mut overrides = RustflagProfileOverrides::default();
    let mut index = 0;

    while index < rustflags.len() {
        let flag = &rustflags[index];

        if flag == "-C" {
            if let Some(value) = rustflags.get(index + 1) {
                apply_codegen_flag_override(&mut overrides, value);
                index += 2;
                continue;
            }
        } else if let Some(value) = flag.strip_prefix("-C") {
            apply_codegen_flag_override(&mut overrides, value);
            index += 1;
            continue;
        }

        index += 1;
    }

    overrides
}

fn apply_codegen_flag_override(overrides: &mut RustflagProfileOverrides, value: &str) {
    let trimmed = value.trim();

    if let Some(debug_value) = trimmed.strip_prefix("debuginfo=") {
        overrides.debug = parse_rustflag_debuginfo_value(debug_value);
    } else if let Some(strip_value) = trimmed.strip_prefix("strip=") {
        overrides.strip = parse_rustflag_strip_value(strip_value);
    } else if let Some(split_debuginfo_value) = trimmed.strip_prefix("split-debuginfo=") {
        overrides.split_debuginfo = parse_split_debuginfo_value(split_debuginfo_value);
    }
}

fn parse_rustflag_debuginfo_value(value: &str) -> Option<bool> {
    parse_env_debuginfo_value(value)
}

fn parse_rustflag_strip_value(value: &str) -> Option<bool> {
    parse_strip_value(value)
}

fn parse_build_rustflags_from_inline_config(config_arg: &str) -> Option<Vec<String>> {
    config_arg
        .strip_prefix("build.rustflags=")
        .and_then(parse_rustflags_inline_value)
        .or_else(|| {
            parse_inline_config_value(config_arg, &["build", "rustflags"])
                .and_then(|value| parse_rustflags_config_value(&value))
        })
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

    prefixes
        .into_iter()
        .find_map(|prefix| {
            config_arg
                .strip_prefix(&prefix)
                .and_then(parse_rustflags_inline_value)
        })
        .or_else(|| {
            let value: toml::Value = toml::from_str(config_arg).ok()?;
            parse_target_rustflags_from_toml(&value, rust_target_triple)
        })
}

fn parse_rustflags_inline_value(value: &str) -> Option<Vec<String>> {
    let parsed: toml::Value = toml::from_str(&format!("value = {value}")).ok()?;
    parse_rustflags_config_value(parsed.get("value")?)
}

fn parse_build_rustflags_from_config_file(config_path: &Path) -> Option<Vec<String>> {
    let config = load_toml_file(config_path)?;
    parse_rustflags_config_value(config.get("build")?.get("rustflags")?)
}

fn parse_target_rustflags_from_config_file(
    config_path: &Path,
    rust_target_triple: &str,
) -> Option<Vec<String>> {
    let config = load_toml_file(config_path)?;
    parse_target_rustflags_from_toml(&config, rust_target_triple)
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

fn parse_target_rustflags_from_toml(
    value: &toml::Value,
    rust_target_triple: &str,
) -> Option<Vec<String>> {
    let target_table = value.get("target")?.as_table()?;
    let mut rustflags = Vec::new();

    if let Some(values) = target_table
        .get(rust_target_triple)
        .and_then(|value| value.get("rustflags"))
        .and_then(parse_rustflags_config_value)
    {
        rustflags.extend(values);
    }

    let mut target_cfg = None;
    for (key, value) in target_table {
        if !key.starts_with("cfg(") {
            continue;
        }

        let Some(resolved_target_cfg) = target_cfg
            .get_or_insert_with(|| CargoTargetCfg::for_target(rust_target_triple))
            .as_ref()
        else {
            continue;
        };

        if cargo_cfg_key_matches(key, resolved_target_cfg)
            && let Some(values) = value
                .get("rustflags")
                .and_then(parse_rustflags_config_value)
        {
            rustflags.extend(values);
        }
    }

    (!rustflags.is_empty()).then_some(rustflags)
}

fn split_encoded_rustflags(input: &str) -> Vec<String> {
    input
        .split('\u{1f}')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

impl CargoTargetCfg {
    fn for_target(rust_target_triple: &str) -> Option<Self> {
        let output = std::process::Command::new("rustc")
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

fn normalized_archive_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl DebugSymbolArtifactKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static_library",
            Self::Shared => "shared_library",
            Self::Jni => "jni_library",
            Self::DebugInfo => "debug_info",
        }
    }
}

#[cfg(test)]
fn profile_has_debuginfo(
    cargo: &Cargo,
    metadata: &crate::cargo::CargoMetadata,
    working_directory: &Path,
    cargo_args: &[String],
    build_profile: &CargoBuildProfile,
    rust_target_triples: &[String],
) -> Result<bool> {
    Ok(resolve_effective_profile_settings(
        cargo,
        metadata,
        working_directory,
        cargo_args,
        build_profile,
        rust_target_triples,
        None,
    )?
    .has_debuginfo)
}

#[cfg(test)]
fn profile_strips_symbols(
    cargo: &Cargo,
    metadata: &crate::cargo::CargoMetadata,
    working_directory: &Path,
    cargo_args: &[String],
    build_profile: &CargoBuildProfile,
    rust_target_triples: &[String],
) -> Result<bool> {
    Ok(resolve_effective_profile_settings(
        cargo,
        metadata,
        working_directory,
        cargo_args,
        build_profile,
        rust_target_triples,
        None,
    )?
    .strips_symbols)
}

#[cfg(test)]
fn profile_externalizes_debug_info(
    cargo: &Cargo,
    metadata: &crate::cargo::CargoMetadata,
    working_directory: &Path,
    cargo_args: &[String],
    build_profile: &CargoBuildProfile,
    rust_target_triples: &[String],
) -> Result<bool> {
    Ok(resolve_effective_profile_settings(
        cargo,
        metadata,
        working_directory,
        cargo_args,
        build_profile,
        rust_target_triples,
        None,
    )?
    .externalizes_debug_info)
}

fn profile_boolean_setting_from_env(
    profile_name: &str,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    let env_name = format!(
        "CARGO_PROFILE_{}_{}",
        profile_name.replace('-', "_").to_ascii_uppercase(),
        setting.env_suffix()
    );
    std::env::var(&env_name)
        .ok()
        .and_then(|value| setting.parse_env_value(&value))
}

fn profile_split_debuginfo_from_env(profile_name: &str) -> Option<bool> {
    let env_name = format!(
        "CARGO_PROFILE_{}_SPLIT_DEBUGINFO",
        profile_name.replace('-', "_").to_ascii_uppercase()
    );
    std::env::var(&env_name)
        .ok()
        .and_then(|value| parse_split_debuginfo_value(&value))
}

fn parse_env_debuginfo_value(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();

    match normalized.as_str() {
        "true" | "1" | "2" | "limited" | "line-tables-only" | "line-directives-only" | "full" => {
            Some(true)
        }
        "false" | "0" | "none" => Some(false),
        _ => None,
    }
}

fn parse_env_strip_value(value: &str) -> Option<bool> {
    parse_strip_value(value)
}

fn profile_boolean_setting_from_explicit_cargo_config_args(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    context
        .explicit_config_args
        .iter()
        .rev()
        .find_map(|config_argument| {
            context
                .selected_package_name
                .and_then(|package_name| {
                    profile_package_boolean_setting_from_inline_config(
                        config_argument,
                        profile_name,
                        package_name,
                        setting,
                    )
                })
                .or_else(|| setting.parse_inline_value(config_argument, profile_name))
                .or_else(|| {
                    profile_boolean_setting_from_config_file(
                        &resolve_cargo_config_path(
                            config_argument,
                            Some(context.working_directory),
                        ),
                        profile_name,
                        context.selected_package_name,
                        setting,
                    )
                })
        })
}

#[cfg(test)]
fn profile_debug_from_explicit_cargo_config_args(
    working_directory: &Path,
    cargo_args: &[String],
    profile_name: &str,
) -> Option<bool> {
    let manifest = toml::Value::Table(Default::default());
    let context = ProfileResolutionContext {
        working_directory,
        explicit_config_args: extract_cargo_config_args(cargo_args),
        cargo_config_files: Vec::new(),
        manifest: &manifest,
        selected_package_name: None,
    };
    profile_boolean_setting_from_explicit_cargo_config_args(
        &context,
        profile_name,
        ProfileBooleanSetting::Debug,
    )
}

fn profile_boolean_setting_from_cargo_config_files(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    context.cargo_config_files.iter().find_map(|config_path| {
        profile_boolean_setting_from_config_file(
            config_path,
            profile_name,
            context.selected_package_name,
            setting,
        )
    })
}

fn profile_split_debuginfo_from_explicit_cargo_config_args(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
) -> Option<bool> {
    context
        .explicit_config_args
        .iter()
        .rev()
        .find_map(|config_argument| {
            context
                .selected_package_name
                .and_then(|package_name| {
                    profile_package_split_debuginfo_from_inline_config(
                        config_argument,
                        profile_name,
                        package_name,
                    )
                })
                .or_else(|| {
                    parse_profile_split_debuginfo_from_inline_config(config_argument, profile_name)
                })
                .or_else(|| {
                    profile_split_debuginfo_from_config_file(
                        &resolve_cargo_config_path(
                            config_argument,
                            Some(context.working_directory),
                        ),
                        profile_name,
                        context.selected_package_name,
                    )
                })
        })
}

fn profile_split_debuginfo_from_cargo_config_files(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
) -> Option<bool> {
    context.cargo_config_files.iter().find_map(|config_path| {
        profile_split_debuginfo_from_config_file(
            config_path,
            profile_name,
            context.selected_package_name,
        )
    })
}

fn profile_split_debuginfo_from_config_file(
    config_path: &Path,
    profile_name: &str,
    selected_package_name: Option<&str>,
) -> Option<bool> {
    let config = load_toml_file(config_path)?;
    profile_split_debuginfo_from_toml(&config, profile_name, selected_package_name)
}

fn profile_boolean_setting_from_config_file(
    config_path: &Path,
    profile_name: &str,
    selected_package_name: Option<&str>,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    let config = load_toml_file(config_path)?;
    profile_boolean_setting_from_toml(&config, profile_name, selected_package_name, setting)
}

fn profile_boolean_setting_from_toml(
    value: &toml::Value,
    profile_name: &str,
    selected_package_name: Option<&str>,
    setting: ProfileBooleanSetting,
) -> Option<bool> {
    selected_package_name
        .and_then(|package_name| {
            profile_package_boolean_setting_from_toml(value, profile_name, package_name, setting)
        })
        .or_else(|| {
            profile_toml_value(value, profile_name)
                .and_then(|profile| profile.get(setting.toml_key()))
                .and_then(|raw_value| setting.parse_toml_value(raw_value))
        })
}

fn profile_split_debuginfo_from_toml(
    value: &toml::Value,
    profile_name: &str,
    selected_package_name: Option<&str>,
) -> Option<bool> {
    selected_package_name
        .and_then(|package_name| {
            profile_package_split_debuginfo_from_toml(value, profile_name, package_name)
        })
        .or_else(|| {
            profile_toml_value(value, profile_name)
                .and_then(|profile| profile.get("split-debuginfo"))
                .and_then(parse_split_debuginfo_toml_value)
        })
}

fn profile_inherits_from_sources(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
) -> Option<String> {
    profile_inherits_from_explicit_cargo_config_args(context, profile_name)
        .or_else(|| profile_inherits_from_cargo_config_files(context, profile_name))
        .or_else(|| profile_inherits_from_toml(context.manifest, profile_name))
}

fn profile_inherits_from_explicit_cargo_config_args(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
) -> Option<String> {
    context
        .explicit_config_args
        .iter()
        .rev()
        .find_map(|config_argument| {
            parse_profile_inherits_from_inline_config(config_argument, profile_name).or_else(|| {
                profile_inherits_from_config_file(
                    &resolve_cargo_config_path(config_argument, Some(context.working_directory)),
                    profile_name,
                )
            })
        })
}

fn profile_inherits_from_cargo_config_files(
    context: &ProfileResolutionContext<'_>,
    profile_name: &str,
) -> Option<String> {
    context
        .cargo_config_files
        .iter()
        .find_map(|config_path| profile_inherits_from_config_file(config_path, profile_name))
}

fn profile_inherits_from_config_file(config_path: &Path, profile_name: &str) -> Option<String> {
    let config = load_toml_file(config_path)?;
    profile_inherits_from_toml(&config, profile_name)
}

fn profile_inherits_from_toml(value: &toml::Value, profile_name: &str) -> Option<String> {
    profile_toml_value(value, profile_name)?
        .get("inherits")?
        .as_str()
        .map(str::trim)
        .filter(|inherits| !inherits.is_empty())
        .map(str::to_string)
}

fn profile_toml_value<'a>(value: &'a toml::Value, profile_name: &str) -> Option<&'a toml::Value> {
    value
        .get("profile")
        .and_then(|profiles| profiles.get(profile_name))
}

fn load_toml_file(path: &Path) -> Option<toml::Value> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn parse_profile_inherits_from_inline_config(
    config_argument: &str,
    profile_name: &str,
) -> Option<String> {
    let prefix = format!("profile.{profile_name}.inherits=");
    config_argument
        .strip_prefix(&prefix)
        .map(trim_wrapping_quotes)
        .map(str::trim)
        .filter(|inherits| !inherits.is_empty())
        .map(str::to_string)
        .or_else(|| {
            parse_inline_config_value(config_argument, &["profile", profile_name, "inherits"])
                .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
                .filter(|inherits| !inherits.is_empty())
        })
}

fn parse_profile_strip_from_inline_config(
    config_argument: &str,
    profile_name: &str,
) -> Option<bool> {
    let prefix = format!("profile.{profile_name}.strip=");
    config_argument
        .strip_prefix(&prefix)
        .and_then(parse_strip_value)
        .or_else(|| {
            parse_inline_config_value(config_argument, &["profile", profile_name, "strip"])
                .and_then(|value| parse_strip_toml_value(&value))
        })
}

fn parse_profile_split_debuginfo_from_inline_config(
    config_argument: &str,
    profile_name: &str,
) -> Option<bool> {
    let prefix = format!("profile.{profile_name}.split-debuginfo=");
    config_argument
        .strip_prefix(&prefix)
        .and_then(parse_split_debuginfo_value)
        .or_else(|| {
            parse_inline_config_value(
                config_argument,
                &["profile", profile_name, "split-debuginfo"],
            )
            .and_then(|value| parse_split_debuginfo_toml_value(&value))
        })
}

fn parse_strip_toml_value(value: &toml::Value) -> Option<bool> {
    match value {
        toml::Value::Boolean(enabled) => Some(*enabled),
        toml::Value::String(mode) => parse_strip_value(mode),
        _ => None,
    }
}

fn parse_split_debuginfo_toml_value(value: &toml::Value) -> Option<bool> {
    match value {
        toml::Value::String(mode) => parse_split_debuginfo_value(mode),
        _ => None,
    }
}

fn parse_strip_value(value: &str) -> Option<bool> {
    let normalized = trim_wrapping_quotes(value).trim().to_ascii_lowercase();

    match normalized.as_str() {
        "false" | "0" | "none" => Some(false),
        "true" | "1" | "symbols" | "debuginfo" => Some(true),
        _ => None,
    }
}

fn parse_split_debuginfo_value(value: &str) -> Option<bool> {
    let normalized = trim_wrapping_quotes(value).trim().to_ascii_lowercase();

    match normalized.as_str() {
        "off" => Some(false),
        "packed" | "unpacked" => Some(true),
        _ => None,
    }
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
fn profile_debug_from_manifest(manifest_path: &Path, profile_name: &str) -> Result<Option<bool>> {
    let manifest = load_profile_manifest(manifest_path)?;

    Ok(resolve_profile_debug_from_manifest_value(
        &manifest,
        profile_name,
        &mut Vec::new(),
    ))
}

#[cfg(test)]
fn resolve_profile_debug_from_manifest_value(
    manifest: &toml::Value,
    profile_name: &str,
    visited: &mut Vec<String>,
) -> Option<bool> {
    if visited
        .iter()
        .any(|visited_name| visited_name == profile_name)
    {
        return None;
    }
    visited.push(profile_name.to_string());

    let resolved = manifest
        .get("profile")
        .and_then(|profiles| profiles.get(profile_name))
        .and_then(|profile| {
            profile
                .get("debug")
                .and_then(parse_debuginfo_toml_value)
                .or_else(|| {
                    profile
                        .get("inherits")
                        .and_then(toml::Value::as_str)
                        .and_then(|inherits| {
                            resolve_profile_debug_from_manifest_value(manifest, inherits, visited)
                        })
                })
        })
        .or_else(|| {
            (profile_name != "release" && profile_name != "dev")
                .then(|| resolve_profile_debug_from_manifest_value(manifest, "release", visited))
                .flatten()
        });

    visited.pop();
    resolved
}

fn default_profile_has_debuginfo(build_profile: &CargoBuildProfile) -> bool {
    matches!(build_profile, CargoBuildProfile::Debug)
}

fn default_profile_externalizes_debug_info(
    has_debuginfo: bool,
    rust_target_triple: Option<&str>,
) -> bool {
    default_profile_externalizes_debug_info_for_target(
        has_debuginfo,
        rust_target_triple,
        cfg!(target_os = "macos"),
    )
}

fn default_profile_externalizes_debug_info_for_target(
    has_debuginfo: bool,
    rust_target_triple: Option<&str>,
    is_macos_host: bool,
) -> bool {
    is_macos_host
        && has_debuginfo
        && rust_target_triple.is_some_and(|target| target.contains("-apple-"))
}

fn resolve_target_externalizes_debug_info(
    base_has_debuginfo: bool,
    profile_split_debuginfo: Option<bool>,
    overrides: RustflagProfileOverrides,
    rust_target_triple: &str,
) -> bool {
    resolve_target_externalizes_debug_info_for_host(
        base_has_debuginfo,
        profile_split_debuginfo,
        overrides,
        rust_target_triple,
        cfg!(target_os = "macos"),
    )
}

fn resolve_target_externalizes_debug_info_for_host(
    base_has_debuginfo: bool,
    profile_split_debuginfo: Option<bool>,
    overrides: RustflagProfileOverrides,
    rust_target_triple: &str,
    is_macos_host: bool,
) -> bool {
    let target_has_debuginfo = overrides.debug.unwrap_or(base_has_debuginfo);

    overrides
        .split_debuginfo
        .or(profile_split_debuginfo)
        .unwrap_or_else(|| {
            default_profile_externalizes_debug_info_for_target(
                target_has_debuginfo,
                Some(rust_target_triple),
                is_macos_host,
            )
        })
}

fn create_zip(source_dir: &Path, zip_path: &Path) -> Result<()> {
    let file = std::fs::File::create(zip_path).map_err(|source| CliError::WriteFailed {
        path: zip_path.to_path_buf(),
        source,
    })?;

    let mut zip_writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .try_for_each(|entry| {
            let relative = entry
                .path()
                .strip_prefix(source_dir.parent().unwrap())
                .unwrap();
            let path_string = relative.to_string_lossy().replace('\\', "/");

            if entry.file_type().is_dir() {
                zip_writer
                    .add_directory(path_string, options)
                    .map_err(|_| PackError::ZipFailed {
                        source: std::io::Error::other("zip dir failed"),
                    })?;
            } else {
                zip_writer
                    .start_file(path_string, options)
                    .map_err(|_| PackError::ZipFailed {
                        source: std::io::Error::other("zip start failed"),
                    })?;

                let content =
                    std::fs::read(entry.path()).map_err(|source| CliError::ReadFailed {
                        path: entry.path().to_path_buf(),
                        source,
                    })?;

                std::io::Write::write_all(&mut zip_writer, &content)
                    .map_err(|source| PackError::ZipFailed { source })?;
            }

            Ok::<_, CliError>(())
        })?;

    zip_writer.finish().map_err(|_| PackError::ZipFailed {
        source: std::io::Error::other("zip finish failed"),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        DebugSymbolArtifact, DebugSymbolArtifactKind, RustflagProfileOverrides,
        default_profile_externalizes_debug_info_for_target, default_profile_has_debuginfo,
        matrix_has_unsupported_externalized_debug_info_targets,
        profile_debug_from_explicit_cargo_config_args, profile_debug_from_manifest,
        profile_externalizes_debug_info, profile_has_debuginfo, profile_strips_symbols,
        resolve_profile_debug_from_manifest_value, resolve_target_externalizes_debug_info_for_host,
        write_debug_symbols_zip,
    };
    use crate::build::CargoBuildProfile;
    use crate::cargo::{Cargo, fixture::CargoMetadataFixture};
    use crate::target::{Architecture, Platform};

    static ENV_TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct EnvVarGuard {
        name: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(name);
            unsafe {
                std::env::set_var(name, value);
            }
            Self { name, previous }
        }

        fn unset(name: &'static str) -> Self {
            let previous = std::env::var_os(name);
            unsafe {
                std::env::remove_var(name);
            }
            Self { name, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(value) => unsafe {
                    std::env::set_var(self.name, value);
                },
                None => unsafe {
                    std::env::remove_var(self.name);
                },
            }
        }
    }

    fn temporary_directory(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    #[test]
    fn writes_debug_symbols_archive_with_manifest_and_payloads() {
        let temp_root = temporary_directory("boltffi-debug-symbols");
        let output_dir = temp_root.join("symbols");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let source_path = temp_root.join("libdemo.a");
        fs::write(&source_path, b"demo").expect("write source artifact");

        let archive_path = write_debug_symbols_zip(
            &output_dir,
            "demo.apple.symbols.zip",
            "apple",
            "unstripped",
            &[DebugSymbolArtifact {
                source_path: source_path.clone(),
                archive_path: PathBuf::from("ios/aarch64-apple-ios/libdemo.a"),
                kind: DebugSymbolArtifactKind::Static,
                target_triple: Some("aarch64-apple-ios".to_string()),
                platform: Some(Platform::Ios),
                architecture: Some(Architecture::Arm64),
                abi: None,
                host_target: None,
            }],
        )
        .expect("write debug symbols zip");

        let archive_file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(archive_file).expect("read zip archive");
        let bundle_root = "demo.apple.symbols";

        let mut manifest = String::new();
        std::io::Read::read_to_string(
            &mut archive
                .by_name(&format!("{bundle_root}/symbols.json"))
                .expect("manifest entry"),
            &mut manifest,
        )
        .expect("read manifest");
        assert!(manifest.contains("\"platform\": \"apple\""));
        assert!(manifest.contains("\"target_triple\": \"aarch64-apple-ios\""));

        let mut payload = Vec::new();
        std::io::Read::read_to_end(
            &mut archive
                .by_name(&format!("{bundle_root}/ios/aarch64-apple-ios/libdemo.a"))
                .expect("payload entry"),
            &mut payload,
        )
        .expect("read payload");
        assert_eq!(payload, b"demo");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn writes_debug_symbols_archive_with_directory_sidecars() {
        let temp_root = temporary_directory("boltffi-debug-symbols-dir");
        let output_dir = temp_root.join("symbols");
        let dsym_dir = temp_root.join("libdemo.dylib.dSYM");
        let dwarfs_dir = dsym_dir.join("Contents").join("Resources").join("DWARF");
        fs::create_dir_all(&dwarfs_dir).expect("create dsym dir");
        fs::write(dsym_dir.join("Contents").join("Info.plist"), "<plist />").expect("write plist");
        fs::write(dwarfs_dir.join("libdemo.dylib"), b"debug").expect("write dwarf payload");

        let archive_path = write_debug_symbols_zip(
            &output_dir,
            "demo.jvm.symbols.zip",
            "java-jvm",
            "unstripped",
            &[DebugSymbolArtifact {
                source_path: dsym_dir.clone(),
                archive_path: PathBuf::from("native/darwin-arm64/libdemo.dylib.dSYM"),
                kind: DebugSymbolArtifactKind::DebugInfo,
                target_triple: None,
                platform: None,
                architecture: None,
                abi: None,
                host_target: Some("darwin-arm64".to_string()),
            }],
        )
        .expect("write debug symbols zip");

        let archive_file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(archive_file).expect("read zip archive");
        let bundle_root = "demo.jvm.symbols";

        archive
            .by_name(&format!(
                "{bundle_root}/native/darwin-arm64/libdemo.dylib.dSYM/Contents/Info.plist"
            ))
            .expect("dsym plist entry");
        archive
            .by_name(&format!(
                "{bundle_root}/native/darwin-arm64/libdemo.dylib.dSYM/Contents/Resources/DWARF/libdemo.dylib"
            ))
            .expect("dsym dwarf entry");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn custom_release_like_profile_inherits_release_debug_setting_from_manifest() {
        let manifest: toml::Value = toml::from_str(
            r#"
[profile.release]
debug = true

[profile.mobile-release]
inherits = "release"
"#,
        )
        .expect("parse manifest");

        let resolved =
            resolve_profile_debug_from_manifest_value(&manifest, "mobile-release", &mut Vec::new());

        assert_eq!(resolved, Some(true));
    }

    #[test]
    fn named_profile_defaults_to_release_without_explicit_debug_setting() {
        let manifest: toml::Value = toml::from_str(
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("parse manifest");

        let resolved =
            resolve_profile_debug_from_manifest_value(&manifest, "mobile-release", &mut Vec::new());

        assert_eq!(resolved, None);
        assert!(!default_profile_has_debuginfo(&CargoBuildProfile::Named(
            "mobile-release".to_string()
        )));
    }

    #[test]
    fn macos_default_split_debuginfo_only_externalizes_apple_targets() {
        assert!(default_profile_externalizes_debug_info_for_target(
            true,
            Some("aarch64-apple-ios"),
            true
        ));
        assert!(!default_profile_externalizes_debug_info_for_target(
            true,
            Some("aarch64-linux-android"),
            true
        ));
        assert!(!default_profile_externalizes_debug_info_for_target(
            false,
            Some("aarch64-apple-ios"),
            true
        ));
        assert!(!default_profile_externalizes_debug_info_for_target(
            true,
            Some("aarch64-apple-ios"),
            false
        ));
        assert!(!default_profile_externalizes_debug_info_for_target(
            true, None, true
        ));
    }

    #[test]
    fn target_split_debuginfo_fallback_uses_effective_target_debuginfo() {
        assert!(resolve_target_externalizes_debug_info_for_host(
            false,
            None,
            RustflagProfileOverrides {
                debug: Some(true),
                strip: None,
                split_debuginfo: None,
            },
            "aarch64-apple-ios",
            true,
        ));
    }

    #[test]
    fn mixed_jvm_matrix_allows_supported_externalized_darwin_targets() {
        assert!(!matrix_has_unsupported_externalized_debug_info_targets(
            "targets.java.jvm.debug_symbols",
            ["aarch64-apple-darwin"],
        ));
        assert!(matrix_has_unsupported_externalized_debug_info_targets(
            "targets.java.jvm.debug_symbols",
            ["x86_64-unknown-linux-gnu"],
        ));
    }

    #[test]
    fn reads_profile_debug_override_from_cargo_config_argument() {
        let temp_root = temporary_directory("boltffi-debug-symbol-config");
        fs::create_dir_all(&temp_root).expect("create temp root");

        let enabled = profile_debug_from_explicit_cargo_config_args(
            &temp_root,
            &[
                String::from("--config"),
                String::from("profile.release.debug=true"),
            ],
            "release",
        );

        assert_eq!(enabled, Some(true));
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn reads_spaced_profile_strip_override_from_cargo_config_argument() {
        let temp_root = temporary_directory("boltffi-debug-symbol-strip-config");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");

        let cargo_args = vec![
            String::from("--config"),
            String::from("profile.release.strip = 'debuginfo'"),
        ];
        let cargo = Cargo::in_working_directory(temp_root.clone(), &cargo_args);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();

        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &cargo_args,
            &CargoBuildProfile::Release,
            &[],
        )
        .expect("resolve spaced strip override");

        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn explicit_cargo_config_debug_override_beats_profile_env_override() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let temp_root = temporary_directory("boltffi-debug-symbol-precedence");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");

        let cargo_args = vec![
            String::from("--config"),
            String::from("profile.mobile-release.debug=true"),
        ];
        let cargo = Cargo::in_working_directory(temp_root.clone(), &cargo_args);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::set("CARGO_PROFILE_MOBILE_RELEASE_DEBUG", "0");

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &cargo_args,
            &CargoBuildProfile::Named("mobile-release".to_string()),
            &[],
        )
        .expect("resolve profile debug precedence");

        assert!(enabled);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn config_release_debug_applies_to_manifest_inherited_profile() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let profile_name = "boltffi-debug-symbols-cross-source";
        let temp_root = temporary_directory("boltffi-debug-symbol-inherited-config");
        let cargo_config_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_config_dir).expect("create cargo config dir");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.boltffi-debug-symbols-cross-source]
inherits = "release"
"#,
        )
        .expect("write manifest");
        fs::write(
            cargo_config_dir.join("config.toml"),
            r#"
[profile.release]
debug = true
"#,
        )
        .expect("write cargo config");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::unset("CARGO_PROFILE_RELEASE_DEBUG");

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Named(profile_name.to_string()),
            &[],
        )
        .expect("resolve inherited release debug");

        assert!(enabled);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn package_profile_override_does_not_apply_to_whole_artifact_validation() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let temp_root = temporary_directory("boltffi-debug-symbol-package-profile");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "workspace-root"
version = "0.1.0"

[profile.release]
debug = false

[profile.release.package.myffi]
debug = true
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::unset("CARGO_PROFILE_RELEASE_DEBUG");

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[],
        )
        .expect("resolve whole-artifact debuginfo");
        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[],
        )
        .expect("resolve whole-artifact strip");

        assert!(!enabled);
        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn manifest_profile_inheriting_dev_keeps_default_debuginfo() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let profile_name = "boltffi-fast-debug";
        let temp_root = temporary_directory("boltffi-debug-symbol-dev-inherit");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.boltffi-fast-debug]
inherits = "dev"
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::unset("CARGO_PROFILE_DEV_DEBUG");

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Named(profile_name.to_string()),
            &[],
        )
        .expect("resolve inherited dev debug");

        assert!(enabled);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn customized_dev_profile_can_disable_debuginfo_and_strip_symbols() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let temp_root = temporary_directory("boltffi-debug-symbol-dev-profile");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.dev]
debug = false
strip = "debuginfo"
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _debug_env_guard = EnvVarGuard::unset("CARGO_PROFILE_DEV_DEBUG");
        let _strip_env_guard = EnvVarGuard::unset("CARGO_PROFILE_DEV_STRIP");

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Debug,
            &[],
        )
        .expect("resolve customized dev debuginfo");
        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Debug,
            &[],
        )
        .expect("resolve customized dev strip");

        assert!(!enabled);
        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rustflags_split_debuginfo_marks_debug_info_as_externalized() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-rustflags-split-debuginfo");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = true
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::set("RUSTFLAGS", "-C split-debuginfo=packed");

        let externalized = profile_externalizes_debug_info(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve rustflags split debuginfo");

        assert!(externalized);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rustflags_enable_debuginfo_for_target_validation() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-rustflags-debug");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::set("RUSTFLAGS", "-C debuginfo=2");

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve rustflags debuginfo");

        assert!(enabled);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn release_defaults_to_strip_when_only_rustflags_enable_debuginfo() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-rustflags-release-strip");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::set("RUSTFLAGS", "-C debuginfo=2");

        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve release strip with rustflags debuginfo");

        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn target_rustflags_strip_override_marks_symbols_as_stripped() {
        let _env_lock = ENV_TEST_MUTEX.lock().expect("lock env test mutex");
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-rustflags-strip");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = true
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();
        let _env_guard = EnvVarGuard::set(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "-C strip=debuginfo",
        );

        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve target rustflags strip");

        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn local_cargo_config_rustflags_override_parent_config() {
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-config-precedence");
        let workspace_dir = temp_root.join("workspace");
        let member_dir = workspace_dir.join("member");
        fs::create_dir_all(member_dir.join(".cargo")).expect("create member cargo config dir");
        fs::create_dir_all(workspace_dir.join(".cargo"))
            .expect("create workspace cargo config dir");
        let manifest_path = member_dir.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = true
"#,
        )
        .expect("write manifest");
        fs::write(
            workspace_dir.join(".cargo").join("config.toml"),
            r#"
[build]
rustflags = ["-C", "strip=debuginfo"]
"#,
        )
        .expect("write parent cargo config");
        fs::write(
            member_dir.join(".cargo").join("config.toml"),
            r#"
[build]
rustflags = ["-C", "strip=none"]
"#,
        )
        .expect("write local cargo config");

        let cargo = Cargo::in_working_directory(member_dir.clone(), &[]);
        let metadata = CargoMetadataFixture::new(member_dir.join("target"))
            .workspace_root(&member_dir)
            .metadata();

        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &member_dir,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve layered cargo config rustflags");

        assert!(!stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn cfg_target_rustflags_enable_debuginfo_for_validation() {
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-cfg-rustflags");
        let cargo_config_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_config_dir).expect("create cargo config dir");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");
        fs::write(
            cargo_config_dir.join("config.toml"),
            r#"
[target.'cfg(all(unix, target_os = "linux", target_env = "gnu"))']
rustflags = ["-C", "debuginfo=1"]
"#,
        )
        .expect("write cargo config");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve cfg target rustflags");

        assert!(enabled);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn combines_exact_and_cfg_target_rustflags_for_validation() {
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-combined-target-rustflags");
        let cargo_config_dir = temp_root.join(".cargo");
        fs::create_dir_all(&cargo_config_dir).expect("create cargo config dir");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");
        fs::write(
            cargo_config_dir.join("config.toml"),
            r#"
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "debuginfo=2"]

[target.'cfg(unix)']
rustflags = ["-C", "strip=debuginfo"]
"#,
        )
        .expect("write cargo config");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve combined target debuginfo");
        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve combined target strip");

        assert!(enabled);
        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn spaced_target_rustflags_config_marks_symbols_as_stripped() {
        let rust_target_triple = "x86_64-unknown-linux-gnu";
        let temp_root = temporary_directory("boltffi-debug-symbol-inline-rustflags-strip");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = true
"#,
        )
        .expect("write manifest");

        let cargo_args = vec![
            String::from("--config"),
            format!("target.'{rust_target_triple}'.rustflags = ['-C', 'strip=debuginfo']"),
        ];
        let cargo = Cargo::in_working_directory(temp_root.clone(), &cargo_args);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();

        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &cargo_args,
            &CargoBuildProfile::Release,
            &[rust_target_triple.to_string()],
        )
        .expect("resolve spaced target rustflags strip");

        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn uses_manifest_relative_cargo_config_when_manifest_path_is_outside_cwd() {
        let temp_root = temporary_directory("boltffi-debug-symbol-cwd-config");
        let workspace_dir = temp_root.join("workspace").join("member");
        let cargo_config_dir = workspace_dir.join(".cargo");
        fs::create_dir_all(&cargo_config_dir).expect("create cargo config dir");
        let manifest_path = workspace_dir.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .expect("write manifest");
        fs::write(
            cargo_config_dir.join("config.toml"),
            r#"
[profile.release]
debug = true
"#,
        )
        .expect("write cargo config");

        let cargo_args = vec![
            "--manifest-path".to_string(),
            "workspace/member/Cargo.toml".to_string(),
        ];
        let cargo = Cargo::in_working_directory(temp_root.clone(), &cargo_args);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&workspace_dir)
            .metadata();

        let enabled = profile_has_debuginfo(
            &cargo,
            &metadata,
            &temp_root,
            &cargo_args,
            &CargoBuildProfile::Release,
            &[],
        )
        .expect("resolve cwd config precedence");

        assert!(enabled);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn detects_stripped_release_profiles() {
        let temp_root = temporary_directory("boltffi-debug-symbol-strip");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = true
strip = "debuginfo"
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();

        let stripped = profile_strips_symbols(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[],
        )
        .expect("resolve strip setting");

        assert!(stripped);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn detects_profile_split_debuginfo_as_external_debug_info() {
        let temp_root = temporary_directory("boltffi-debug-symbol-split-debuginfo");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = true
split-debuginfo = "unpacked"
"#,
        )
        .expect("write manifest");

        let cargo = Cargo::in_working_directory(temp_root.clone(), &[]);
        let metadata = CargoMetadataFixture::new(temp_root.join("target"))
            .workspace_root(&temp_root)
            .metadata();

        let externalized = profile_externalizes_debug_info(
            &cargo,
            &metadata,
            &temp_root,
            &[],
            &CargoBuildProfile::Release,
            &[],
        )
        .expect("resolve profile split debuginfo");

        assert!(externalized);
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn reads_profile_debug_from_manifest_file() {
        let temp_root = temporary_directory("boltffi-debug-symbol-manifest");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let manifest_path = temp_root.join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"
[package]
name = "demo"
version = "0.1.0"

[profile.release]
debug = "line-tables-only"
"#,
        )
        .expect("write manifest");

        let enabled =
            profile_debug_from_manifest(&manifest_path, "release").expect("read profile debug");

        assert_eq!(enabled, Some(true));
        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }
}
