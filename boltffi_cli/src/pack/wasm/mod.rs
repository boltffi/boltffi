mod bindgen;
mod error;
mod javascript;
mod module;
mod npm;
mod package;
mod sections;

pub use error::Error;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use boltffi_backend::target::typescript::TypeScriptHost;
use boltffi_binding::BindingMetadataSurface;
use walkdir::WalkDir;

use crate::build::{
    BindingExpansion, BuildOptions, BuildSelection, Builder, OutputCallback, all_successful,
    failed_targets,
};
use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackWasmOptions;
use crate::config::{Config, WasmNpmTarget, WasmOptimizeLevel, WasmOptimizeOnMissing, WasmProfile};
use crate::pack::PackError;
use crate::reporter::Reporter;

use super::{print_cargo_line, resolve_build_cargo_args};

use self::npm::{
    generate_wasm_loader_entrypoints, generate_wasm_package_json, generate_wasm_readme,
};
use self::sections::strip_wasm_sections;
use self::{bindgen::Bindgen, module::Module, package::Package};

pub(crate) fn pack_wasm(
    config: &Config,
    options: PackWasmOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_wasm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.enabled = false".to_string(),
            status: None,
        });
    }
    if config.wasm_npm_generate_package_json() && config.wasm_npm_package_name().is_none() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.npm.package_name is required for pack wasm".to_string(),
            status: None,
        });
    }

    reporter.section("🌐", "Packing WASM");

    let requested_wasm_profile = if options.execution.release {
        WasmProfile::Release
    } else {
        config.wasm_profile()
    };

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile = crate::build::resolve_build_profile(
        matches!(requested_wasm_profile, WasmProfile::Release),
        &build_cargo_args,
    );

    let wasm_artifact_profile = match build_profile {
        crate::build::CargoBuildProfile::Debug => WasmProfile::Debug,
        crate::build::CargoBuildProfile::Release => WasmProfile::Release,
        crate::build::CargoBuildProfile::Named(_) if config.wasm_has_artifact_path_override() => {
            requested_wasm_profile
        }
        crate::build::CargoBuildProfile::Named(profile_name) => {
            return Err(CliError::CommandFailed {
                command: format!(
                    "custom cargo profile '{}' for wasm pack requires targets.wasm.artifact_path",
                    profile_name
                ),
                status: None,
            });
        }
    };

    if !options.execution.no_build {
        let step = reporter.step("Building WASM target");
        build_wasm_target(config, requested_wasm_profile, &build_cargo_args, &step)?;
        step.finish_success();
    }

    let wasm_artifact_path = WasmArtifactPath::resolve(config, wasm_artifact_profile)?.into_path();
    if !wasm_artifact_path.exists() {
        return Err(CliError::FileNotFound(wasm_artifact_path));
    }

    let npm_output = config.wasm_npm_output();
    let package = Package::new(&npm_output)?;
    let staging = package.path();
    let module_name = config.wasm_typescript_module_name();
    let enabled_targets = config.wasm_npm_targets();
    let runtime_imports = TypeScriptHost::new(&module_name)
        .map(|host| host.runtime_package(config.wasm_runtime_package()))
        .and_then(|host| host.runtime_imports())
        .map_err(|error| CliError::CommandFailed {
            command: error.to_string(),
            status: None,
        })?;
    let original_bytes = fs::read(&wasm_artifact_path).map_err(|source| CliError::ReadFailed {
        path: wasm_artifact_path.clone(),
        source,
    })?;
    let original = Module::parse(&original_bytes)?;
    let packaged_wasm_path = staging.join(format!("{module_name}_bg.wasm"));
    if let Some(version) = &original.bindgen_version {
        original.validate_bindgen()?;
        let step = reporter.step("Processing wasm-bindgen imports");
        let bindgen = Bindgen::resolve(config, version)?;
        bindgen.process(&wasm_artifact_path, &staging, &module_name)?;
        step.finish_success();
    } else {
        package.copy(&wasm_artifact_path, format!("{module_name}_bg.wasm"))?;
    }

    let strip_debug = should_strip_debug(config, wasm_artifact_profile);
    let step = reporter.step("Stripping WASM sections");
    let stripped_bytes = strip_wasm_sections(&packaged_wasm_path, strip_debug)?;
    if stripped_bytes == 0 {
        step.finish_success();
    } else {
        step.finish_success_with(&format!("removed {} KiB", stripped_bytes / 1024));
    }

    if config.wasm_optimize_enabled(wasm_artifact_profile) {
        let step = reporter.step("Optimizing WASM binary");
        optimize_wasm_binary(
            config,
            &packaged_wasm_path,
            wasm_artifact_profile,
            &build_cargo_args,
        )?;
        step.finish_success();
    }

    if options.execution.regenerate {
        let step = reporter.step("Generating TypeScript bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Typescript,
                output: Some(staging.clone()),
                experimental: false,
                cargo_args: build_cargo_args.clone(),
                deny_skipped: options.execution.deny_skipped,
            },
        )?;
        step.finish_success();
    } else {
        let sources = config.wasm_typescript_output();
        if original.bindgen_version.is_some()
            && !sources.join(format!("{module_name}_imports.ts")).is_file()
        {
            return Err(Error::Unsupported("these TypeScript bindings predate wasm-bindgen integration; pack again with --regenerate true".to_owned()).into());
        }
        let node_source = format!("{module_name}_node.ts");
        std::iter::once(format!("{module_name}.ts"))
            .chain(sources.join(&node_source).is_file().then_some(node_source))
            .try_for_each(|name| package.copy(&sources.join(&name), name))?;
    }

    let processed_bytes = fs::read(&packaged_wasm_path).map_err(|source| CliError::ReadFailed {
        path: packaged_wasm_path.clone(),
        source,
    })?;
    let processed = Module::parse(&processed_bytes)?;
    original.validate_interface(&processed)?;
    if original.bindgen_version.is_some() {
        processed.validate_bindgen()?;
        Bindgen::write_imports(
            &processed,
            &staging,
            &module_name,
            &config.wasm_runtime_package(),
        )?;
    } else {
        let path = staging.join(runtime_imports.path().as_path());
        fs::write(&path, runtime_imports.contents())
            .map_err(|source| CliError::WriteFailed { path, source })?;
    }

    let source_files = package.files()?;
    let step = reporter.step("Transpiling TypeScript bindings");
    transpile_typescript_bundle(config, &staging)?;
    step.finish_success();

    let step = reporter.step("Generating WASM loader entrypoints");
    generate_wasm_loader_entrypoints(&module_name, &enabled_targets, &staging)?;
    step.finish_success();

    if config.wasm_npm_generate_package_json() {
        let step = reporter.step("Generating package.json");
        generate_wasm_package_json(config, &module_name, &enabled_targets, &staging)?;
        step.finish_success_with(&format!("{}", npm_output.join("package.json").display()));
    } else if npm_output.join("package.json").is_file() {
        package.copy(&npm_output.join("package.json"), "package.json")?;
    }

    if config.wasm_npm_generate_readme() {
        let step = reporter.step("Generating README.md");
        generate_wasm_readme(config, &module_name, &enabled_targets, &staging)?;
        step.finish_success_with(&format!("{}", npm_output.join("README.md").display()));
    } else if npm_output.join("README.md").is_file() {
        package.copy(&npm_output.join("README.md"), "README.md")?;
    }

    if config
        .wasm_typescript_output()
        .canonicalize()
        .ok()
        .as_deref()
        != Some(package.output())
    {
        let sources = Package::new(&config.wasm_typescript_output())?;
        source_files.iter().try_for_each(|relative| {
            let path = staging.join(relative);
            sources.copy(&path, relative)?;
            if relative
                .extension()
                .is_some_and(|extension| extension == "js")
            {
                let declaration = relative.with_extension("d.ts");
                if staging.join(&declaration).is_file() {
                    sources.copy(&staging.join(&declaration), declaration)?;
                }
            }
            if relative
                .extension()
                .is_some_and(|extension| extension == "ts")
            {
                fs::remove_file(&path).map_err(|source| CliError::WriteFailed { path, source })?;
            }
            Ok::<_, CliError>(())
        })?;
        sources.publish()?;
    }
    package.publish()
}

