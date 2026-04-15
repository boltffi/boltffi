mod names;
mod spm;
mod xcframework;

use std::path::Path;

use crate::build::{
    BuildOptions, Builder, CargoBuildCommand, OutputCallback, all_successful, failed_targets,
};
use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackAppleOptions;
use crate::config::{Config, DebugSymbolsBundle, DebugSymbolsFormat, SpmDistribution, SpmLayout};
use crate::pack::PackError;
use crate::pack::symbols::{
    DebugSymbolArtifact, DebugSymbolArtifactKind, ensure_debug_symbols_profile_has_debuginfo,
    ensure_existing_debug_symbol_artifacts_are_usable, write_debug_symbols_zip,
};
use crate::reporter::Reporter;
use crate::target::{BuiltLibrary, Platform};

use super::{
    discover_built_libraries_for_targets, missing_built_libraries, print_cargo_line,
    resolve_build_cargo_args, resolve_cargo_build_command,
};

pub(crate) use self::spm::SpmPackageGenerator;
pub(crate) use self::xcframework::{XcframeworkBuilder, compute_checksum};

pub(crate) fn pack_apple(
    config: &Config,
    options: PackAppleOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_apple_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.apple.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🍎", "Packing Apple");

    if !config.apple_include_macos() {
        reporter.warning("macOS excluded (targets.apple.include_macos = false)");
    }

    if options.spm_only && options.xcframework_only {
        return Err(CliError::CommandFailed {
            command: "cannot combine --spm-only and --xcframework-only".to_string(),
            status: None,
        });
    }

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile =
        crate::build::resolve_build_profile(options.execution.release, &build_cargo_args);
    let apple_targets = config.apple_targets();

    let cargo_build_command =
        resolve_cargo_build_command(config, options.execution.cargo_build_cmd.as_deref());

    if !options.execution.no_build {
        if config.apple_debug_symbols_enabled() {
            ensure_debug_symbols_profile_has_debuginfo(
                &build_cargo_args,
                &build_profile,
                "targets.apple.debug_symbols",
                &apple_targets
                    .iter()
                    .map(|target| target.triple().to_string())
                    .collect::<Vec<_>>(),
            )?;
        }
        let step = reporter.step("Building Apple targets");
        build_apple_targets(
            config,
            &apple_targets,
            options.execution.release,
            &build_cargo_args,
            cargo_build_command,
            &step,
        )?;
        step.finish_success();
    }

    let layout = options.layout.unwrap_or_else(|| config.apple_spm_layout());
    let package_root = config.apple_spm_output();

    if options.execution.regenerate {
        let step = reporter.step("Generating Apple bindings");
        generate_apple_bindings(config, layout, &package_root)?;
        step.finish_success();
    }

    let libraries = discover_built_libraries_for_targets(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
        &apple_targets,
    )?;
    let apple_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|library| library.target.platform().is_apple())
        .collect();

    let missing_targets = missing_built_libraries(&apple_targets, &apple_libraries);
    if !missing_targets.is_empty() {
        return Err(PackError::MissingBuiltLibraries {
            platform: "Apple".to_string(),
            targets: missing_targets,
        }
        .into());
    }

    if options.execution.no_build && config.apple_debug_symbols_enabled() {
        ensure_existing_debug_symbol_artifacts_are_usable(
            &apple_libraries
                .iter()
                .map(|library| library.path.clone())
                .collect::<Vec<_>>(),
            "targets.apple.debug_symbols",
        )?;
    }

    let headers_dir = config.apple_header_output();
    if !headers_dir.exists() {
        return Err(CliError::FileNotFound(headers_dir));
    }

    let should_build_xcframework = !options.spm_only;
    let should_generate_spm = !options.xcframework_only;

    let xcframework_output = if should_build_xcframework {
        let step = reporter.step("Creating xcframework");
        let output = XcframeworkBuilder::new(config, apple_libraries.clone(), headers_dir.clone())
            .build_with_zip()?;
        step.finish_success();
        Some(output)
    } else {
        None
    };

    if config.apple_debug_symbols_enabled() {
        let step = reporter.step("Bundling Apple debug symbols");
        write_apple_debug_symbols(config, &apple_libraries)?;
        step.finish_success();
    }

    if should_generate_spm {
        let (checksum, version) = match config.apple_spm_distribution() {
            SpmDistribution::Local => (None, None),
            SpmDistribution::Remote => {
                let checksum = xcframework_output
                    .as_ref()
                    .and_then(|output| output.checksum.clone())
                    .map(Ok)
                    .unwrap_or_else(|| {
                        let step = reporter.step("Computing checksum");
                        let result = existing_xcframework_checksum(config);
                        step.finish_success();
                        result
                    })?;
                let version = options
                    .version
                    .or_else(detect_version)
                    .unwrap_or_else(|| "0.1.0".to_string());
                (Some(checksum), Some(version))
            }
        };

        if config.apple_spm_skip_package_swift() {
            reporter.warning("Skipping Package.swift (skip_package_swift = true)");
        } else {
            let generator = match config.apple_spm_distribution() {
                SpmDistribution::Local => SpmPackageGenerator::new_local(config, layout),
                SpmDistribution::Remote => {
                    let checksum = checksum.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires checksum".to_string(),
                        status: None,
                    })?;
                    let version = version.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires version".to_string(),
                        status: None,
                    })?;
                    SpmPackageGenerator::new_remote(config, checksum, version, layout)
                }
            };

            let step = reporter.step("Generating Package.swift");
            let package_path = generator.generate()?;
            step.finish_success_with(&format!("{}", package_path.display()));
        }
    }

    Ok(())
}

