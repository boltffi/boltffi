use std::path::{Path, PathBuf};
use std::process::Command;

use askama::Template;

use crate::build::{
    CargoBuildProfile, OutputCallback, resolve_build_profile, run_command_streaming,
};
use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::commands::generate::run_generate_csharp_with_output_from_source_dir;
use crate::commands::pack::PackCSharpOptions;
use crate::config::Config;
use crate::pack::{PackError, format_command_for_log, print_cargo_line, resolve_build_cargo_args};
use crate::reporter::{Reporter, Step};
use crate::target::{CSharpRuntimeIdentifier, NativeHostPlatform};
use crate::toolchain::NativeHostToolchain;

pub(crate) fn pack_csharp(
    config: &Config,
    options: PackCSharpOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_csharp_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.csharp.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🔷", "Packing C#");

    let step = reporter.step("Preparing C# packaging");
    let plan = CSharpPackagingPlan::from_config(
        config,
        options.execution.release,
        &options.execution.cargo_args,
    )?;
    step.finish_success();

    sync_csharp_project(config, &plan)?;
    remove_stale_csharp_runtime_assets(&plan)?;

    if options.execution.regenerate {
        let step = reporter.step("Generating C# bindings");
        run_generate_csharp_with_output_from_source_dir(
            config,
            Some(plan.layout.source_directory.clone()),
            &plan.source_directory,
            &plan.artifact_name,
        )?;
        step.finish_success();
    }

    for packaging_target in &plan.packaging_targets {
        let action = if options.execution.no_build {
            "Reusing"
        } else {
            "Building"
        };
        let step = reporter.step(&format!(
            "{action} Rust cdylib for {}",
            packaging_target.runtime_identifier.canonical_name()
        ));
        let native_library = if options.execution.no_build {
            existing_csharp_native_library(packaging_target)?
        } else {
            build_csharp_native_library(packaging_target, &step)?
        };
        copy_csharp_native_asset(&plan.layout, packaging_target, &native_library)?;
        step.finish_success_with(&format!("{}", native_library.display()));
    }

    let step = reporter.step("Building NuGet package");
    let package_path = dotnet_pack(&plan, &step)?;
    step.finish_success_with(&format!("{}", package_path.display()));

    reporter.finish();
    Ok(())
}

#[derive(Debug)]
struct CSharpPackageLayout {
    root_directory: PathBuf,
    source_directory: PathBuf,
    package_output: PathBuf,
    project_path: PathBuf,
}

#[derive(Debug)]
struct CSharpCargoContext {
    rust_target_triple: String,
    release: bool,
    build_profile: CargoBuildProfile,
    artifact_name: String,
    cargo_manifest_path: PathBuf,
    package_selector: Option<String>,
    target_directory: PathBuf,
    cargo_command_args: Vec<String>,
    toolchain_selector: Option<String>,
}

#[derive(Debug)]
struct CSharpPackagingTarget {
    runtime_identifier: CSharpRuntimeIdentifier,
    host_target: NativeHostPlatform,
    cargo_context: CSharpCargoContext,
    toolchain: NativeHostToolchain,
}

#[derive(Debug)]
struct CSharpPackagingPlan {
    package_id: String,
    package_version: String,
    target_framework: String,
    artifact_name: String,
    source_directory: PathBuf,
    layout: CSharpPackageLayout,
    packaging_targets: Vec<CSharpPackagingTarget>,
}

impl CSharpCargoContext {
    fn artifact_directory(&self) -> PathBuf {
        self.target_directory
            .join(&self.rust_target_triple)
            .join(self.build_profile.output_directory_name())
    }
}

impl CSharpPackagingPlan {
    fn from_config(config: &Config, release: bool, cargo_args: &[String]) -> Result<Self> {
        let build_cargo_args = resolve_build_cargo_args(config, cargo_args);
        let cargo = Cargo::current(&build_cargo_args)?;
        ensure_csharp_pack_cargo_args_supported(&cargo)?;
        let metadata = cargo.metadata()?;
        let cargo_manifest_path = cargo.manifest_path()?;
        let package_selector =
            cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
        let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
        let artifact_name = package
            .resolve_library_artifact_name(&config.crate_artifact_name(), &cargo_manifest_path)?
            .to_string();
        let library_target =
            package.resolve_library_target(&artifact_name, &cargo_manifest_path)?;
        if !library_target.builds_cdylib() {
            return Err(CliError::CommandFailed {
                command: "pack csharp requires the selected Rust library target to build a cdylib"
                    .to_string(),
                status: None,
            });
        }

        let source_directory = package
            .manifest_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| CliError::CommandFailed {
                command:
                    "could not resolve selected Cargo package source directory for C# generation"
                        .to_string(),
                status: None,
            })?;

