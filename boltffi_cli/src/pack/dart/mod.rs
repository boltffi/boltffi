use crate::{
    build::{
        BindingExpansion, BuildOptions, BuildResult, BuildSelection, Builder, CargoBuildProfile,
        OutputCallback, all_successful, failed_targets, resolve_build_profile,
    },
    cargo::Cargo,
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::PackDartOptions,
    },
    config::Config,
    pack::{PackError, print_cargo_line, resolve_build_cargo_args},
    reporter::Reporter,
};

/// Shared by `pack dart` and `boltffi build dart`/`build all`'s Dart leg.
/// Enables `boltffi/dart` (links the runtime) and `--cfg boltffi_dart` so
/// user-crate macros emit the dual-path stubs. Other packs do not set this
/// cfg, so those artifacts stay unbloated.
pub(crate) fn build_dart_targets(
    config: &Config,
    release: bool,
    build_cargo_args: &[String],
    verbose: bool,
) -> Result<Vec<BuildResult>> {
    let on_output: Option<OutputCallback> = if verbose {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let mut dart_cargo_args = build_cargo_args.to_vec();
    let package_manifest = dart_expansion(config, build_cargo_args)?.manifest_path();
    // Every facade alias (top-level and per-target) must get `…/dart` so a
    // host that only activates one renamed key still links the runtime.
    let feature_list = boltffi_dependency_keys(&package_manifest)
        .into_iter()
        .map(|dep| format!("{dep}/dart"))
        .collect::<Vec<_>>();
    let features = if feature_list.is_empty() {
        "boltffi/dart".to_owned()
    } else {
        feature_list.join(",")
    };
    dart_cargo_args.push("--features".to_string());
    dart_cargo_args.push(features);

    let expansion = dart_expansion(config, &dart_cargo_args)?;

    let mut options = dart_build_options(expansion, release, on_output);
    push_dart_cfg_env(&mut options.extra_env);

    let builder = Builder::new(config, options);
    builder.build_targets(&config.dart_targets())
}

fn boltffi_dependency_keys(package_manifest: impl AsRef<std::path::Path>) -> Vec<String> {
    let package_manifest = package_manifest.as_ref();
    let Ok(text) = std::fs::read_to_string(package_manifest) else {
        return Vec::new();
    };
    let Ok(value) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let workspace_deps = workspace_dependency_table(package_manifest);
    let mut keys = std::collections::BTreeSet::new();

    collect_boltffi_dependency_keys(
        value.get("dependencies"),
        workspace_deps.as_ref(),
        &mut keys,
    );

    // `[target.'cfg(...)'.dependencies]` may hold the only (or differently
    // renamed) facade entry; enable every alias so each Dart host target
    // can select the key that is active for its cfg.
    if let Some(targets) = value.get("target").and_then(|v| v.as_table()) {
        for table in targets.values() {
            collect_boltffi_dependency_keys(
                table.as_table().and_then(|t| t.get("dependencies")),
                workspace_deps.as_ref(),
                &mut keys,
            );
        }
    }

    keys.into_iter().collect()
}

/// First resolved key, for tests that only need a single representative alias.
#[cfg(test)]
fn boltffi_dependency_key(package_manifest: impl AsRef<std::path::Path>) -> Option<String> {
    boltffi_dependency_keys(package_manifest).into_iter().next()
}

fn collect_boltffi_dependency_keys(
    deps: Option<&toml::Value>,
    workspace_deps: Option<&toml::map::Map<String, toml::Value>>,
    keys: &mut std::collections::BTreeSet<String>,
) {
    let Some(deps) = deps.and_then(|v| v.as_table()) else {
        return;
    };
    for (key, dep) in deps {
        if dependency_is_boltffi(key, dep, workspace_deps) {
            keys.insert(key.clone());
        }
    }
}

fn dependency_is_boltffi(
    key: &str,
    dep: &toml::Value,
    workspace_deps: Option<&toml::map::Map<String, toml::Value>>,
) -> bool {
    match dep {
        toml::Value::String(_) => key == "boltffi",
        toml::Value::Table(table) => {
            if let Some(package) = table.get("package").and_then(|v| v.as_str()) {
                return package == "boltffi";
            }
            if table
                .get("workspace")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && let Some(workspace_deps) = workspace_deps
                && let Some(workspace_dep) = workspace_deps.get(key)
            {
                return dependency_is_boltffi(key, workspace_dep, None);
            }
            key == "boltffi"
        }
        _ => false,
    }
}

fn workspace_dependency_table(
    package_manifest: &std::path::Path,
) -> Option<toml::map::Map<String, toml::Value>> {
    let mut dir = package_manifest.parent()?;
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.is_file()
            && let Ok(text) = std::fs::read_to_string(&candidate)
            && let Ok(value) = text.parse::<toml::Table>()
            && let Some(workspace) = value.get("workspace").and_then(|v| v.as_table())
        {
            if let Some(deps) = workspace.get("dependencies").and_then(|v| v.as_table()) {
                return Some(deps.clone());
            }
            return None;
        }
        dir = dir.parent()?;
    }
}

