mod link;

use crate::build::{BuildOptions, Builder, OutputCallback, all_successful, failed_targets};
use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackAndroidOptions;
use crate::config::Config;
use crate::pack::PackError;
use crate::pack::symbols::{
    ensure_debug_symbols_profile_has_debuginfo, ensure_existing_debug_symbol_artifacts_are_usable,
};
use crate::reporter::Reporter;
use crate::target::Platform;

use super::{
    discover_built_libraries_for_targets, missing_built_libraries, print_cargo_line,
    resolve_build_cargo_args,
};

pub(crate) use self::link::{AndroidPackageLayout, AndroidPackager};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidBindingMode {
    Kotlin,
    KotlinMultiplatform,
}

pub(crate) fn pack_android(
    config: &Config,
    options: PackAndroidOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_android_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.android.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🤖", "Packing Android");

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile =
        crate::build::resolve_build_profile(options.execution.release, &build_cargo_args);
    let android_targets = config.android_targets();

    if !options.execution.no_build {
        if config.android_debug_symbols_enabled() {
            ensure_debug_symbols_profile_has_debuginfo(
                &build_cargo_args,
                &build_profile,
                "targets.android.debug_symbols",
                &android_targets
                    .iter()
                    .map(|target| target.triple().to_string())
                    .collect::<Vec<_>>(),
            )?;
        }
        let step = reporter.step("Building Android targets");
        build_android_targets(
            config,
            &android_targets,
            options.execution.release,
            &build_cargo_args,
            &step,
        )?;
        step.finish_success();
    }

    if options.execution.regenerate {
        let step = reporter.step("Generating Kotlin bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Kotlin,
                output: Some(config.android_kotlin_output()),
                experimental: false,
                ir: false,
                cargo_args: build_cargo_args.clone(),
            },
        )?;
        step.finish_success();

        let step = reporter.step("Generating C header");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Header,
                output: Some(config.android_header_output()),
                experimental: false,
                ir: false,
                cargo_args: build_cargo_args.clone(),
            },
        )?;
        step.finish_success();
    }

    let libraries = discover_built_libraries_for_targets(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
        &android_targets,
    )?;
    let android_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|library| library.target.platform() == Platform::Android)
        .collect();

    let missing_targets = missing_built_libraries(&android_targets, &android_libraries);
    if !missing_targets.is_empty() {
        return Err(PackError::MissingBuiltLibraries {
            platform: "Android".to_string(),
            targets: missing_targets,
        }
        .into());
    }

    if options.execution.no_build && config.android_debug_symbols_enabled() {
        ensure_existing_debug_symbol_artifacts_are_usable(
            &android_libraries
                .iter()
                .map(|library| library.path.clone())
                .collect::<Vec<_>>(),
            "targets.android.debug_symbols",
        )?;
    }

    let packager = AndroidPackager::new(config, android_libraries, build_profile.is_release_like());
    let step = reporter.step("Packaging jniLibs");
    packager.package()?;
    step.finish_success();

    Ok(())
}

pub(crate) fn build_android_targets(
    config: &Config,
    targets: &[crate::target::RustTarget],
    release: bool,
    build_cargo_args: &[String],
    step: &crate::reporter::Step,
) -> Result<()> {
    build_android_targets_for_package(
        config,
        targets,
        release,
        config.library_name(),
        build_cargo_args,
        step,
    )
}

pub(crate) fn build_android_targets_for_package(
    config: &Config,
    targets: &[crate::target::RustTarget],
    release: bool,
    package: impl Into<String>,
    build_cargo_args: &[String],
    step: &crate::reporter::Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        package: Some(package.into()),
        cargo_args: build_cargo_args.to_vec(),
        env: Vec::new(),
        on_output,
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_android(targets)?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}