        let runtime_identifiers =
            config
                .csharp_runtime_identifiers()
                .map_err(|message| CliError::CommandFailed {
                    command: message,
                    status: None,
                })?;
        let current_host = NativeHostPlatform::current().ok_or_else(|| CliError::CommandFailed {
            command:
                "C# packaging is only supported on osx-arm64, osx-x64, linux-x64, linux-arm64, and win-x64 hosts".to_string(),
            status: None,
        })?;
        let build_profile = resolve_build_profile(release, &build_cargo_args);
        let toolchain_selector = cargo.toolchain_selector().map(str::to_owned);
        let cargo_command_args = cargo.probe_command_arguments();

        let packaging_targets = runtime_identifiers
            .iter()
            .copied()
            .map(|runtime_identifier| {
                let host_target = runtime_identifier.native_host_platform();
                let toolchain = NativeHostToolchain::discover_csharp(
                    toolchain_selector.as_deref(),
                    &cargo_command_args,
                    host_target,
                    current_host,
                )?;
                let cargo_context = CSharpCargoContext {
                    rust_target_triple: toolchain.rust_target_triple().to_string(),
                    release,
                    build_profile: build_profile.clone(),
                    artifact_name: artifact_name.clone(),
                    cargo_manifest_path: cargo_manifest_path.clone(),
                    package_selector: package_selector.clone(),
                    target_directory: metadata.target_directory.clone(),
                    cargo_command_args: cargo_command_args.clone(),
                    toolchain_selector: toolchain_selector.clone(),
                };
                Ok(CSharpPackagingTarget {
                    runtime_identifier,
                    host_target,
                    cargo_context,
                    toolchain,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let root_directory = config.csharp_output();
        let layout = CSharpPackageLayout {
            source_directory: root_directory.join("src"),
            package_output: config.csharp_package_output(),
            project_path: root_directory.join("BoltFFI.CSharp.csproj"),
            root_directory,
        };

        Ok(Self {
            package_id: config.csharp_package_id(),
            package_version: config
                .package_version()
                .unwrap_or_else(|| "0.1.0".to_string()),
            target_framework: config.csharp_target_framework(),
            artifact_name,
            source_directory,
            layout,
            packaging_targets,
        })
    }
}

fn ensure_csharp_pack_cargo_args_supported(cargo: &Cargo) -> Result<()> {
    if let Some(target_selector) = cargo.target_selector() {
        return Err(CliError::CommandFailed {
            command: format!(
                "pack csharp resolves native assets from targets.csharp.runtime_identifiers; remove cargo --target '{}'",
                target_selector
            ),
            status: None,
        });
    }

    Ok(())
}

fn build_csharp_native_library(
    packaging_target: &CSharpPackagingTarget,
    step: &Step,
) -> Result<PathBuf> {
    let cargo_context = &packaging_target.cargo_context;
    let verbose = step.is_verbose();
    let on_output: Option<OutputCallback> = Some(Box::new(move |line: &str| {
        if verbose {
            print_cargo_line(line);
        }
    }));

    let crate_directory = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;
    let mut command = Command::new("cargo");
    command.current_dir(crate_directory);

    if let Some(toolchain_selector) = cargo_context.toolchain_selector.as_deref() {
        command.arg(toolchain_selector);
    }

    command
        .arg("build")
        .arg("--target")
        .arg(&cargo_context.rust_target_triple)
        .arg("--manifest-path")
        .arg(&cargo_context.cargo_manifest_path);
    if let Some(package_selector) = cargo_context.package_selector.as_deref() {
        command.arg("-p").arg(package_selector);
    }
    if cargo_context.release {
        command.arg("--release");
    }
    command.args(&cargo_context.cargo_command_args);
    packaging_target
        .toolchain
        .configure_cargo_build(&mut command);

    if step.is_verbose() {
        crate::pack::print_verbose_detail(&format!(
            "Cargo build command: {}",
            format_command_for_log(&command)
        ));
    }

    if !run_command_streaming(&mut command, on_output.as_ref()) {
        return Err(PackError::BuildFailed {
            targets: vec![
                packaging_target
                    .runtime_identifier
                    .canonical_name()
                    .to_string(),
            ],
        }
        .into());
    }

    existing_csharp_native_library(packaging_target)
}

fn existing_csharp_native_library(packaging_target: &CSharpPackagingTarget) -> Result<PathBuf> {
    let cargo_context = &packaging_target.cargo_context;
    let native_library = cargo_context.artifact_directory().join(
        packaging_target
            .host_target
            .shared_library_filename(&cargo_context.artifact_name),
    );

    if native_library.exists() {
        return Ok(native_library);
    }

    Err(CliError::FileNotFound(native_library))
}

fn copy_csharp_native_asset(
    layout: &CSharpPackageLayout,
    packaging_target: &CSharpPackagingTarget,
    native_library: &Path,
) -> Result<PathBuf> {
    let output_directory = layout
        .root_directory
        .join("runtimes")
        .join(packaging_target.runtime_identifier.canonical_name())
        .join("native");
    std::fs::create_dir_all(&output_directory).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: output_directory.clone(),
            source,
        }
    })?;

