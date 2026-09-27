use std::path::{Component, Path, PathBuf};

pub(crate) fn configured_build_target(
    cargo_args: &[String],
    working_directory: Option<&Path>,
) -> Option<String> {
    extract_cargo_config_args(cargo_args)
        .into_iter()
        .rev()
        .find_map(|config_argument| {
            parse_build_target_from_inline_config(&config_argument).or_else(|| {
                parse_build_target_from_config_file(&resolve_cargo_config_path(
                    &config_argument,
                    working_directory,
                ))
            })
        })
        .or_else(|| {
            std::env::var("CARGO_BUILD_TARGET")
                .ok()
                .map(|target| target.trim().to_string())
                .filter(|target| !target.is_empty())
        })
        .or_else(|| {
            cargo_config_file_candidates(cargo_args, working_directory)
                .into_iter()
                .find_map(|config_path| parse_build_target_from_config_file(&config_path))
        })
}

pub(crate) fn extract_cargo_config_args(cargo_args: &[String]) -> Vec<String> {
    let mut config_arguments = Vec::new();
    let mut index = 0;

    while index < cargo_args.len() {
        let argument = &cargo_args[index];

        if let Some(value) = argument.strip_prefix("--config=") {
            config_arguments.push(value.to_string());
        } else if argument == "--config"
            && let Some(value) = cargo_args.get(index + 1)
        {
            config_arguments.push(value.clone());
            index += 1;
        }

        index += 1;
    }

    config_arguments
}

pub(crate) fn resolve_cargo_config_path(
    config_argument: &str,
    working_directory: Option<&Path>,
) -> PathBuf {
    let config_path = PathBuf::from(config_argument);

    if config_path.is_absolute() {
        return config_path;
    }

    working_directory
        .map(|working_directory| working_directory.join(&config_path))
        .unwrap_or(config_path)
}

pub(crate) fn cargo_config_file_candidates(
    cargo_args: &[String],
    working_directory: Option<&Path>,
) -> Vec<PathBuf> {
    cargo_config_file_candidates_with_inputs(
        cargo_config_search_roots(cargo_args, working_directory),
        working_directory.map(Path::to_path_buf),
        std::env::var_os("CARGO_HOME").map(PathBuf::from),
        cargo_home_fallback_home_dir(),
    )
}

pub(crate) fn cargo_config_file_candidates_with_inputs(
    search_roots: Vec<PathBuf>,
    working_directory: Option<PathBuf>,
    cargo_home: Option<PathBuf>,
    home_directory: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    search_roots.into_iter().for_each(|search_root| {
        let mut current_root = search_root;

        loop {
            let new_candidates = [".cargo/config.toml", ".cargo/config"]
                .into_iter()
                .map(|config_name| current_root.join(config_name))
                .filter(|config_path| config_path.exists() && !candidates.contains(config_path))
                .collect::<Vec<_>>();
            candidates.extend(new_candidates);

            if !current_root.pop() {
                break;
            }
        }
    });

    let resolved_cargo_home = cargo_home.map(|cargo_home| {
        if cargo_home.is_absolute() {
            cargo_home
        } else if let Some(working_directory) = working_directory.as_ref() {
            working_directory.join(cargo_home)
        } else {
            cargo_home
        }
    });

    resolved_cargo_home
        .or_else(|| home_directory.map(|home_directory| home_directory.join(".cargo")))
        .into_iter()
        .flat_map(|config_root| {
            ["config.toml", "config"]
                .into_iter()
                .map(move |config_name| config_root.join(config_name))
        })
        .filter(|config_path| config_path.exists() && !candidates.contains(config_path))
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|config_path| candidates.push(config_path));

    candidates
}

pub(crate) fn cargo_config_search_roots(
    cargo_args: &[String],
    working_directory: Option<&Path>,
) -> Vec<PathBuf> {
    let mut search_roots = Vec::new();

    if let Some(manifest_directory) = selected_manifest_directory(cargo_args, working_directory)
        && !search_roots.contains(&manifest_directory)
    {
        search_roots.push(manifest_directory);
    }

    if let Some(working_directory) = working_directory.map(Path::to_path_buf)
        && !search_roots.contains(&working_directory)
    {
        search_roots.push(working_directory);
    }

    search_roots
}

pub(crate) fn parse_build_target_from_inline_config(config_argument: &str) -> Option<String> {
    config_argument
        .strip_prefix("build.target=")
        .map(trim_wrapping_quotes)
        .filter(|target| !target.is_empty())
        .map(str::to_string)
        .or_else(|| {
            parse_inline_config_value(config_argument, &["build", "target"]).and_then(|value| {
                value
                    .as_str()
                    .filter(|target| !target.is_empty())
                    .map(str::to_string)
            })
        })
}

pub(crate) fn parse_build_target_from_config_file(config_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    value
        .get("build")?
        .get("target")?
        .as_str()
        .filter(|target| !target.is_empty())
        .map(str::to_string)
}