/// Whether the pack should drop debug sections.
///
/// Debug info only goes when optimization applies — release by default — so a
/// debug-profile pack keeps DWARF as it did before this step existed.
/// Descriptor tables are dead weight in every profile and are handled
/// separately by the strip itself.
fn should_strip_debug(config: &Config, profile: WasmProfile) -> bool {
    config.wasm_optimize_enabled(profile) && config.wasm_optimize_strip_debug()
}

fn build_wasm_target(
    config: &Config,
    profile: WasmProfile,
    build_cargo_args: &[String],
    step: &crate::reporter::Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let expansion = BindingExpansion::resolve_for_surface(
        config,
        build_cargo_args,
        BindingMetadataSurface::Wasm32,
    )?;
    let build_options = BuildOptions {
        release: matches!(profile, WasmProfile::Release),
        selection: BuildSelection::Expanded(Box::new(expansion)),
        on_output,
        extra_env: Vec::new(),
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_wasm_with_triple(config.wasm_triple())?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}

struct WasmArtifactPath {
    path: PathBuf,
}

impl WasmArtifactPath {
    fn resolve(config: &Config, profile: WasmProfile) -> Result<Self> {
        Ok(Self::in_target_directory(
            config,
            profile,
            super::cargo_target_directory()?,
        ))
    }

    fn in_target_directory(
        config: &Config,
        profile: WasmProfile,
        cargo_target_directory: PathBuf,
    ) -> Self {
        let path = if config.wasm_has_artifact_path_override() {
            config.wasm_artifact_path(profile)
        } else {
            cargo_target_directory
                .join(config.wasm_triple())
                .join(profile.as_str())
                .join(format!("{}.wasm", config.crate_artifact_name()))
        };

        Self { path }
    }

    fn into_path(self) -> PathBuf {
        self.path
    }
}

/// Binaryen infers which wasm features a module may use from its
/// `target_features` custom section — and the linker deletes that section under
/// `[profile.release] strip = true`, which is the usual advice for small wasm.
/// `wasm-opt` then rejects the module it was just handed: rustc emits
/// `memory.copy`, and without the section binaryen assumes bulk memory is off.
///
/// So the features are passed explicitly, taken from what rustc reports for the
/// target rather than hardcoded, which keeps them correct if rustc's defaults
/// move. `--all-features` would also silence the error, but it lets `wasm-opt`
/// *emit* SIMD, threads or GC instructions the host engine may not implement.
fn binaryen_feature_flags(
    config: &Config,
    profile: WasmProfile,
    build_cargo_args: &[String],
) -> Vec<&'static str> {
    effective_target_features(config, profile, build_cargo_args)
        .iter()
        .flat_map(|feature| binaryen_flags_for(feature))
        .copied()
        .collect()
}

