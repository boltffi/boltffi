use std::path::PathBuf;

use boltffi_bindgen::cargo::LibraryCargoArgs;
use boltffi_bindgen::target::Target;

use crate::build::{BindingExpansion, CargoBuildProfile};
use crate::cargo::{Cargo, SelectedLibrary};
use crate::cli::{CliError, Result};
use crate::commands::generate::java::TargetGeneration;
use crate::commands::generate::run_generate_java_with_generations;
use crate::commands::pack::{PackExecutionOptions, PackJavaOptions};
use crate::config::{Config, DebugSymbolsBundle, DebugSymbolsFormat};
use crate::pack::resolve_build_cargo_args;
use crate::pack::symbols::{
    DebugSymbolArtifact, DebugSymbolArtifactKind, ensure_debug_symbols_profile_has_debuginfo,
    write_debug_symbols_zip,
};
use crate::reporter::Reporter;
use crate::target::JavaHostTarget;
use crate::toolchain::NativeHostToolchain;

use super::link::{
    build_jvm_native_library, compile_jni_library, resolve_jni_include_directories,
    validate_desktop_jni_symbol_stripping_for,
};
use super::outputs::{
    remove_stale_flat_jvm_outputs_if_current_host_unrequested,
    remove_stale_requested_jvm_shared_library_copies_after_success,
    remove_stale_structured_jvm_outputs,
};

#[derive(Debug, Clone)]
pub(crate) struct JvmCargoContext {
    pub(crate) host_target: JavaHostTarget,
    pub(crate) rust_target_triple: String,
    pub(crate) release: bool,
    pub(crate) build_profile: CargoBuildProfile,
    pub(crate) library: SelectedLibrary,
    pub(crate) target_directory: PathBuf,
    pub(crate) cargo_command_args: LibraryCargoArgs,
    pub(crate) toolchain_selector: Option<String>,
}

pub(crate) struct JvmPackagingTarget {
    pub(crate) cargo_context: JvmCargoContext,
    pub(crate) toolchain: NativeHostToolchain,
}

pub(crate) struct PreparedJvmPackaging {
    pub(crate) host_targets: Vec<JavaHostTarget>,
    pub(crate) packaging_targets: Vec<JvmPackagingTarget>,
}

pub(crate) struct JavaPackPlan {
    execution: PackExecutionOptions,
    packaging: PreparedJvmPackaging,
    expansion: BindingExpansion,
}

impl JvmCargoContext {
    pub(crate) fn artifact_directory(&self) -> PathBuf {
        self.target_directory
            .join(&self.rust_target_triple)
            .join(self.build_profile.output_directory_name())
    }
}

impl JvmPackagingTarget {
    fn binding_generation(&self, expansion: &BindingExpansion) -> TargetGeneration {
        let cargo_context = &self.cargo_context;
        let cargo_args = self
            .cargo_context
            .release
            .then(|| "--release".to_string())
            .into_iter()
            .chain(cargo_context.cargo_command_args.iter().cloned());
        let generation = expansion
            .generation()
            .triple(cargo_context.rust_target_triple.clone())
            .cargo_args(cargo_args)
            .cargo_environment(self.toolchain.cargo_environment())
            .cargo_toolchain_selector(cargo_context.toolchain_selector.clone());
        TargetGeneration::new(
            format!(
                "{} [{}]",
                cargo_context.host_target.canonical_name(),
                cargo_context.rust_target_triple
            ),
            generation,
        )
    }
}

pub(crate) fn check_java_packaging_prereqs(
    config: &Config,
    release: bool,
    cargo_args: &[String],
) -> Result<()> {
    prepare_java_packaging(config, release, cargo_args, None, "pack java").map(|_| ())
}

pub(crate) fn pack_java(
    config: &Config,
    options: PackJavaOptions,
    reporter: &Reporter,
) -> Result<()> {
    reporter.section("☕", "Packing Java");
    let step = reporter.step("Validating JVM toolchains");
    let plan = prepare_java_pack(config, options)?;
    step.finish_success();
    execute_java_pack(config, plan, reporter)
}

pub(crate) fn prepare_java_pack(config: &Config, options: PackJavaOptions) -> Result<JavaPackPlan> {
    if !config.is_java_jvm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.java.jvm.enabled = false".to_string(),
            status: None,
        });
    }
    ensure_java_ir_regeneration(options.execution.regenerate)?;
    ensure_java_no_build_supported(
        config,
        options.execution.no_build,
        options.experimental,
        "pack java",
    )?;
    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let expansion = BindingExpansion::resolve(config, &build_cargo_args)?;
    let packaging = prepare_java_packaging(
        config,
        options.execution.release,
        &options.execution.cargo_args,
        Some(&expansion),
        "pack java",
    )?;
    Ok(JavaPackPlan {
        execution: options.execution,
        packaging,
        expansion,
    })
}