pub(crate) fn parse_profile_debug_from_inline_config(
    config_argument: &str,
    profile_name: &str,
) -> Option<bool> {
    let prefix = format!("profile.{profile_name}.debug=");
    config_argument
        .strip_prefix(&prefix)
        .and_then(parse_debuginfo_value)
        .or_else(|| {
            parse_inline_config_value(config_argument, &["profile", profile_name, "debug"])
                .and_then(|value| parse_debuginfo_toml_value(&value))
        })
}

#[cfg(test)]
pub(crate) fn parse_profile_debug_from_config_file(
    config_path: &Path,
    profile_name: &str,
) -> Option<bool> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    resolve_profile_debug_from_toml(&value, profile_name, &mut Vec::new())
}

fn selected_manifest_directory(
    cargo_args: &[String],
    working_directory: Option<&Path>,
) -> Option<PathBuf> {
    let mut index = 0;

    while index < cargo_args.len() {
        let argument = &cargo_args[index];
        let manifest_path = if let Some(path) = argument.strip_prefix("--manifest-path=") {
            Some(path)
        } else if argument == "--manifest-path" {
            cargo_args.get(index + 1).map(|value| value.as_str())
        } else {
            None
        };

        if let Some(manifest_path) = manifest_path {
            let manifest_path = PathBuf::from(manifest_path);
            let resolved_path = if manifest_path.is_absolute() {
                manifest_path
            } else if let Some(working_directory) = working_directory {
                working_directory.join(manifest_path)
            } else {
                manifest_path
            };

            return resolved_path
                .parent()
                .map(normalize_search_path)
                .filter(|path| !path.as_os_str().is_empty());
        }

        if argument == "--manifest-path" {
            index += 1;
        }

        index += 1;
    }

    None
}

fn normalize_search_path(path: &Path) -> PathBuf {
    path.components()
        .fold(PathBuf::new(), |mut normalized_path, component| {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if normalized_path.file_name().is_some() {
                        normalized_path.pop();
                    } else if !normalized_path.has_root() {
                        normalized_path.push(component.as_os_str());
                    }
                }
                _ => normalized_path.push(component.as_os_str()),
            }

            normalized_path
        })
}

fn cargo_home_fallback_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            let mut home_directory = PathBuf::from(drive);
            home_directory.push(path);
            Some(home_directory)
        })
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

pub(crate) fn parse_inline_config_value(
    config_argument: &str,
    path: &[&str],
) -> Option<toml::Value> {
    let value: toml::Value = toml::from_str(config_argument).ok()?;
    let mut current = &value;

    for key in path {
        current = current.get(*key)?;
    }

    Some(current.clone())
}

pub(crate) fn parse_debuginfo_toml_value(value: &toml::Value) -> Option<bool> {
    match value {
        toml::Value::Boolean(enabled) => Some(*enabled),
        toml::Value::Integer(level) => Some(*level > 0),
        toml::Value::String(level) => parse_debuginfo_value(level),
        _ => None,
    }
}