/// Maps a rustc target feature onto the flags binaryen spells it with. Features
/// binaryen does not know are dropped: passing an unknown `--enable-*` makes it
/// exit, and leaving one out is no worse than today.
fn binaryen_flags_for(feature: &str) -> &'static [&'static str] {
    match feature {
        // Binaryen splits bulk memory: `memory.copy` and `memory.fill` sit
        // behind the `-opt` half, and rustc emits both.
        "bulk-memory" => &["--enable-bulk-memory", "--enable-bulk-memory-opt"],
        "multivalue" => &["--enable-multivalue"],
        "mutable-globals" => &["--enable-mutable-globals"],
        "nontrapping-fptoint" => &["--enable-nontrapping-float-to-int"],
        "reference-types" => &["--enable-reference-types"],
        "sign-ext" => &["--enable-sign-ext"],
        "simd128" => &["--enable-simd"],
        "atomics" => &["--enable-threads"],
        "exception-handling" => &["--enable-exception-handling"],
        "extended-const" => &["--enable-extended-const"],
        "relaxed-simd" => &["--enable-relaxed-simd"],
        "tail-call" => &["--enable-tail-call"],
        _ => &[],
    }
}

/// Reads the features the *build* actually enables, by asking rustc through
/// cargo with the same arguments the build used.
///
/// A bare `rustc --print cfg` would report only the default toolchain's
/// defaults: it receives no `RUSTFLAGS`, no `CARGO_TARGET_*_RUSTFLAGS`, no
/// `.cargo/config.toml`, and not the toolchain a `rust-toolchain.toml` selects.
/// A project enabling `simd128` that way would still have its module rejected.
/// Going through `cargo rustc` makes cargo resolve all of that.
///
/// Returns empty when cargo cannot be reached or the query fails, which leaves
/// the previous behaviour rather than failing the pack over a diagnostic.
fn effective_target_features(
    config: &Config,
    profile: WasmProfile,
    build_cargo_args: &[String],
) -> Vec<String> {
    let mut command = Command::new("cargo");
    command
        .arg("rustc")
        .args(["--target", config.wasm_triple()]);
    if matches!(profile, WasmProfile::Release) {
        command.arg("--release");
    }
    command.args(build_cargo_args);
    command.args(["--", "--print", "cfg"]);

    let Ok(output) = command.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_target_features(&String::from_utf8_lossy(&output.stdout))
}