pub(crate) fn pack_prepared_java(
    config: &Config,
    plan: JavaPackPlan,
    reporter: &Reporter,
) -> Result<()> {
    reporter.section("☕", "Packing Java");
    execute_java_pack(config, plan, reporter)
}

fn execute_java_pack(config: &Config, plan: JavaPackPlan, reporter: &Reporter) -> Result<()> {
    let JavaPackPlan {
        execution,
        packaging:
            PreparedJvmPackaging {
                host_targets,
                packaging_targets,
            },
        expansion,
    } = plan;

    if execution.regenerate {
        let step = reporter.step("Generating Java bindings through Binding IR");
        run_generate_java_with_generations(
            config,
            Some(config.java_jvm_output()),
            expansion.artifact_name(),
            packaging_targets
                .iter()
                .map(|target| target.binding_generation(&expansion)),
        )?;
        step.finish_success();
    }

    let packaged_outputs = packaging_targets
        .iter()
        .map(|packaging_target| {
            let host_target = packaging_target.cargo_context.host_target;
            let step = reporter.step(&format!(
                "Building Rust library for {}",
                host_target.canonical_name()
            ));
            let build_artifacts = build_jvm_native_library(
                packaging_target,
                execution.release,
                Some(&expansion),
                &step,
            )?;
            step.finish_success();

            let step = reporter.step(&format!(
                "Compiling JNI library for {}",
                host_target.canonical_name()
            ));
            let output = compile_jni_library(config, packaging_target, &build_artifacts, &step)?;
            step.finish_success();
            Ok(output)
        })
        .collect::<Result<Vec<_>>>()?;

    let artifact_name = selected_jvm_package_artifact_name(&packaging_targets)?;
    if config.java_jvm_debug_symbols_archive_enabled() {
        let step = reporter.step("Bundling JVM debug symbols");
        write_jvm_debug_symbols(config, artifact_name, &packaged_outputs)?;
        step.finish_success();
    }
    remove_stale_requested_jvm_shared_library_copies_after_success(
        &config.java_jvm_output(),
        &packaged_outputs,
        artifact_name,
    )?;
    remove_stale_structured_jvm_outputs(&config.java_jvm_output().join("native"), &host_targets)?;
    remove_stale_flat_jvm_outputs_if_current_host_unrequested(
        &config.java_jvm_output(),
        JavaHostTarget::current(),
        &host_targets,
        artifact_name,
    )?;

    reporter.finish();
    Ok(())
}

fn prepare_java_packaging(
    config: &Config,
    release: bool,
    cargo_args: &[String],
    binding_expansion: Option<&BindingExpansion>,
    command_name: &str,
) -> Result<PreparedJvmPackaging> {
    let prepared = prepare_jvm_packaging_matrix(
        config,
        release,
        cargo_args,
        config.java_jvm_strip_symbols(),
        command_name,
        binding_expansion,
    )?;
    if config.java_jvm_debug_symbols_enabled() {
        let build_cargo_args = resolve_build_cargo_args(config, cargo_args);
        ensure_debug_symbols_profile_has_debuginfo(
            &build_cargo_args,
            &prepared.packaging_targets[0].cargo_context.build_profile,
            "targets.java.jvm.debug_symbols",
            &prepared
                .packaging_targets
                .iter()
                .map(|target| target.cargo_context.rust_target_triple.clone())
                .collect::<Vec<_>>(),
        )?;
    }

    Ok(prepared)
}

pub(crate) fn prepare_kmp_jvm_packaging(
    config: &Config,
    release: bool,
    cargo_args: &[String],
    binding_expansion: &BindingExpansion,
) -> Result<PreparedJvmPackaging> {
    prepare_jvm_packaging_matrix(
        config,
        release,
        cargo_args,
        false,
        "pack kmp",
        Some(binding_expansion),
    )
}

pub(crate) fn prepare_android_kotlin_jvm_packaging(
    config: &Config,
    release: bool,
    cargo_args: &[String],
    binding_expansion: &BindingExpansion,
) -> Result<PreparedJvmPackaging> {
    prepare_jvm_packaging_matrix(
        config,
        release,
        cargo_args,
        false,
        "pack android",
        Some(binding_expansion),
    )
}