fn push_dart_cfg_env(extra_env: &mut Vec<(String, String)>) {
    const DART_FLAGS: &[&str] = &["--cfg", "boltffi_dart", "--check-cfg=cfg(boltffi_dart)"];
    const SEP: char = '\u{1f}';

    if let Ok(encoded) = std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        let mut parts: Vec<String> = encoded
            .split(SEP)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        parts.extend(DART_FLAGS.iter().map(|flag| (*flag).to_owned()));
        extra_env.push((
            "CARGO_ENCODED_RUSTFLAGS".to_string(),
            parts.join(&SEP.to_string()),
        ));
        return;
    }

    let rustflags = match std::env::var("RUSTFLAGS") {
        Ok(existing) if !existing.is_empty() => {
            format!("{existing} --cfg boltffi_dart --check-cfg=cfg(boltffi_dart)")
        }
        _ => "--cfg boltffi_dart --check-cfg=cfg(boltffi_dart)".to_string(),
    };
    extra_env.push(("RUSTFLAGS".to_string(), rustflags));
}

// Cargo only sets CARGO_FEATURE_* for build scripts, so this must build as
// a binding expansion for the macros to see active features (same fix as
// the Python target's cdylib build). resolve_preferred (not
// resolve_for_surface) keeps the configured/default artifact selected
// even when the package has more than one FFI-capable cargo target.
fn dart_expansion(config: &Config, build_cargo_args: &[String]) -> Result<BindingExpansion> {
    BindingExpansion::resolve_preferred(config, build_cargo_args, &config.crate_artifact_name())
}

fn dart_build_options(
    expansion: BindingExpansion,
    release: bool,
    on_output: Option<OutputCallback>,
) -> BuildOptions {
    BuildOptions {
        release,
        selection: BuildSelection::Expanded(Box::new(expansion)),
        on_output,
        extra_env: Vec::new(),
    }
}