fn parse_target_features(cfg: &str) -> Vec<String> {
    cfg.lines()
        .filter_map(|line| line.trim().strip_prefix("target_feature="))
        .map(|value| value.trim_matches('"').to_string())
        .collect()
}

fn optimize_wasm_binary(
    config: &Config,
    wasm_path: &Path,
    profile: WasmProfile,
    build_cargo_args: &[String],
) -> Result<()> {
    let optimize_level_flag = match config.wasm_optimize_level() {
        WasmOptimizeLevel::O0 => "-O0",
        WasmOptimizeLevel::O1 => "-O1",
        WasmOptimizeLevel::O2 => "-O2",
        WasmOptimizeLevel::O3 => "-O3",
        WasmOptimizeLevel::O4 => "-O4",
        WasmOptimizeLevel::Size => "-Os",
        WasmOptimizeLevel::MinSize => "-Oz",
    };

    let wasm_opt_path = match which::which("wasm-opt") {
        Ok(path) => path,
        Err(_) => {
            return match config.wasm_optimize_on_missing() {
                WasmOptimizeOnMissing::Error => Err(CliError::CommandFailed {
                    command: "wasm-opt not found in PATH".to_string(),
                    status: None,
                }),
                WasmOptimizeOnMissing::Warn => {
                    println!("warning: wasm-opt not found, skipping optimization");
                    Ok(())
                }
                WasmOptimizeOnMissing::Skip => Ok(()),
            };
        }
    };

    let optimized_path = wasm_path.with_extension("optimized.wasm");
    let mut command = Command::new(wasm_opt_path);
    command
        .arg(optimize_level_flag)
        .arg(wasm_path)
        .arg("-o")
        .arg(&optimized_path);

    for flag in binaryen_feature_flags(config, profile, build_cargo_args) {
        command.arg(flag);
    }

    if !config.wasm_optimize_strip_debug() {
        // Keeps the name section, and any debug sections the strip pass left.
        command.arg("-g");
    }

    let status = command.status().map_err(|_| CliError::CommandFailed {
        command: "wasm-opt".to_string(),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "wasm-opt".to_string(),
            status: status.code(),
        });
    }

    std::fs::rename(&optimized_path, wasm_path).map_err(|source| CliError::WriteFailed {
        path: wasm_path.to_path_buf(),
        source,
    })
}