    let output_path = output_directory.join(
        native_library
            .file_name()
            .expect("native library should have a filename"),
    );
    std::fs::copy(native_library, &output_path).map_err(|source| CliError::CopyFailed {
        from: native_library.to_path_buf(),
        to: output_path.clone(),
        source,
    })?;

    Ok(output_path)
}

fn remove_stale_csharp_runtime_assets(plan: &CSharpPackagingPlan) -> Result<()> {
    let requested = plan
        .packaging_targets
        .iter()
        .map(|target| target.runtime_identifier)
        .collect::<std::collections::HashSet<_>>();
    let runtimes_root = plan.layout.root_directory.join("runtimes");

    for runtime_identifier in CSharpRuntimeIdentifier::EXPLICIT_TARGETS {
        if requested.contains(runtime_identifier) {
            continue;
        }

        let runtime_dir = runtimes_root.join(runtime_identifier.canonical_name());
        match std::fs::remove_dir_all(&runtime_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(CliError::WriteFailed {
                    path: runtime_dir,
                    source,
                });
            }
        }
    }

    Ok(())
}

fn sync_csharp_project(config: &Config, plan: &CSharpPackagingPlan) -> Result<()> {
    std::fs::create_dir_all(&plan.layout.root_directory).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: plan.layout.root_directory.clone(),
            source,
        }
    })?;
    std::fs::create_dir_all(&plan.layout.source_directory).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: plan.layout.source_directory.clone(),
            source,
        }
    })?;
    std::fs::create_dir_all(&plan.layout.package_output).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: plan.layout.package_output.clone(),
            source,
        }
    })?;

    let project = render_csharp_project(config, plan)?;
    std::fs::write(&plan.layout.project_path, project).map_err(|source| CliError::WriteFailed {
        path: plan.layout.project_path.clone(),
        source,
    })
}

#[derive(Template)]
#[template(path = "BoltFFI.CSharp.csproj.xml", escape = "html")]
struct CSharpProjectTemplate<'a> {
    target_framework: &'a str,
    runtime_identifiers: &'a str,
    package_id: &'a str,
    package_version: &'a str,
    has_description: bool,
    description: &'a str,
    has_license: bool,
    license: &'a str,
    has_repository: bool,
    repository: &'a str,
}

fn render_csharp_project(config: &Config, plan: &CSharpPackagingPlan) -> Result<String> {
    let runtime_identifiers = plan
        .packaging_targets
        .iter()
        .map(|target| target.runtime_identifier.canonical_name())
        .collect::<Vec<_>>()
        .join(";");
    let description = config.package.description.as_deref().unwrap_or_default();
    let license = config.package_license();
    let repository = config.package_repository();

    CSharpProjectTemplate {
        target_framework: &plan.target_framework,
        runtime_identifiers: &runtime_identifiers,
        package_id: &plan.package_id,
        package_version: &plan.package_version,
        has_description: config.package.description.is_some(),
        description,
        has_license: license.is_some(),
        license: license.as_deref().unwrap_or_default(),
        has_repository: repository.is_some(),
        repository: repository.as_deref().unwrap_or_default(),
    }
    .render()
    .map_err(|source| CliError::CommandFailed {
        command: format!("render C# project template: {source}"),
        status: None,
    })
}

fn dotnet_pack(plan: &CSharpPackagingPlan, step: &Step) -> Result<PathBuf> {
    let mut command = Command::new("dotnet");
    command
        .arg("pack")
        .arg(&plan.layout.project_path)
        .arg("--configuration")
        .arg(
            if plan
                .packaging_targets
                .iter()
                .any(|target| target.cargo_context.build_profile.is_release_like())
            {
                "Release"
            } else {
                "Debug"
            },
        )
        .arg("--output")
        .arg(&plan.layout.package_output)
        .arg("--ignore-failed-sources")
        .arg("-p:NuGetAudit=false")
        // Keep RuntimeIdentifiers in the generated project, but avoid restoring
        // framework runtime packs while building this library-only package.
        .arg("-p:RuntimeIdentifiers=")
        .arg("-p:RuntimeIdentifier=")
        .arg("--nologo");

    if step.is_verbose() {
        crate::pack::print_verbose_detail(&format!(
            "dotnet pack command: {}",
            format_command_for_log(&command)
        ));
    }

    let status = command.status().map_err(|source| CliError::CommandFailed {
        command: format!("dotnet pack: {source}"),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "dotnet pack".to_string(),
            status: status.code(),
        });
    }

    Ok(plan.layout.package_output.join(format!(
        "{}.{}.nupkg",
        plan.package_id, plan.package_version
    )))
}