pub(crate) fn pack_dart(
    config: &Config,
    options: PackDartOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_dart_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.dart.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("☕", "Packing Dart");

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);

    if options.execution.regenerate {
        let step = reporter.step("Generating Dart bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Dart,
                output: Some(config.dart_output()),
                experimental: options.experimental,
                cargo_args: build_cargo_args.clone(),
                deny_skipped: options.execution.deny_skipped,
            },
        )?;

        step.finish_success();
    }

    if !options.execution.no_build {
        let step = reporter.step("Building Rust cdylib");
        let results = build_dart_targets(
            config,
            matches!(build_profile, CargoBuildProfile::Release),
            &build_cargo_args,
            step.is_verbose(),
        )?;
        if !all_successful(&results) {
            return Err(CliError::Pack(PackError::BuildFailed {
                targets: failed_targets(&results),
            }));
        }
        step.finish_success();
    }

    let step = reporter.step("Packaging native libraries");

    let cargo = Cargo::current(&build_cargo_args)?;

    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let package_selector =
        cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);

    let libraries = metadata.resolve_built_libraries_for_targets(
        &cargo_manifest_path,
        build_profile.output_directory_name(),
        &config.crate_artifact_name(),
        package_selector.as_deref(),
        &config.dart_targets(),
    )?;

    let package_dir = config.dart_output().join(&config.package.name);
    let native_libs_dir = package_dir.join("native");
    std::fs::create_dir_all(&native_libs_dir).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: native_libs_dir.clone(),
            source,
        }
    })?;

    for l in libraries {
        let native_lib_triple_dir = native_libs_dir.join(l.target.triple());
        std::fs::create_dir_all(&native_lib_triple_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: native_lib_triple_dir.clone(),
                source,
            }
        })?;

        let native_lib_filepath =
            native_lib_triple_dir.join(l.path.file_name().expect("file shouldn't terminate in .."));

        std::fs::copy(&l.path, &native_lib_filepath).map_err(|source| CliError::CopyFailed {
            from: l.path,
            to: native_lib_filepath,
            source,
        })?;
    }

    step.finish_success();

    reporter.finish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        BindingExpansion, BuildSelection, boltffi_dependency_key, boltffi_dependency_keys,
        dart_build_options, dart_expansion,
    };
    use crate::config::Config;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    #[test]
    fn boltffi_dependency_key_resolves_renamed_and_workspace_inherited_facades() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("boltffi-dep-key-{unique_suffix}"));
        let member = root.join("member");
        std::fs::create_dir_all(member.join("src")).expect("create member src");
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"member\"]\n\n\
             [workspace.dependencies]\n\
             ffi = { package = \"boltffi\", version = \"0.30.0\" }\n",
        )
        .expect("write workspace Cargo.toml");
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nffi = { workspace = true }\n",
        )
        .expect("write member Cargo.toml");

        let key = boltffi_dependency_key(member.join("Cargo.toml"));
        std::fs::remove_dir_all(&root).expect("cleanup");
        assert_eq!(key.as_deref(), Some("ffi"));
    }

    #[test]
    fn boltffi_dependency_key_resolves_inline_package_rename() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("boltffi-dep-key-inline-{unique_suffix}"));
        std::fs::create_dir_all(&dir).expect("create dir");
        let manifest = dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nmy_ffi = { package = \"boltffi\", version = \"0.30.0\" }\n",
        )
        .expect("write Cargo.toml");

        let key = boltffi_dependency_key(&manifest);
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(key.as_deref(), Some("my_ffi"));
    }

    #[test]
    fn boltffi_dependency_key_resolves_target_specific_package_rename() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("boltffi-dep-key-target-{unique_suffix}"));
        std::fs::create_dir_all(&dir).expect("create dir");
        let manifest = dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [target.'cfg(unix)'.dependencies]\n\
             ffi = { package = \"boltffi\", version = \"0.30.0\" }\n",
        )
        .expect("write Cargo.toml");

        let key = boltffi_dependency_key(&manifest);
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(key.as_deref(), Some("ffi"));
    }

    #[test]
    fn boltffi_dependency_keys_collects_every_target_specific_alias() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("boltffi-dep-keys-multi-{unique_suffix}"));
        std::fs::create_dir_all(&dir).expect("create dir");
        let manifest = dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [target.'cfg(unix)'.dependencies]\n\
             unix_ffi = { package = \"boltffi\", version = \"0.30.0\" }\n\n\
             [target.'cfg(windows)'.dependencies]\n\
             windows_ffi = { package = \"boltffi\", version = \"0.30.0\" }\n",
        )
        .expect("write Cargo.toml");

        let keys = boltffi_dependency_keys(&manifest);
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(keys, vec!["unix_ffi".to_owned(), "windows_ffi".to_owned()]);
    }

    /// `pack dart` must build the cdylib as a binding expansion, not a plain
    /// `cargo build`: the #[data]/#[error] macros read active features from
    /// BINDING_METADATA_FEATURES_ENV, which only `BuildSelection::Expanded`
    /// wires up (see `Builder::apply_expansion`). A plain build silently
    /// drops every #[cfg(feature = ...)]-gated module from the FFI surface.
    #[test]
    fn dart_cdylib_builds_as_a_binding_expansion() {
        let expansion = BindingExpansion::fixture(
            "/workspace/Cargo.toml",
            "/workspace/demo/Cargo.toml",
            ["--features".to_string(), "ffi".to_string()],
        );

        let options = dart_build_options(expansion, false, None);

        assert!(matches!(options.selection, BuildSelection::Expanded(_)));
    }

    /// A crate whose only other FFI-capable target is a cdylib example must
    /// still resolve through the configured/default artifact rather than
    /// erroring on the ambiguity.
    #[test]
    fn dart_expansion_prefers_the_configured_artifact_over_an_ambiguous_ffi_target_set() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let crate_dir = std::env::temp_dir().join(format!(
            "boltffi-dart-multi-ffi-target-test-{unique_suffix}"
        ));
        std::fs::create_dir_all(crate_dir.join("src")).expect("create src dir");
        std::fs::create_dir_all(crate_dir.join("examples")).expect("create examples dir");
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\n\
             name = \"demo_multi\"\n\
             version = \"0.1.0\"\n\
             edition = \"2021\"\n\n\
             [lib]\n\
             crate-type = [\"cdylib\", \"rlib\"]\n\n\
             [[example]]\n\
             name = \"extra\"\n\
             crate-type = [\"cdylib\"]\n",
        )
        .expect("write Cargo.toml");
        std::fs::write(crate_dir.join("src/lib.rs"), "").expect("write lib.rs");
        std::fs::write(crate_dir.join("examples/extra.rs"), "fn main() {}\n")
            .expect("write example");

        let config = parse_config("[package]\nname = \"demo_multi\"\n");
        let cargo_args = vec![
            "--manifest-path".to_string(),
            crate_dir.join("Cargo.toml").display().to_string(),
        ];

        let expansion = dart_expansion(&config, &cargo_args);

        std::fs::remove_dir_all(&crate_dir).expect("cleanup temp dir");
        assert!(expansion.is_ok(), "{:?}", expansion.err());
    }
}
