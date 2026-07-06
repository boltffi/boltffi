use std::ffi::OsString;

use crate::build::{
    BindingExpansion, BuildOptions, BuildResult, Builder, all_successful, count_successful,
    failed_targets, resolve_build_profile,
};
use crate::cli::Result;
use crate::config::Config;
use crate::pack::PackError;

pub enum BuildPlatform {
    Apple,
    Android,
    Wasm,
    Dart,
    All,
}

pub struct BuildCommandOptions {
    pub platform: BuildPlatform,
    pub release: bool,
    pub cargo_args: Vec<String>,
}

pub fn run_build(config: &Config, options: BuildCommandOptions) -> Result<Vec<BuildResult>> {
    let BuildCommandOptions {
        platform,
        release,
        cargo_args: cli_cargo_args,
    } = options;

    let cargo_args: Vec<String> = config
        .cargo_args_for_command("build")
        .into_iter()
        .chain(cli_cargo_args)
        .collect();

    let build_profile = resolve_build_profile(release, &cargo_args);

    let profile = build_profile.output_directory_name();

    let results = match platform {
        BuildPlatform::Apple => {
            if !config.is_apple_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for Apple ({})...", profile);
            apple_builder(config, release, cargo_args.clone())?
                .build_targets(&config.apple_targets())?
        }
        BuildPlatform::Android => {
            if !config.is_android_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for Android ({})...", profile);
            plain_builder(config, release, cargo_args.clone())
                .build_android(&config.android_targets())?
        }
        BuildPlatform::Wasm => {
            if !config.is_wasm_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for wasm ({})...", profile);
            plain_builder(config, release, cargo_args.clone())
                .build_wasm_with_triple(config.wasm_triple())?
        }
        BuildPlatform::Dart => {
            if !config.is_dart_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for dart ({})...", profile);
            plain_builder(config, release, cargo_args.clone())
                .build_targets(&config.dart_targets())?
        }
        BuildPlatform::All => {
            println!("Building all targets ({})...", profile);
            let mut all_results = Vec::new();
            if config.is_apple_enabled() {
                all_results.extend(
                    apple_builder(config, release, cargo_args.clone())?
                        .build_targets(&config.apple_targets())?,
                );
            }
            if config.is_android_enabled() {
                all_results.extend(
                    plain_builder(config, release, cargo_args.clone())
                        .build_android(&config.android_targets())?,
                );
            }
            if config.is_wasm_enabled() {
                all_results.extend(
                    plain_builder(config, release, cargo_args.clone())
                        .build_wasm_with_triple(config.wasm_triple())?,
                );
            }
            if config.is_dart_enabled() {
                all_results.extend(
                    plain_builder(config, release, cargo_args.clone())
                        .build_targets(&config.dart_targets())?,
                );
            }
            all_results
        }
    };

    if results.is_empty() {
        println!("No enabled targets matched the requested platform");
        return Ok(results);
    }

    print_build_results(&results);

    if all_successful(&results) {
        Ok(results)
    } else {
        Err(PackError::BuildFailed {
            targets: failed_targets(&results),
        }
        .into())
    }
}

fn apple_builder(config: &Config, release: bool, cargo_args: Vec<String>) -> Result<Builder<'_>> {
    let expansion = BindingExpansion::resolve(config, &cargo_args)?;
    Ok(Builder::new(
        config,
        build_options(release, cargo_args, expansion.env()?),
    ))
}

fn plain_builder(config: &Config, release: bool, cargo_args: Vec<String>) -> Builder<'_> {
    Builder::new(config, build_options(release, cargo_args, Vec::new()))
}

fn build_options(
    release: bool,
    cargo_args: Vec<String>,
    env: Vec<(OsString, OsString)>,
) -> BuildOptions {
    BuildOptions {
        release,
        package: None,
        cargo_args,
        env,
        on_output: None,
    }
}

fn print_build_results(results: &[BuildResult]) {
    println!();

    results.iter().for_each(|result| {
        let icon = if result.success { "[ok]" } else { "[failed]" };
        println!("  {} {}", icon, result.triple);
    });

    println!();

    let success_count = count_successful(results);
    let total = results.len();

    if all_successful(results) {
        println!("Built {}/{} targets successfully", success_count, total);
    } else {
        println!(
            "Built {}/{} targets ({} failed)",
            success_count,
            total,
            total - success_count
        );
        println!();
        println!("Failed targets:");
        failed_targets(results).iter().for_each(|triple| {
            println!("  - {}", triple);
        });
    }
}
