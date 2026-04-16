use crate::build::{
    BuildOptions, BuildResult, Builder, all_successful, count_successful, failed_targets,
    resolve_build_profile,
};
use crate::cli::Result;
use crate::config::Config;
use crate::pack::PackError;
use crate::pack::resolve_cargo_build_command;

pub enum BuildPlatform {
    Apple,
    Android,
    Wasm,
    All,
}

pub struct BuildCommandOptions {
    pub platform: BuildPlatform,
    pub release: bool,
    pub cargo_args: Vec<String>,
    /// See [`crate::build::CargoBuildCommand::parse`] for accepted values.
    pub cargo_build_cmd: Option<String>,
}

pub fn run_build(config: &Config, options: BuildCommandOptions) -> Result<Vec<BuildResult>> {
    let BuildCommandOptions {
        platform,
        release,
        cargo_args: cli_cargo_args,
        cargo_build_cmd,
    } = options;

    let cargo_args: Vec<String> = config
        .cargo_args_for_command("build")
        .into_iter()
        .chain(cli_cargo_args)
        .collect();

    let build_profile = resolve_build_profile(release, &cargo_args);

    let cargo_build_command = resolve_cargo_build_command(config, cargo_build_cmd.as_deref());

    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
        cargo_args,
        on_output: None,
        cargo_build_command,
    };

    let builder = Builder::new(config, build_options);

    let profile = build_profile.output_directory_name();

    let results = match platform {
        BuildPlatform::Apple => {
            if !config.is_apple_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for Apple ({})...", profile);
            builder.build_targets(&config.apple_targets())?
        }
        BuildPlatform::Android => {
            if !config.is_android_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for Android ({})...", profile);
            builder.build_android(&config.android_targets())?
        }
        BuildPlatform::Wasm => {
            if !config.is_wasm_enabled() {
                return Ok(Vec::new());
            }
            println!("Building for wasm ({})...", profile);
            builder.build_wasm_with_triple(config.wasm_triple())?
        }
        BuildPlatform::All => {
            println!("Building all targets ({})...", profile);
            let mut all_results = Vec::new();
            if config.is_apple_enabled() {
                all_results.extend(builder.build_targets(&config.apple_targets())?);
            }
            if config.is_android_enabled() {
                all_results.extend(builder.build_android(&config.android_targets())?);
            }
            if config.is_wasm_enabled() {
                all_results.extend(builder.build_wasm_with_triple(config.wasm_triple())?);
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