fn build_apple_targets(
    config: &Config,
    targets: &[crate::target::RustTarget],
    release: bool,
    build_cargo_args: &[String],
    cargo_build_command: Option<CargoBuildCommand>,
    step: &crate::reporter::Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| {
            print_cargo_line(line);
        }))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
        cargo_args: build_cargo_args.to_vec(),
        on_output,
        cargo_build_command,
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_targets(targets)?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}

fn generate_apple_bindings(config: &Config, layout: SpmLayout, package_root: &Path) -> Result<()> {
    let swift_output_directory = match layout {
        SpmLayout::Bundled => config
            .apple_spm_wrapper_sources()
            .map(|path| package_root.join(path).join("BoltFFI"))
            .unwrap_or_else(|| package_root.join("Sources").join("BoltFFI")),
        SpmLayout::FfiOnly => package_root.join("Sources").join("BoltFFI"),
        SpmLayout::Split => config.apple_swift_output().join("BoltFFI"),
    };

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Swift,
            output: Some(swift_output_directory),
            experimental: false,
        },
    )?;

    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Header,
            output: Some(config.apple_header_output()),
            experimental: false,
        },
    )?;

    Ok(())
}

fn existing_xcframework_checksum(config: &Config) -> Result<String> {
    let xcframework_zip = config
        .apple_xcframework_output()
        .join(format!("{}.xcframework.zip", config.xcframework_name()));

    if xcframework_zip.exists() {
        return compute_checksum(&xcframework_zip);
    }

    Err(CliError::FileNotFound(xcframework_zip))
}

fn detect_version() -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("version = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .map(|value| value.trim().trim_matches('"').to_string())
                })
        })
}

fn write_apple_debug_symbols(config: &Config, libraries: &[BuiltLibrary]) -> Result<()> {
    let archive_name = match config.apple_debug_symbols_format() {
        DebugSymbolsFormat::Zip => format!("{}.xcframework.symbols.zip", config.xcframework_name()),
    };
    let bundle = match config.apple_debug_symbols_bundle() {
        DebugSymbolsBundle::Unstripped => "unstripped",
    };
    let artifacts = libraries
        .iter()
        .map(|library| DebugSymbolArtifact {
            source_path: library.path.clone(),
            archive_path: std::path::PathBuf::from(apple_symbol_directory_name(
                library.target.platform(),
            ))
            .join(library.target.triple())
            .join(
                library
                    .path
                    .file_name()
                    .expect("built apple library should have a filename"),
            ),
            kind: DebugSymbolArtifactKind::Static,
            target_triple: Some(library.target.triple().to_string()),
            platform: Some(library.target.platform()),
            architecture: Some(library.target.architecture()),
            abi: None,
            host_target: None,
        })
        .collect::<Vec<_>>();

    write_debug_symbols_zip(
        &config.apple_debug_symbols_output(),
        &archive_name,
        "apple",
        bundle,
        &artifacts,
    )?;

    Ok(())
}

fn apple_symbol_directory_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Ios => "ios",
        Platform::IosSimulator => "ios-simulator",
        Platform::MacOs => "macos",
        Platform::Android | Platform::Wasm | Platform::Linux => unreachable!("non-apple platform"),
    }
}