#[cfg(test)]
mod tests {
    use super::{CSharpPackageLayout, CSharpPackagingPlan, pack_csharp, render_csharp_project};
    use crate::cli::CliError;
    use crate::commands::pack::{PackCSharpOptions, PackExecutionOptions};
    use crate::config::{CSharpConfig, CargoConfig, Config, PackageConfig, TargetsConfig};
    use crate::reporter::{Reporter, Verbosity};
    use crate::target::CSharpRuntimeIdentifier;
    use std::path::PathBuf;

    fn config() -> Config {
        Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "demo".to_string(),
                crate_name: None,
                version: Some("1.2.3".to_string()),
                description: Some("Demo <runtime>".to_string()),
                license: Some("MIT".to_string()),
                repository: Some("https://example.com/demo".to_string()),
            },
            targets: TargetsConfig {
                csharp: CSharpConfig {
                    enabled: true,
                    runtime_identifiers: Some(vec![
                        CSharpRuntimeIdentifier::OsxArm64,
                        CSharpRuntimeIdentifier::LinuxX64,
                    ]),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    fn options() -> PackCSharpOptions {
        PackCSharpOptions {
            execution: PackExecutionOptions {
                release: false,
                regenerate: true,
                no_build: false,
                cargo_args: Vec::new(),
            },
        }
    }

    #[test]
    fn csharp_runtime_identifier_maps_native_assets() {
        assert_eq!(
            CSharpRuntimeIdentifier::OsxArm64.native_library_filename("demo"),
            "libdemo.dylib"
        );
        assert_eq!(
            CSharpRuntimeIdentifier::LinuxX64.native_library_filename("demo"),
            "libdemo.so"
        );
        assert_eq!(
            CSharpRuntimeIdentifier::WinX64.native_library_filename("demo"),
            "demo.dll"
        );
    }

    #[test]
    fn csharp_project_includes_packable_native_assets() {
        let plan = CSharpPackagingPlan {
            package_id: "Demo.Runtime".to_string(),
            package_version: "1.2.3".to_string(),
            target_framework: "net10.0".to_string(),
            artifact_name: "demo".to_string(),
            source_directory: PathBuf::from("/tmp/demo"),
            layout: CSharpPackageLayout {
                root_directory: PathBuf::from("/tmp/dist/csharp"),
                source_directory: PathBuf::from("/tmp/dist/csharp/src"),
                package_output: PathBuf::from("/tmp/dist/csharp/packages"),
                project_path: PathBuf::from("/tmp/dist/csharp/BoltFFI.CSharp.csproj"),
            },
            packaging_targets: Vec::new(),
        };

        let project = render_csharp_project(&config(), &plan).expect("C# project should render");

        assert!(project.contains("<TargetFramework>net10.0</TargetFramework>"));
        assert!(project.contains("<RuntimeIdentifiers></RuntimeIdentifiers>"));
        assert!(project.contains("<PackageId>Demo.Runtime</PackageId>"));
        assert!(project.contains("<Version>1.2.3</Version>"));
        assert!(project.contains("<AllowUnsafeBlocks>true</AllowUnsafeBlocks>"));
        assert!(project.contains(r#"<Compile Include="src/**/*.cs" />"#));
        assert!(
            project.contains(
                r#"<None Include="runtimes/**/*" Pack="true" PackagePath="%(Identity)" />"#
            )
        );
        assert!(project.contains("<Description>Demo &#60;runtime&#62;</Description>"));
        assert!(project.contains("<RepositoryUrl>https://example.com/demo</RepositoryUrl>"));
    }

    #[test]
    fn rejects_pack_csharp_when_target_is_disabled() {
        let mut config = config();
        config.targets.csharp.enabled = false;

        let error = pack_csharp(&config, options(), &Reporter::new(Verbosity::Quiet))
            .expect_err("disabled csharp target should fail before packaging");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, .. }
                if command.contains("targets.csharp.enabled = false")
        ));
    }
}