fn transpile_typescript_bundle(config: &Config, output_dir: &Path) -> Result<()> {
    let module_name = config.wasm_typescript_module_name();
    let browser_source = output_dir.join(format!("{module_name}.ts"));
    let node_source = output_dir.join(format!("{module_name}_node.ts"));
    if !browser_source.is_file() {
        return Err(CliError::FileNotFound(browser_source));
    }
    if config.wasm_npm_targets().contains(&WasmNpmTarget::Nodejs) && !node_source.is_file() {
        return Err(CliError::FileNotFound(node_source));
    }
    let compiled = tempfile::Builder::new()
        .prefix(".typescript-")
        .tempdir_in(output_dir)
        .map_err(|source| CliError::CreateDirectoryFailed {
            path: output_dir.to_owned(),
            source,
        })?;
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "npx", "tsc"]);
        command
    } else {
        Command::new("tsc")
    };
    command
        .arg(browser_source)
        .args(node_source.is_file().then_some(&node_source))
        .arg("--target")
        .arg("ES2020")
        .arg("--lib")
        .arg("ES2021,DOM")
        .arg("--module")
        .arg("ES2020")
        .arg("--moduleResolution")
        .arg("bundler")
        .arg("--declaration")
        .arg("--allowJs")
        .arg("--rootDir")
        .arg(output_dir)
        .arg("--skipLibCheck")
        .arg("--strictBindCallApply")
        .arg("--noImplicitAny")
        .arg("--noEmitOnError")
        .arg("true")
        .arg("--outDir")
        .arg(compiled.path());
    if config.wasm_source_map_enabled() {
        command.args(["--sourceMap", "--inlineSources", "--sourceRoot", "./"]);
    }

    let output = command.output().map_err(|_| CliError::CommandFailed {
        command: "tsc".to_string(),
        status: None,
    })?;

    if output.status.success() {
        return WalkDir::new(compiled.path())
            .into_iter()
            .try_for_each(|entry| {
                let entry = entry.map_err(|error| CliError::CommandFailed {
                    command: format!("read compiled TypeScript: {error}"),
                    status: None,
                })?;
                if !entry.file_type().is_file() {
                    return Ok(());
                }
                let relative = entry
                    .path()
                    .strip_prefix(compiled.path())
                    .expect("compiled file");
                let destination = output_dir.join(relative);
                if destination
                    .extension()
                    .is_some_and(|extension| extension == "js")
                    && destination.exists()
                {
                    return Ok(());
                }
                if destination
                    .extension()
                    .is_some_and(|extension| extension == "map")
                    && !destination
                        .with_extension("")
                        .with_extension("ts")
                        .is_file()
                {
                    return Ok(());
                }
                fs::rename(entry.path(), &destination).map_err(|source| CliError::WriteFailed {
                    path: destination,
                    source,
                })
            });
    }

    Err(CliError::CommandFailed {
        command: format!(
            "tsc failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        status: output.status.code(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{WasmArtifactPath, should_strip_debug};
    use crate::config::{Config, WasmProfile};

    fn parse_config(input: &str) -> Config {
        toml::from_str(input).expect("toml parse failed")
    }

    const BARE_CONFIG: &str = r#"
        [package]
        name = "demo"
        "#;

    #[test]
    fn release_strips_debug_by_default() {
        assert!(should_strip_debug(
            &parse_config(BARE_CONFIG),
            WasmProfile::Release
        ));
    }

    #[test]
    fn debug_profile_keeps_debug_sections_by_default() {
        // Optimization is release-only by default, and debug info rides with it.
        assert!(!should_strip_debug(
            &parse_config(BARE_CONFIG),
            WasmProfile::Debug
        ));
    }

    #[test]
    fn opting_out_keeps_debug_sections_in_release() {
        let config = parse_config(
            r#"
            [package]
            name = "demo"

            [targets.wasm.optimize]
            strip_debug = false
            "#,
        );

        assert!(!should_strip_debug(&config, WasmProfile::Release));
    }

    #[test]
    fn enabling_optimization_strips_debug_on_a_debug_profile() {
        let config = parse_config(
            r#"
            [package]
            name = "demo"

            [targets.wasm.optimize]
            enabled = true
            "#,
        );

        assert!(should_strip_debug(&config, WasmProfile::Debug));
    }

    #[test]
    fn disabling_optimization_keeps_debug_sections_in_release() {
        let config = parse_config(
            r#"
            [package]
            name = "demo"

            [targets.wasm.optimize]
            enabled = false
            "#,
        );

        assert!(!should_strip_debug(&config, WasmProfile::Release));
    }

    #[test]
    fn default_wasm_artifact_uses_cargo_target_directory() {
        let config = parse_config(
            r#"
            [package]
            name = "demo"
            "#,
        );

        let artifact_path = WasmArtifactPath::in_target_directory(
            &config,
            WasmProfile::Release,
            PathBuf::from("/tmp/boltffi-target"),
        )
        .into_path();

        assert_eq!(
            artifact_path,
            PathBuf::from("/tmp/boltffi-target")
                .join("wasm32-unknown-unknown")
                .join("release")
                .join("demo.wasm")
        );
    }

    #[test]
    fn configured_wasm_artifact_path_is_preserved() {
        let config = parse_config(
            r#"
            [package]
            name = "demo"

            [targets.wasm]
            artifact_path = "artifacts/demo.wasm"
            "#,
        );

        let artifact_path = WasmArtifactPath::in_target_directory(
            &config,
            WasmProfile::Release,
            PathBuf::from("/tmp/boltffi-target"),
        )
        .into_path();

        assert_eq!(artifact_path, PathBuf::from("artifacts/demo.wasm"));
    }
}

#[cfg(test)]
mod optimize_feature_tests {
    use super::{binaryen_flags_for, parse_target_features};

    #[test]
    fn parses_the_features_rustc_prints() {
        let cfg = "debug_assertions\ntarget_arch=\"wasm32\"\ntarget_feature=\"bulk-memory\"\ntarget_feature=\"sign-ext\"\ntarget_os=\"unknown\"\n";

        assert_eq!(parse_target_features(cfg), vec!["bulk-memory", "sign-ext"]);
    }

    #[test]
    fn bulk_memory_enables_both_halves() {
        // `memory.copy` sits behind the `-opt` half, and rustc emits it, so
        // enabling only `--enable-bulk-memory` still fails validation.
        assert_eq!(
            binaryen_flags_for("bulk-memory"),
            ["--enable-bulk-memory", "--enable-bulk-memory-opt"]
        );
    }

    #[test]
    fn drops_features_binaryen_does_not_know() {
        // Passing an unknown `--enable-*` makes wasm-opt exit, so an unmapped
        // feature has to be skipped rather than forwarded.
        assert!(binaryen_flags_for("some-future-proposal").is_empty());
    }

    #[test]
    fn maps_the_wasm32_defaults_binaryen_needs() {
        // What `cargo rustc --print cfg` reports for wasm32-unknown-unknown.
        // Without the bulk-memory half, a stripped module is rejected for using
        // `memory.copy`.
        let cfg = concat!(
            "target_feature=\"bulk-memory\"\n",
            "target_feature=\"multivalue\"\n",
            "target_feature=\"mutable-globals\"\n",
            "target_feature=\"nontrapping-fptoint\"\n",
            "target_feature=\"reference-types\"\n",
            "target_feature=\"sign-ext\"\n",
        );
        let flags: Vec<&str> = parse_target_features(cfg)
            .iter()
            .flat_map(|feature| binaryen_flags_for(feature))
            .copied()
            .collect();

        assert!(flags.contains(&"--enable-bulk-memory-opt"), "{flags:?}");
        assert!(flags.contains(&"--enable-sign-ext"), "{flags:?}");
        assert_eq!(flags.len(), 7, "{flags:?}");
    }

    #[test]
    fn a_non_default_feature_enabled_by_rustflags_is_mapped() {
        // The case a bare `rustc --print cfg` would miss: cargo reports
        // `simd128` when the build enables it, and it has to reach wasm-opt.
        let cfg = "target_feature=\"bulk-memory\"\ntarget_feature=\"simd128\"\n";
        let flags: Vec<&str> = parse_target_features(cfg)
            .iter()
            .flat_map(|feature| binaryen_flags_for(feature))
            .copied()
            .collect();

        assert!(flags.contains(&"--enable-simd"), "{flags:?}");
    }
}
