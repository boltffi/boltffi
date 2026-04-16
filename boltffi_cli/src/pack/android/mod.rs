mod link;

use crate::build::{
    BuildOptions, Builder, CargoBuildCommand, OutputCallback, all_successful, failed_targets,
};
use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackAndroidOptions;
use crate::config::Config;
use crate::pack::PackError;
use crate::reporter::Reporter;
use crate::target::Platform;

use super::{
    discover_built_libraries_for_targets, missing_built_libraries, print_cargo_line,
    resolve_build_cargo_args, resolve_cargo_build_command,
};

pub(crate) use self::link::AndroidPackager;

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

    let build_cargo_args = resolve_build_cargo_args(config, &options.cargo_args);
    let build_profile = crate::build::resolve_build_profile(options.release, &build_cargo_args);
    let android_targets = config.android_targets();
    let cargo_build_command =
        resolve_cargo_build_command(config, options.cargo_build_cmd.as_deref());

    if !options.no_build {
        let step = reporter.step("Building Android targets");
        build_android_targets(
            config,
            &android_targets,
            options.release,
            &build_cargo_args,
            cargo_build_command,
            &step,
        )?;
        step.finish_success();
    }

    if options.regenerate {
        let step = reporter.step("Generating Kotlin bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Kotlin,
                output: Some(config.android_kotlin_output()),
                experimental: false,
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

    let packager = AndroidPackager::new(config, android_libraries, build_profile.is_release_like());
    let step = reporter.step("Packaging jniLibs");
    packager.package()?;
    step.finish_success();

    Ok(())
}

fn build_android_targets(
    config: &Config,
    targets: &[crate::target::RustTarget],
    release: bool,
    build_cargo_args: &[String],
    cargo_build_command: Option<CargoBuildCommand>,
    step: &crate::reporter::Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
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
    let results = builder.build_android(targets)?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}