fn prepare_jvm_packaging_matrix(
    config: &Config,
    release: bool,
    cargo_args: &[String],
    strip_symbols: bool,
    command_name: &str,
    binding_expansion: Option<&BindingExpansion>,
) -> Result<PreparedJvmPackaging> {
    let build_cargo_args = resolve_build_cargo_args(config, cargo_args);
    ensure_jvm_pack_cargo_args_supported(&build_cargo_args, command_name)?;
    let build_profile = crate::build::resolve_build_profile(release, &build_cargo_args);
    let host_targets = resolve_java_host_targets_for_packaging(config)?;
    if host_targets.is_empty() {
        return Err(CliError::CommandFailed {
            command: "targets.java.jvm.host_targets must be non-empty when provided".to_string(),
            status: None,
        });
    }
    let packaging_targets = resolve_jvm_packaging_targets(
        config,
        &build_cargo_args,
        release,
        build_profile,
        &host_targets,
        strip_symbols,
        binding_expansion,
    )?;

    Ok(PreparedJvmPackaging {
        host_targets,
        packaging_targets,
    })
}

pub(crate) fn ensure_java_no_build_supported(
    config: &Config,
    no_build: bool,
    experimental: bool,
    command_name: &str,
) -> Result<()> {
    if no_build && config.should_process(Target::Java, experimental) {
        return Err(CliError::CommandFailed {
            command: format!(
                "{command_name} --no-build is unsupported in Phase 4 when JVM packaging is enabled; rerun without --no-build"
            ),
            status: None,
        });
    }

    Ok(())
}

fn ensure_java_ir_regeneration(regenerate: bool) -> Result<()> {
    if !regenerate {
        return Err(CliError::CommandFailed {
            command: "pack java requires regenerated bindings until generated Binding IR provenance can be validated; remove '--regenerate false'"
                .to_string(),
            status: None,
        });
    }

    Ok(())
}

fn ensure_jvm_pack_cargo_args_supported(cargo_args: &[String], command_name: &str) -> Result<()> {
    if let Some(target_selector) = Cargo::current(cargo_args)?.target_selector() {
        return Err(CliError::CommandFailed {
            command: format!(
                "{command_name} resolves desktop targets from targets.java.jvm.host_targets; remove cargo --target '{}'",
                target_selector
            ),
            status: None,
        });
    }

    Ok(())
}

pub(crate) fn selected_jvm_package_artifact_name(
    packaging_targets: &[JvmPackagingTarget],
) -> Result<&str> {
    packaging_targets
        .first()
        .map(|target| target.cargo_context.library.artifact_name())
        .ok_or_else(|| CliError::CommandFailed {
            command: "could not resolve selected Cargo package artifact name for JVM generation"
                .to_string(),
            status: None,
        })
}

fn write_jvm_debug_symbols(
    config: &Config,
    artifact_name: &str,
    outputs: &[super::link::JvmPackagedNativeOutput],
) -> Result<()> {
    let mut artifacts = Vec::with_capacity(outputs.len() * 2);
    for output in outputs {
        artifacts.push(DebugSymbolArtifact {
            source_path: output.jni_library_path.clone(),
            archive_path: PathBuf::from("native")
                .join(output.host_target.canonical_name())
                .join(
                    output
                        .jni_library_path
                        .file_name()
                        .expect("jni library should have a filename"),
                ),
            kind: DebugSymbolArtifactKind::Jni,
            target_triple: None,
            platform: None,
            architecture: None,
            abi: None,
            host_target: Some(output.host_target.canonical_name().to_string()),
        });

        if let Some(shared_library_path) = output.shared_library_path.as_ref() {
            artifacts.push(DebugSymbolArtifact {
                source_path: shared_library_path.clone(),
                archive_path: PathBuf::from("native")
                    .join(output.host_target.canonical_name())
                    .join(
                        shared_library_path
                            .file_name()
                            .expect("shared library should have a filename"),
                    ),
                kind: DebugSymbolArtifactKind::Shared,
                target_triple: None,
                platform: None,
                architecture: None,
                abi: None,
                host_target: Some(output.host_target.canonical_name().to_string()),
            });
        }

        for sidecar_path in &output.debug_info_sidecars {
            artifacts.push(DebugSymbolArtifact {
                source_path: sidecar_path.clone(),
                archive_path: PathBuf::from("native")
                    .join(output.host_target.canonical_name())
                    .join(
                        sidecar_path
                            .file_name()
                            .expect("debug info sidecar should have a filename"),
                    ),
                kind: DebugSymbolArtifactKind::DebugInfo,
                target_triple: None,
                platform: None,
                architecture: None,
                abi: None,
                host_target: Some(output.host_target.canonical_name().to_string()),
            });
        }
    }

    write_debug_symbols_zip(
        &config.java_jvm_debug_symbols_output(),
        &match config.java_jvm_debug_symbols_format() {
            DebugSymbolsFormat::Zip => format!("{artifact_name}.jvm.symbols.zip"),
        },
        "java-jvm",
        match config.java_jvm_debug_symbols_bundle() {
            DebugSymbolsBundle::Unstripped => "unstripped",
        },
        &artifacts,
    )?;

    Ok(())
}

