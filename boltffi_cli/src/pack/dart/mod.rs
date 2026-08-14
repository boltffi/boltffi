use crate::{
    build::{
        BindingExpansion, BuildOptions, BuildSelection, Builder, CargoBuildProfile, OutputCallback,
        all_successful, failed_targets, resolve_build_profile,
    },
    cargo::Cargo,
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::PackDartOptions,
    },
    config::Config,
    pack::{PackError, print_cargo_line, resolve_build_cargo_args, scratch},
    reporter::{Reporter, Step},
};

/// `dart_shims.rs` is a build-time handoff from Dart generation to the
/// later build step's `build.rs`, never something the Dart package ships.
/// Relocates it out of the Dart package tree into
/// `scratch::Directory::for_target("dart")`. Called from every Dart
/// generation entry point (not just `pack dart`) so `pack dart --regenerate
/// false` works regardless of which command generated the bindings. Does
/// not delete the scratch copy after a build consumes it -- scratch is
/// durable until `cargo clean`, not single-shot.
pub(crate) fn relocate_dart_shim_to_scratch(config: &Config) -> Result<std::path::PathBuf> {
    let scratch_path = scratch::Directory::for_target("dart")?.join("dart_shims.rs");
    let generated_path = config
        .dart_output()
        .join(&config.package.name)
        .join("native")
        .join("dart_shims.rs");

    if generated_path.exists() {
        std::fs::create_dir_all(scratch_path.parent().expect("scratch path has a parent"))
            .map_err(|source| CliError::CreateDirectoryFailed {
                path: scratch_path.clone(),
                source,
            })?;
        std::fs::rename(&generated_path, &scratch_path)
            .or_else(|_| {
                // `rename` can fail across filesystem/volume boundaries.
                std::fs::copy(&generated_path, &scratch_path)?;
                std::fs::remove_file(&generated_path)
            })
            .map_err(|source| CliError::CopyFailed {
                from: generated_path,
                to: scratch_path.clone(),
                source,
            })?;
    }

    Ok(scratch_path)
}

fn build_dart_targets(
    config: &Config,
    release: bool,
    build_cargo_args: &[String],
    dart_shim_rs_path: Option<&std::path::Path>,
    step: &Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    // Always enabled (not just when a shim was generated) to keep the
    // built artifact stable across runs -- build.rs stages an empty stub
    // when BOLTFFI_DART_SHIM_RS is unset.
    let mut dart_cargo_args = build_cargo_args.to_vec();
    dart_cargo_args.push("--features".to_string());
    dart_cargo_args.push("boltffi/dart".to_string());

    let expansion = dart_expansion(config, &dart_cargo_args)?;

    let mut options = dart_build_options(expansion, release, on_output);
    if let Some(shim_path) = dart_shim_rs_path {
        // Must be absolute: this crosses into `boltffi`'s build.rs via env
        // var, whose cwd is `boltffi`'s own crate root, not wherever this
        // CLI was invoked from.
        let absolute_shim_path = if shim_path.is_absolute() {
            shim_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(shim_path))
                .unwrap_or_else(|_| shim_path.to_path_buf())
        };
        options.extra_env.push((
            "BOLTFFI_DART_SHIM_RS".to_string(),
            absolute_shim_path.display().to_string(),
        ));
    }

    let builder = Builder::new(config, options);
    let results = builder.build_targets(&config.dart_targets())?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(CliError::Pack(PackError::BuildFailed { targets: failed }))
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

    // Bindings are generated before the Rust cdylib is built, unlike other
    // targets: generation also emits `dart_shims.rs`, which the build needs
    // staged in scratch first. `run_generate_with_output` already relocates
    // it as part of `GenerateTarget::Dart`.
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

    let dart_shim_rs_path = scratch::Directory::for_target("dart")?.join("dart_shims.rs");

    if !options.execution.no_build {
        let step = reporter.step("Building Rust cdylib");
        build_dart_targets(
            config,
            matches!(build_profile, CargoBuildProfile::Release),
            &build_cargo_args,
            Some(&dart_shim_rs_path),
            &step,
        )?;
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

    use super::{BindingExpansion, BuildSelection, dart_build_options, dart_expansion};
    use crate::config::Config;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
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