fn parse_debuginfo_value(value: &str) -> Option<bool> {
    let normalized = trim_wrapping_quotes(value).trim().to_ascii_lowercase();

    match normalized.as_str() {
        "true" | "1" | "2" | "limited" | "line-tables-only" | "line-directives-only" | "full" => {
            Some(true)
        }
        "false" | "0" | "none" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
fn resolve_profile_debug_from_toml(
    value: &toml::Value,
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

    let resolved = value
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
                            resolve_profile_debug_from_toml(value, inherits, visited)
                        })
                })
        });

    visited.pop();
    resolved
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        cargo_config_file_candidates_with_inputs, cargo_config_search_roots,
        configured_build_target, extract_cargo_config_args, parse_build_target_from_config_file,
        parse_build_target_from_inline_config, parse_profile_debug_from_config_file,
        parse_profile_debug_from_inline_config,
    };

    #[test]
    fn extracts_cargo_config_args_from_split_and_inline_flags() {
        let config_args = extract_cargo_config_args(&[
            "--locked".to_string(),
            "--config=build.target='x86_64-unknown-linux-gnu'".to_string(),
            "--config".to_string(),
            "target.x86_64-unknown-linux-gnu.linker='zig'".to_string(),
        ]);

        assert_eq!(
            config_args,
            vec![
                "build.target='x86_64-unknown-linux-gnu'".to_string(),
                "target.x86_64-unknown-linux-gnu.linker='zig'".to_string(),
            ]
        );
    }

    #[test]
    fn parses_build_target_from_inline_cargo_config() {
        assert_eq!(
            parse_build_target_from_inline_config("build.target=\"x86_64-pc-windows-gnu\"")
                .as_deref(),
            Some("x86_64-pc-windows-gnu")
        );
    }

    #[test]
    fn parses_build_target_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_directory =
            std::env::temp_dir().join(format!("boltffi-build-target-config-test-{unique}"));
        fs::create_dir_all(&temp_directory).expect("create temp directory");
        let config_path = temp_directory.join("config.toml");

        fs::write(
            &config_path,
            "[build]\ntarget = \"x86_64-unknown-linux-musl\"\n",
        )
        .expect("write config file");

        let target = parse_build_target_from_config_file(&config_path).expect("config target");

        assert_eq!(target, "x86_64-unknown-linux-musl");
    }

    #[test]
    fn parses_profile_debug_from_cargo_config_file_with_inheritance() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_directory =
            std::env::temp_dir().join(format!("boltffi-profile-debug-config-test-{unique}"));
        fs::create_dir_all(&temp_directory).expect("create temp directory");
        let config_path = temp_directory.join("config.toml");

        fs::write(
            &config_path,
            r#"
[profile.release]
debug = "line-tables-only"

[profile.mobile-release]
inherits = "release"
"#,
        )
        .expect("write config file");

        let enabled = parse_profile_debug_from_config_file(&config_path, "mobile-release");

        assert_eq!(enabled, Some(true));
    }

    #[test]
    fn parses_line_directives_only_profile_debug_from_cargo_config_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_directory =
            std::env::temp_dir().join(format!("boltffi-profile-line-directives-config-{unique}"));
        fs::create_dir_all(&temp_directory).expect("create temp directory");
        let config_path = temp_directory.join("config.toml");

        fs::write(
            &config_path,
            r#"
[profile.release]
debug = "line-directives-only"
"#,
        )
        .expect("write config file");

        let enabled = parse_profile_debug_from_config_file(&config_path, "release");

        assert_eq!(enabled, Some(true));
    }

    #[test]
    fn searches_manifest_directory_before_current_directory() {
        let search_roots = cargo_config_search_roots(
            &[
                "--manifest-path".to_string(),
                "workspace/member/Cargo.toml".to_string(),
            ],
            Some(Path::new("/tmp/project")),
        );

        assert_eq!(
            search_roots,
            vec![
                PathBuf::from("/tmp/project/workspace/member"),
                PathBuf::from("/tmp/project"),
            ]
        );
    }

    #[test]
    fn includes_home_cargo_config_candidates_after_workspace_candidates() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("boltffi-cargo-config-candidates-{unique}"));
        let workspace_directory = temp_root.join("workspace");
        let home_directory = temp_root.join("home");

        fs::create_dir_all(workspace_directory.join(".cargo")).expect("create workspace .cargo");
        fs::create_dir_all(home_directory.join(".cargo")).expect("create home .cargo");
        fs::write(workspace_directory.join(".cargo").join("config.toml"), [])
            .expect("write workspace cargo config");
        fs::write(home_directory.join(".cargo").join("config"), [])
            .expect("write home cargo config");

        let candidates = cargo_config_file_candidates_with_inputs(
            vec![workspace_directory.clone()],
            Some(workspace_directory.clone()),
            None,
            Some(home_directory.clone()),
        );

        assert!(candidates.contains(&workspace_directory.join(".cargo").join("config.toml")));
        assert!(candidates.contains(&home_directory.join(".cargo").join("config")));
    }

    #[test]
    fn explicit_cargo_home_replaces_the_default_configuration_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("boltffi-explicit-cargo-home-{unique}"));
        let workspace = root.join("workspace");
        let home_directory = root.join("home");
        let cargo_home = root.join("cargo");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::create_dir_all(home_directory.join(".cargo")).expect("create default cargo home");
        fs::create_dir_all(&cargo_home).expect("create explicit cargo home");
        fs::write(
            home_directory.join(".cargo/config.toml"),
            "[profile.dev]\ndebug = true\n",
        )
        .expect("write default configuration");
        fs::write(
            cargo_home.join("config.toml"),
            "[profile.dev]\ndebug = false\n",
        )
        .expect("write explicit configuration");

        let candidates = cargo_config_file_candidates_with_inputs(
            vec![workspace.clone()],
            Some(workspace),
            Some(cargo_home.clone()),
            Some(home_directory),
        );

        assert_eq!(candidates, vec![cargo_home.join("config.toml")]);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn resolves_cargo_configured_build_target_from_inline_config() {
        let target = configured_build_target(
            &["--config=build.target='x86_64-pc-windows-gnu'".to_string()],
            Some(Path::new("/tmp/workspace")),
        )
        .expect("configured build target");

        assert_eq!(target, "x86_64-pc-windows-gnu");
    }

    #[test]
    fn parses_spaced_inline_build_target_config() {
        let target =
            parse_build_target_from_inline_config("build.target = 'x86_64-pc-windows-gnu'")
                .expect("inline build target");

        assert_eq!(target, "x86_64-pc-windows-gnu");
    }

    #[test]
    fn parses_spaced_inline_profile_debug_config() {
        let enabled =
            parse_profile_debug_from_inline_config("profile.release.debug = true", "release");

        assert_eq!(enabled, Some(true));
    }
}
