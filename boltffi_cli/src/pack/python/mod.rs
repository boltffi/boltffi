mod build;
mod layout;
mod plan;
mod runtime;
mod wheel;

use crate::cli::{CliError, Result};
use crate::commands::generate::run_generate_python_with_manifest;
use crate::commands::pack::PackPythonOptions;
use crate::config::Config;
use crate::reporter::Reporter;

use self::build::PythonSharedLibraryBuilder;
use self::plan::PythonPackagingPlan;
use self::wheel::PythonWheelBuilder;

pub(crate) fn pack_python(
    config: &Config,
    options: PackPythonOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_python_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.python.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🐍", "Packing Python");

    let step = reporter.step("Preparing Python packaging");
    let plan = PythonPackagingPlan::from_config(
        config,
        options.execution.release,
        &options.execution.cargo_args,
        &options.python_interpreters,
    )?;
    let wheel_builder = PythonWheelBuilder::new(&plan)?;
    step.finish_success();

    if options.execution.regenerate {
        let step = reporter.step("Generating Python sources");
        run_generate_python_with_manifest(
            config,
            Some(plan.layout.root_directory.clone()),
            plan.cargo_context.manifest_path.clone(),
            plan.cargo_context.artifact_name.clone(),
            plan.cargo_context.cargo_command_args.clone(),
            plan.cargo_context.toolchain_selector.clone(),
        )?;
        step.finish_success();
    }

    let shared_library_builder = PythonSharedLibraryBuilder::new(&plan);
    let shared_library = if options.execution.no_build {
        let step = reporter.step("Reusing host Rust shared library");
        let shared_library = shared_library_builder.existing()?;
        step.finish_success_with(&format!("{}", shared_library.source_path.display()));
        shared_library
    } else {
        let step = reporter.step("Building host Rust shared library");
        let shared_library = shared_library_builder.build(&step)?;
        step.finish_success_with(&format!("{}", shared_library.source_path.display()));
        shared_library
    };

    let step = reporter.step("Building Python wheel");
    let built_wheel_matrix = wheel_builder.build(&shared_library, &step)?;
    let wheel_summary = built_wheel_matrix
        .wheels
        .iter()
        .map(|built_wheel| {
            format!(
                "{} => {}",
                built_wheel.interpreter,
                built_wheel.wheel_path.display()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    step.finish_success_with(&wheel_summary);

    reporter.finish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pack_python;
    use crate::cli::CliError;
    use crate::commands::pack::{PackExecutionOptions, PackPythonOptions};
    use crate::config::{CargoConfig, Config, PackageConfig, PythonConfig, TargetsConfig};
    use crate::reporter::{Reporter, Verbosity};

    fn reporter() -> Reporter {
        Reporter::new(Verbosity::Quiet)
    }

    fn config(enabled: bool, experimental: bool) -> Config {
        Config {
            experimental: experimental
                .then(|| "python".to_string())
                .into_iter()
                .collect(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "demo".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                python: PythonConfig {
                    enabled,
                    ..Default::default()
                },
                ..TargetsConfig::default()
            },
        }
    }

    #[test]
    fn rejects_pack_python_when_target_is_disabled() {
        let error = pack_python(
            &config(false, false),
            PackPythonOptions {
                execution: PackExecutionOptions {
                    release: false,
                    regenerate: false,
                    no_build: true,
                    cargo_args: Vec::new(),
                },
                python_interpreters: Vec::new(),
            },
            &reporter(),
        )
        .expect_err("expected disabled python target to fail");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "targets.python.enabled = false"
        ));
    }
}
