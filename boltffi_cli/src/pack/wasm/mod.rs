mod npm;

use std::path::{Path, PathBuf};
use std::process::Command;

use boltffi_binding::BindingMetadataSurface;

use crate::build::{
    BindingExpansion, BuildOptions, BuildSelection, Builder, OutputCallback, all_successful,
    failed_targets,
};
use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackWasmOptions;
use crate::config::{Config, WasmOptimizeLevel, WasmOptimizeOnMissing, WasmProfile};
use crate::pack::PackError;
use crate::reporter::Reporter;

use super::{print_cargo_line, resolve_build_cargo_args};

use self::npm::{
    generate_wasm_loader_entrypoints, generate_wasm_package_json, generate_wasm_readme,
};

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

    if config.wasm_optimize_enabled(wasm_artifact_profile) {
        let step = reporter.step("Optimizing WASM binary");
        optimize_wasm_binary(config, &wasm_artifact_path)?;
        step.finish_success();
    }

    if options.execution.regenerate {
        let step = reporter.step("Generating TypeScript bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Typescript,
                output: Some(config.wasm_typescript_output()),
                experimental: false,
                ir: true,
                cargo_args: build_cargo_args.clone(),
            },
        )?;
        step.finish_success();
    }

    let npm_output = config.wasm_npm_output();
    std::fs::create_dir_all(&npm_output).map_err(|source| CliError::CreateDirectoryFailed {
        path: npm_output.clone(),
        source,
    })?;

    let module_name = config.wasm_typescript_module_name();
    let packaged_wasm_path = npm_output.join(format!("{}_bg.wasm", module_name));
    std::fs::copy(&wasm_artifact_path, &packaged_wasm_path).map_err(|source| {
        CliError::CopyFailed {
            from: wasm_artifact_path.clone(),
            to: packaged_wasm_path.clone(),
            source,
        }
    })?;

    let generated_typescript_source = config
        .wasm_typescript_output()
        .join(format!("{}.ts", module_name));
    if !generated_typescript_source.exists() {
        return Err(CliError::FileNotFound(generated_typescript_source));
    }

    let step = reporter.step("Transpiling TypeScript bindings");
    transpile_typescript_bundle(config, &generated_typescript_source, &npm_output)?;
    step.finish_success();

    let generated_node_typescript_source = config
        .wasm_typescript_output()
        .join(format!("{}_node.ts", module_name));
    if generated_node_typescript_source.exists() {
        let step = reporter.step("Transpiling Node.js bindings");
        transpile_typescript_bundle(config, &generated_node_typescript_source, &npm_output)?;
        step.finish_success();
    }

    let enabled_targets = config.wasm_npm_targets();
    let step = reporter.step("Generating WASM loader entrypoints");
    generate_wasm_loader_entrypoints(&module_name, &enabled_targets, &npm_output)?;
    step.finish_success();

    if config.wasm_npm_generate_package_json() {
        let step = reporter.step("Generating package.json");
        let package_json_path =
            generate_wasm_package_json(config, &module_name, &enabled_targets, &npm_output)?;
        step.finish_success_with(&format!("{}", package_json_path.display()));
    }

    if config.wasm_npm_generate_readme() {
        let step = reporter.step("Generating README.md");
        let readme_path =
            generate_wasm_readme(config, &module_name, &enabled_targets, &npm_output)?;
        step.finish_success_with(&format!("{}", readme_path.display()));
    }

    Ok(())
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

fn optimize_wasm_binary(config: &Config, wasm_path: &Path) -> Result<()> {
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

    if !config.wasm_optimize_strip_debug() {
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

fn transpile_typescript_bundle(
    config: &Config,
    source_file: &Path,
    output_dir: &Path,
) -> Result<()> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "npx", "tsc"]);
        command
    } else {
        Command::new("tsc")
    };
    command
        .arg(source_file)
        .arg("--target")
        .arg("ES2020")
        .arg("--module")
        .arg("ES2020")
        .arg("--moduleResolution")
        .arg("bundler")
        .arg("--declaration")
        .arg("--sourceMap")
        .arg(if config.wasm_source_map_enabled() {
            "true"
        } else {
            "false"
        })
        .arg("--skipLibCheck")
        .arg("--noEmitOnError")
        .arg("false")
        .arg("--outDir")
        .arg(output_dir);

    let output = command.output().map_err(|_| CliError::CommandFailed {
        command: "tsc".to_string(),
        status: None,
    })?;

    let module_name = config.wasm_typescript_module_name();
    let javascript_path = output_dir.join(format!("{}.js", module_name));
    let declarations_path = output_dir.join(format!("{}.d.ts", module_name));
    let emitted_outputs_exist = javascript_path.exists() && declarations_path.exists();

    if output.status.success() || emitted_outputs_exist {
        return Ok(());
    }

    Err(CliError::CommandFailed {
        command: format!("tsc failed: {}", String::from_utf8_lossy(&output.stderr)),
        status: output.status.code(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::WasmArtifactPath;
    use crate::config::{Config, WasmProfile};

    fn parse_config(input: &str) -> Config {
        toml::from_str(input).expect("toml parse failed")
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