fn resolve_java_host_targets_for_packaging(config: &Config) -> Result<Vec<JavaHostTarget>> {
    config
        .java_jvm_host_targets()
        .map_err(|message| CliError::CommandFailed {
            command: message,
            status: None,
        })
}

fn resolve_jvm_packaging_targets(
    config: &Config,
    build_cargo_args: &[String],
    release: bool,
    build_profile: CargoBuildProfile,
    host_targets: &[JavaHostTarget],
    strip_symbols: bool,
    binding_expansion: Option<&BindingExpansion>,
) -> Result<Vec<JvmPackagingTarget>> {
    let current_host = JavaHostTarget::current().ok_or_else(|| CliError::CommandFailed {
        command:
            "JVM packaging is only supported on darwin-arm64, darwin-x86_64, linux-x86_64, linux-aarch64, and windows-x86_64 hosts".to_string(),
        status: None,
    })?;
    let cargo = Cargo::current(build_cargo_args)?;
    let cargo_command_args = binding_expansion
        .map(|expansion| Ok(expansion.cargo_args().clone()))
        .unwrap_or_else(|| LibraryCargoArgs::parse(cargo.probe_command_arguments()))?;
    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let selected_library = binding_expansion
        .map(|expansion| expansion.selected_library().clone())
        .map_or_else(
            || {
                SelectedLibrary::resolve_preferred(
                    config,
                    &cargo,
                    &metadata,
                    &cargo_manifest_path,
                    &config.crate_artifact_name(),
                )
            },
            Ok,
        )?;
    let toolchain_selector = cargo.toolchain_selector().map(str::to_owned);

    host_targets
        .iter()
        .copied()
        .map(|host_target| {
            validate_desktop_jni_symbol_stripping_for(strip_symbols, host_target)?;
            let toolchain = NativeHostToolchain::discover(
                toolchain_selector.as_deref(),
                cargo_command_args.as_slice(),
                selected_library.cargo_manifest_path(),
                host_target,
                current_host,
            )?;
            let cargo_context = JvmCargoContext {
                host_target,
                rust_target_triple: toolchain.rust_target_triple().to_string(),
                release,
                build_profile: build_profile.clone(),
                library: selected_library.clone(),
                target_directory: metadata.target_directory.clone(),
                cargo_command_args: cargo_command_args.clone(),
                toolchain_selector: toolchain_selector.clone(),
            };
            let _ = resolve_jni_include_directories(&cargo_context)?;
            Ok(JvmPackagingTarget {
                cargo_context,
                toolchain,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ensure_java_ir_regeneration, ensure_java_no_build_supported,
        ensure_jvm_pack_cargo_args_supported, resolve_jvm_packaging_targets,
        write_jvm_debug_symbols,
    };
    use crate::build::CargoBuildProfile;
    use crate::cli::CliError;
    use crate::config::{CargoConfig, Config, PackageConfig, TargetsConfig};
    use crate::pack::java::link::JvmPackagedNativeOutput;
    use crate::target::JavaHostTarget;
    use boltffi_bindgen::cargo::LibraryCargoArgsError;

    fn temporary_directory(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    fn config(java_enabled: bool) -> Config {
        Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "workspace-member".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                java: crate::config::JavaConfig {
                    jvm: crate::config::JavaJvmConfig {
                        enabled: java_enabled,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    fn config_with_host_targets(
        java_enabled: bool,
        host_targets: Vec<JavaHostTarget>,
        strip_symbols: bool,
    ) -> Config {
        let mut config = config(java_enabled);
        config.targets.java.jvm.host_targets = Some(host_targets);
        config.targets.java.jvm.strip_symbols = strip_symbols;
        config
    }

    #[test]
    fn rejects_pack_all_no_build_when_java_is_enabled() {
        let error = ensure_java_no_build_supported(&config(true), true, false, "pack all")
            .expect_err("expected no-build rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("pack all --no-build is unsupported in Phase 4")
        ));
    }

    #[test]
    fn allows_pack_all_no_build_when_java_is_disabled() {
        ensure_java_no_build_supported(&config(false), true, false, "pack all")
            .expect("expected no-build to be allowed");
    }

    #[test]
    fn rejects_explicit_cargo_target_for_pack_java() {
        let error = ensure_jvm_pack_cargo_args_supported(
            &[
                "--target".to_string(),
                "x86_64-unknown-linux-gnu".to_string(),
            ],
            "pack java",
        )
        .expect_err("expected explicit target rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("remove cargo --target 'x86_64-unknown-linux-gnu'")
        ));
    }

    #[test]
    fn rejects_broad_package_selection_before_jvm_cargo_metadata() {
        let current_host = JavaHostTarget::current().expect("current host");
        let error = match resolve_jvm_packaging_targets(
            &config_with_host_targets(true, vec![current_host], false),
            &["--workspace".to_string()],
            false,
            CargoBuildProfile::Debug,
            &[current_host],
            false,
            None,
        ) {
            Ok(_) => panic!("workspace selection must fail before Cargo metadata"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            CliError::LibraryCargoArgs(LibraryCargoArgsError::PackageSet { argument })
                if argument == "--workspace"
        ));
    }

    #[test]
    fn rejects_explicit_cargo_target_for_pack_kmp_with_kmp_command_name() {
        let error = ensure_jvm_pack_cargo_args_supported(
            &[
                "--target".to_string(),
                "x86_64-unknown-linux-gnu".to_string(),
            ],
            "pack kmp",
        )
        .expect_err("expected explicit target rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("pack kmp resolves desktop targets")
                    && command.contains("targets.java.jvm.host_targets")
        ));
    }

    #[test]
    fn pack_java_no_longer_requires_experimental_gate() {
        ensure_java_no_build_supported(&config(true), false, false, "pack java")
            .expect("expected pack java to proceed without experimental gate");
    }

    #[test]
    fn rejects_binding_ir_packaging_without_regeneration() {
        let error = ensure_java_ir_regeneration(false)
            .expect_err("Binding IR packaging must not consume unverified generated files");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("pack java requires regenerated bindings")
        ));
        ensure_java_ir_regeneration(true).expect("regenerated Binding IR packaging should proceed");
    }

    #[test]
    fn rejects_windows_strip_symbols_during_preflight() {
        let error = match resolve_jvm_packaging_targets(
            &config_with_host_targets(true, vec![JavaHostTarget::WindowsX86_64], true),
            &[],
            false,
            CargoBuildProfile::Named("dist".to_string()),
            &[JavaHostTarget::WindowsX86_64],
            true,
            None,
        ) {
            Ok(_) => panic!("expected unsupported windows strip config to fail during preflight"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            CliError::CommandFailed { status: None, .. }
        ));
    }

    #[test]
    fn jvm_debug_symbols_archive_uses_selected_artifact_name_and_includes_bundled_cdylib() {
        let temp_root = temporary_directory("boltffi-jvm-debug-symbols");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let jni_library = temp_root.join("libdemo_jni.dylib");
        let shared_library = temp_root.join("libdemo.dylib");
        fs::write(&jni_library, b"jni").expect("write jni library");
        fs::write(&shared_library, b"shared").expect("write shared library");

        let config = Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "workspace-root".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                java: crate::config::JavaConfig {
                    jvm: crate::config::JavaJvmConfig {
                        enabled: true,
                        output: temp_root.join("java"),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        };

        write_jvm_debug_symbols(
            &config,
            "workspace_member",
            &[JvmPackagedNativeOutput {
                host_target: JavaHostTarget::DarwinArm64,
                has_shared_library_copy: true,
                jni_library_path: jni_library,
                shared_library_path: Some(shared_library),
                debug_info_sidecars: Vec::new(),
            }],
        )
        .expect("write jvm debug symbols");

        let archive_path = config
            .java_jvm_debug_symbols_output()
            .join("workspace_member.jvm.symbols.zip");
        let archive_file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(archive_file).expect("read zip archive");
        let bundle_root = "workspace_member.jvm.symbols";

        archive
            .by_name(&format!(
                "{bundle_root}/native/darwin-arm64/libdemo_jni.dylib"
            ))
            .expect("jni entry");
        archive
            .by_name(&format!("{bundle_root}/native/darwin-arm64/libdemo.dylib"))
            .expect("shared entry");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn jvm_debug_symbols_archive_includes_windows_pdb_sidecars() {
        let temp_root = temporary_directory("boltffi-jvm-debug-symbols-pdb");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let jni_library = temp_root.join("demo_jni.dll");
        let jni_pdb = temp_root.join("demo_jni.pdb");
        let shared_library = temp_root.join("demo.dll");
        let shared_pdb = temp_root.join("demo.pdb");
        fs::write(&jni_library, b"jni").expect("write jni library");
        fs::write(&jni_pdb, b"jni-pdb").expect("write jni pdb");
        fs::write(&shared_library, b"shared").expect("write shared library");
        fs::write(&shared_pdb, b"shared-pdb").expect("write shared pdb");

        let config = Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "workspace-root".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                java: crate::config::JavaConfig {
                    jvm: crate::config::JavaJvmConfig {
                        enabled: true,
                        output: temp_root.join("java"),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        };

        write_jvm_debug_symbols(
            &config,
            "workspace_member",
            &[JvmPackagedNativeOutput {
                host_target: JavaHostTarget::WindowsX86_64,
                has_shared_library_copy: true,
                jni_library_path: jni_library,
                shared_library_path: Some(shared_library),
                debug_info_sidecars: vec![jni_pdb, shared_pdb],
            }],
        )
        .expect("write jvm debug symbols");

        let archive_path = config
            .java_jvm_debug_symbols_output()
            .join("workspace_member.jvm.symbols.zip");
        let archive_file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(archive_file).expect("read zip archive");
        let bundle_root = "workspace_member.jvm.symbols";

        archive
            .by_name(&format!("{bundle_root}/native/windows-x86_64/demo_jni.dll"))
            .expect("jni entry");
        archive
            .by_name(&format!("{bundle_root}/native/windows-x86_64/demo_jni.pdb"))
            .expect("jni pdb entry");
        archive
            .by_name(&format!("{bundle_root}/native/windows-x86_64/demo.dll"))
            .expect("shared entry");
        archive
            .by_name(&format!("{bundle_root}/native/windows-x86_64/demo.pdb"))
            .expect("shared pdb entry");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn jvm_debug_symbols_archive_includes_directory_sidecars() {
        let temp_root = temporary_directory("boltffi-jvm-debug-symbols-dsym");
        fs::create_dir_all(&temp_root).expect("create temp root");
        let jni_library = temp_root.join("libdemo_jni.dylib");
        let dsym_dir = temp_root.join("libdemo_jni.dylib.dSYM");
        let dsym_dwarf_dir = dsym_dir.join("Contents").join("Resources").join("DWARF");
        fs::write(&jni_library, b"jni").expect("write jni library");
        fs::create_dir_all(&dsym_dwarf_dir).expect("create dsym dwarf dir");
        fs::write(dsym_dir.join("Contents").join("Info.plist"), "<plist />")
            .expect("write dsym plist");
        fs::write(dsym_dwarf_dir.join("libdemo_jni.dylib"), b"debug").expect("write dsym dwarf");

        let config = Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "workspace-root".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                java: crate::config::JavaConfig {
                    jvm: crate::config::JavaJvmConfig {
                        enabled: true,
                        output: temp_root.join("java"),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        };

        write_jvm_debug_symbols(
            &config,
            "workspace_member",
            &[JvmPackagedNativeOutput {
                host_target: JavaHostTarget::DarwinArm64,
                has_shared_library_copy: false,
                jni_library_path: jni_library,
                shared_library_path: None,
                debug_info_sidecars: vec![dsym_dir],
            }],
        )
        .expect("write jvm debug symbols");

        let archive_path = config
            .java_jvm_debug_symbols_output()
            .join("workspace_member.jvm.symbols.zip");
        let archive_file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(archive_file).expect("read zip archive");
        let bundle_root = "workspace_member.jvm.symbols";

        archive
            .by_name(&format!(
                "{bundle_root}/native/darwin-arm64/libdemo_jni.dylib.dSYM/Contents/Info.plist"
            ))
            .expect("dsym plist entry");
        archive
            .by_name(&format!(
                "{bundle_root}/native/darwin-arm64/libdemo_jni.dylib.dSYM/Contents/Resources/DWARF/libdemo_jni.dylib"
            ))
            .expect("dsym dwarf entry");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }
}
