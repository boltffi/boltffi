use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use boltffi_backend::target::java::JavaDesktopLoader;
use boltffi_backend::target::jvm::{LibraryName, NativeLibraries};
use boltffi_backend::target::kmp::{KMP_SUPPORT_REPORT_FILE, KmpSupportMode};
use boltffi_backend::target::kotlin::{
    KotlinApiStyle as BackendKotlinApiStyle, KotlinDesktopLoader as BackendKotlinDesktopLoader,
    KotlinFactoryStyle as BackendKotlinFactoryStyle,
};
use boltffi_backend::{CoverageMode, GeneratedOutput};
use boltffi_bindgen::generate::{Generation, GenerationError};
use boltffi_bindgen::render::kotlin::{
    FactoryStyle as BindgenFactoryStyle, KotlinApiStyle as BindgenKotlinApiStyle,
    KotlinDesktopLoader as BindgenKotlinDesktopLoader, KotlinOptions,
};
use boltffi_bindgen::target::Target;
use boltffi_binding::BindingMetadataSurface;

use crate::build::BindingExpansion;
use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::config::{
    Config, KotlinFactoryStyle, SpmLayout,
    targets::kotlin::{KotlinApiStyle, KotlinDesktopLoader},
};
use crate::toolchain::{AndroidToolchain, NativeHostToolchain};

use super::{
    GenerateOptions, GenerateTarget,
    java::{Output as JavaOutput, Plan as JavaPlan, Platform as JavaPlatform, TargetGeneration},
};

const KMP_MANAGED_GENERATED_PATHS: &[&str] = &[
    "settings.gradle.kts",
    "build.gradle.kts",
    KMP_SUPPORT_REPORT_FILE,
    "src/commonMain/kotlin",
    "src/jvmMain/kotlin",
    "src/androidMain/kotlin",
    "src/jvmMain/c",
    "src/androidMain/c",
];

struct RenderedJava {
    target: String,
    output: GeneratedOutput,
}

impl RenderedJava {
    fn difference(&self, candidate: &Self) -> Option<String> {
        if self.output == candidate.output {
            return None;
        }
        let differing_files = self
            .output
            .files()
            .iter()
            .chain(candidate.output.files())
            .map(|file| file.path().as_path().to_path_buf())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|path| self.contents(path) != candidate.contents(path))
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        Some(format!(
            "target-specific Java Binding IR outputs disagree between '{}' and '{}': files=[{}], coverage={}, diagnostics={}",
            self.target,
            candidate.target,
            differing_files.join(", "),
            self.output.coverage() != candidate.output.coverage(),
            self.output.diagnostics() != candidate.output.diagnostics(),
        ))
    }

    fn contents(&self, path: &Path) -> Option<&str> {
        self.output
            .files()
            .iter()
            .find(|file| file.path().as_path() == path)
            .map(|file| file.contents())
    }
}

pub fn run_ir_generation(config: &Config, options: &GenerateOptions) -> Result<()> {
    match &options.target {
        GenerateTarget::Swift => generate_swift(config, options),
        GenerateTarget::Python => generate_python(config, options),
        GenerateTarget::Java => generate_java(config, options),
        GenerateTarget::Kotlin => generate_kotlin(config, options),
        GenerateTarget::KotlinMultiplatform => generate_kmp(config, options),
        GenerateTarget::Typescript => generate_typescript(config, options),
        GenerateTarget::CSharp => generate_csharp(config, options),
        other => Err(CliError::CommandFailed {
            command: format!(
                "--ir is only available for swift, python, java, kotlin, kmp, typescript, and csharp, not {}",
                target_label(other)
            ),
            status: None,
        }),
    }
}

fn generate_typescript(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_wasm_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.wasm.enabled = false".to_string(),
            status: None,
        });
    }

    let expansion = BindingExpansion::resolve_for_commands(
        config,
        &["build", "generate"],
        &options.cargo_args,
    )?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.wasm_typescript_output());

    expansion
        .generation()
        .binding_surface(BindingMetadataSurface::Wasm32)
        .coverage_mode(CoverageMode::Partial)
        .typescript_module(config.wasm_typescript_module_name())
        .typescript_runtime_package(config.wasm_runtime_package())
        .render(Target::TypeScript)
        .and_then(|output| {
            print_coverage("typescript", &output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error("typescript", error))
}

fn generate_java(config: &Config, options: &GenerateOptions) -> Result<()> {
    let plan = JavaPlan::resolve(config, options.output.clone())?;
    let cargo_args = config
        .cargo_args_for_commands(&["build", "generate"])
        .into_iter()
        .chain(options.cargo_args.iter().cloned())
        .collect::<Vec<_>>();
    ensure_java_cargo_target_unset(&cargo_args, plan.platform())?;
    let expansion = BindingExpansion::resolve(config, &cargo_args)?;
    let generations = resolve_java_generations(config, &plan, &expansion)?;
    write_java(config, &plan, expansion.artifact_name(), generations)
}

fn ensure_java_cargo_target_unset(cargo_args: &[String], platform: JavaPlatform) -> Result<()> {
    let cargo = Cargo::current(cargo_args)?;
    let Some(target) = cargo.target_selector() else {
        return Ok(());
    };
    let (targets, source) = match platform {
        JavaPlatform::Jvm => ("desktop targets", "targets.java.jvm.host_targets"),
        JavaPlatform::Android => ("Android targets", "targets.android.architectures"),
    };
    Err(CliError::CommandFailed {
        command: format!(
            "generate java resolves {targets} from {source}; remove cargo --target '{target}'"
        ),
        status: None,
    })
}

fn resolve_java_generations(
    config: &Config,
    plan: &JavaPlan,
    expansion: &BindingExpansion,
) -> Result<Vec<TargetGeneration>> {
    match plan.platform() {
        JavaPlatform::Jvm => {
            let targets =
                config
                    .java_jvm_host_targets()
                    .map_err(|message| CliError::CommandFailed {
                        command: message,
                        status: None,
                    })?;
            Ok(NativeHostToolchain::discover_matrix(
                expansion.toolchain_selector(),
                expansion.cargo_args().as_slice(),
                expansion.selected_library().cargo_manifest_path(),
                &targets,
            )?
            .into_iter()
            .map(|(target, toolchain)| {
                let triple = toolchain.rust_target_triple().to_owned();
                let generation = expansion
                    .generation()
                    .triple(triple.clone())
                    .cargo_environment(toolchain.cargo_environment());
                TargetGeneration::new(
                    format!("{} [{triple}]", target.canonical_name()),
                    generation,
                )
            })
            .collect())
        }
        JavaPlatform::Android => {
            let toolchain = AndroidToolchain::discover(
                config.java_android_min_sdk(),
                config.android_ndk_version(),
            )?;
            config
                .android_targets()
                .into_iter()
                .map(|target| {
                    let triple = target.triple().to_owned();
                    let generation = expansion
                        .generation()
                        .triple(triple.clone())
                        .cargo_environment(toolchain.cargo_environment(&target)?);
                    Ok(TargetGeneration::new(triple, generation))
                })
                .collect()
        }
    }
}

pub fn run_java_generations(
    config: &Config,
    output: Option<PathBuf>,
    artifact_name: &str,
    generations: impl IntoIterator<Item = TargetGeneration>,
) -> Result<()> {
    let plan = JavaPlan::resolve(config, output)?;
    write_java(config, &plan, artifact_name, generations)
}

fn write_java(
    config: &Config,
    plan: &JavaPlan,
    artifact_name: &str,
    generations: impl IntoIterator<Item = TargetGeneration>,
) -> Result<()> {
    let bindgen_target = Target::Java;
    let target_name = bindgen_target.name();
    let libraries = NativeLibraries::from_artifact(artifact_name)
        .map_err(|error| generation_error(target_name, GenerationError::Render(error)))?;
    let desktop_loader = match plan.platform().uses_desktop_loader() {
        true => JavaDesktopLoader::Bundled,
        false => JavaDesktopLoader::None,
    };
    let mut outputs = generations.into_iter().map(|generation| {
        let (target_label, generation) = generation.into_parts();
        generation
            .coverage_mode(CoverageMode::Partial)
            .java_package(config.java_package())
            .java_file(config.java_module_name())
            .java_android_library(libraries.android().as_str())
            .java_desktop_jni_library(libraries.desktop_jni().as_str())
            .java_desktop_fallback_library(libraries.desktop_fallback().as_str())
            .java_desktop_loader(desktop_loader)
            .java_version(plan.version())
            .java_c_header(PathBuf::from("jni").join(format!("{artifact_name}.h")))
            .render(bindgen_target)
            .map_err(|error| target_generation_error(target_name, &target_label, error))
            .map(|output| RenderedJava {
                target: target_label,
                output,
            })
    });
    let generated = outputs
        .next()
        .transpose()?
        .ok_or_else(|| CliError::CommandFailed {
            command: "Java Binding IR generation requires at least one Cargo build contract"
                .to_string(),
            status: None,
        })?;
    if let Some(mismatch) = outputs.try_fold(None, |mismatch, candidate| {
        candidate.map(|candidate| mismatch.or_else(|| generated.difference(&candidate)))
    })? {
        return Err(CliError::CommandFailed {
            command: mismatch,
            status: None,
        });
    }
    print_coverage(target_name, &generated.output);
    JavaOutput::new(plan.output(), &config.java_package())?.write(generated.output)
}

fn generate_swift(config: &Config, options: &GenerateOptions) -> Result<()> {
    let target = Target::Swift;
    let target_name = target.name();

    if !config.is_enabled(target) {
        return Err(CliError::CommandFailed {
            command: "targets.apple.enabled = false".to_string(),
            status: None,
        });
    }

    let expansion = BindingExpansion::resolve_for_commands(
        config,
        &["build", "generate"],
        &options.cargo_args,
    )?;
    let output_directory = swift_output_directory(config, options);
    let ffi_module = config
        .apple_swift_ffi_module_name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}FFI", config.xcframework_name()));

    expansion
        .generation()
        .swift_ffi_module(ffi_module)
        .swift_file(config.swift_bindings_file_stem())
        .swift_custom_mappings(config.apple_swift_custom_mappings())
        .render(target)
        .and_then(|output| {
            print_coverage(target_name, &output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error(target_name, error))
}

fn swift_output_directory(config: &Config, options: &GenerateOptions) -> PathBuf {
    options.output.clone().unwrap_or_else(|| {
        let output = config.apple_swift_output();
        match config.apple_spm_layout() {
            SpmLayout::Split => output.join("BoltFFI"),
            SpmLayout::Bundled | SpmLayout::FfiOnly => output,
        }
    })
}

fn generate_csharp(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_csharp_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.csharp.enabled = false".to_string(),
            status: None,
        });
    }

    let expansion = BindingExpansion::resolve_for_commands(
        config,
        &["build", "generate"],
        &options.cargo_args,
    )?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.csharp_output());

    expansion
        .generation()
        .coverage_mode(CoverageMode::Partial)
        .csharp_namespace(config.csharp_namespace().map(str::to_owned))
        .csharp_native_library(expansion.artifact_name())
        .render(Target::CSharp)
        .and_then(|output| {
            print_coverage(Target::CSharp.name(), &output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error(Target::CSharp.name(), error))
}

fn generate_python(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_python_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.python.enabled = false".to_string(),
            status: None,
        });
    }

    let expansion = BindingExpansion::resolve_for_commands(
        config,
        &["build", "generate"],
        &options.cargo_args,
    )?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.python_output());

    write_python(
        config,
        output_directory,
        expansion.manifest_path(),
        expansion.artifact_name().to_string(),
        expansion.cargo_args().clone().into_vec(),
        expansion.toolchain_selector().map(str::to_owned),
    )
}

fn generate_kotlin(config: &Config, options: &GenerateOptions) -> Result<()> {
    let target = Target::Kotlin;
    let target_name = target.name();

    if !config.is_enabled(target) {
        return Err(CliError::CommandFailed {
            command: "targets.android.enabled = false".to_string(),
            status: None,
        });
    }

    let expansion = BindingExpansion::resolve_for_commands(
        config,
        &["build", "generate"],
        &options.cargo_args,
    )?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.android_kotlin_output());
    let libraries =
        NativeLibraries::from_artifact(config.resolved_android_kotlin_desktop_library_name())
            .map_err(|error| generation_error(target_name, GenerationError::Render(error)))?;
    let android_library = LibraryName::parse(config.resolved_android_kotlin_library_name())
        .map_err(|error| generation_error(target_name, GenerationError::Render(error)))?;

    expansion
        .generation()
        .kotlin_package(config.android_kotlin_package())
        .kotlin_file(config.android_kotlin_module_name())
        .kotlin_api_style(kotlin_api_style(config.android_kotlin_api_style()))
        .kotlin_factory_style(kotlin_factory_style(config.android_kotlin_factory_style()))
        .kotlin_custom_mappings(config.android_kotlin_custom_mappings())
        .kotlin_android_library(android_library.as_str())
        .kotlin_desktop_jni_library(libraries.desktop_jni().as_str())
        .kotlin_desktop_fallback_library(libraries.desktop_fallback().as_str())
        .kotlin_desktop_loader(kotlin_desktop_loader(
            config.android_kotlin_desktop_loader(),
        ))
        .kotlin_c_header(PathBuf::from("jni").join(format!("{}.h", config.library_name())))
        .render(target)
        .and_then(|output| {
            print_coverage(target_name, &output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error(target_name, error))
}

fn generate_kmp(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_kotlin_multiplatform_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.kotlin_multiplatform.enabled = false".to_string(),
            status: None,
        });
    }

    if !config.should_process(Target::KotlinMultiplatform, options.experimental) {
        return Err(CliError::CommandFailed {
            command: format!(
                "{} is experimental, use --experimental flag or add \"{}\" to [experimental]",
                Target::KotlinMultiplatform.name(),
                Target::KotlinMultiplatform.name()
            ),
            status: None,
        });
    }

    let cargo_args = config
        .cargo_args_for_commands(&["build", "generate"])
        .into_iter()
        .chain(options.cargo_args.iter().cloned())
        .collect::<Vec<_>>();
    let cargo = Cargo::current(&cargo_args)?;
    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let package_selector =
        cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
    let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
    let library_target =
        package.resolve_library_target(&config.crate_artifact_name(), &cargo_manifest_path)?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.kotlin_multiplatform_output());

    write_kmp(
        config,
        output_directory,
        package.manifest_path.clone(),
        library_target.name.clone(),
        cargo.probe_command_arguments(),
        cargo.toolchain_selector().map(str::to_owned),
    )
}

fn kotlin_desktop_loader(loader: KotlinDesktopLoader) -> BackendKotlinDesktopLoader {
    match loader {
        KotlinDesktopLoader::Bundled => BackendKotlinDesktopLoader::Bundled,
        KotlinDesktopLoader::System => BackendKotlinDesktopLoader::System,
        KotlinDesktopLoader::None => BackendKotlinDesktopLoader::None,
    }
}

fn kotlin_api_style(style: KotlinApiStyle) -> BackendKotlinApiStyle {
    match style {
        KotlinApiStyle::TopLevel => BackendKotlinApiStyle::TopLevel,
        KotlinApiStyle::ModuleObject => BackendKotlinApiStyle::ModuleObject,
    }
}

fn kotlin_factory_style(style: KotlinFactoryStyle) -> BackendKotlinFactoryStyle {
    match style {
        KotlinFactoryStyle::Constructors => BackendKotlinFactoryStyle::Constructors,
        KotlinFactoryStyle::CompanionMethods => BackendKotlinFactoryStyle::CompanionMethods,
    }
}

pub fn run_python_generation(
    config: &Config,
    output: Option<PathBuf>,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    if !config.is_python_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.python.enabled = false".to_string(),
            status: None,
        });
    }

    let output_directory = output.unwrap_or_else(|| config.python_output());

    write_python(
        config,
        output_directory,
        manifest_path,
        artifact_name,
        cargo_args,
        toolchain_selector,
    )
}

pub fn run_csharp_generation(
    config: &Config,
    output: Option<PathBuf>,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    if !config.is_csharp_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.csharp.enabled = false".to_string(),
            status: None,
        });
    }
    let output_directory = output.unwrap_or_else(|| config.csharp_output());
    Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .cargo_toolchain_selector(toolchain_selector)
        .coverage_mode(CoverageMode::Partial)
        .csharp_namespace(config.csharp_namespace().map(str::to_owned))
        .csharp_native_library(artifact_name)
        .render(Target::CSharp)
        .and_then(|output| {
            print_coverage(Target::CSharp.name(), &output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error(Target::CSharp.name(), error))
}

pub fn run_kmp_generation(
    config: &Config,
    output: Option<PathBuf>,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    if !config.is_kotlin_multiplatform_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.kotlin_multiplatform.enabled = false".to_string(),
            status: None,
        });
    }

    let output_directory = output.unwrap_or_else(|| config.kotlin_multiplatform_output());

    write_kmp(
        config,
        output_directory,
        manifest_path,
        artifact_name,
        cargo_args,
        toolchain_selector,
    )
}

pub fn run_c_header_generation(
    output_directory: PathBuf,
    manifest_path: PathBuf,
    header_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .cargo_toolchain_selector(toolchain_selector)
        .render_c_header(format!("{header_name}.h"))
        .and_then(|output| Generation::write_output(output, &output_directory))
        .map(drop)
        .map_err(|error| generation_error("c header", error))
}

fn write_python(
    config: &Config,
    output_directory: PathBuf,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .cargo_toolchain_selector(toolchain_selector)
        .coverage_mode(CoverageMode::Partial)
        .python_module_name(config.python_module_name())
        .python_distribution_name(config.package.name.clone())
        .python_package_version(config.package_version())
        .python_native_library(artifact_name)
        .render(Target::Python)
        .and_then(|output| {
            print_coverage("python", &output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error("python", error))
}

fn write_kmp(
    config: &Config,
    output_directory: PathBuf,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    let support_mode = if config.kotlin_multiplatform_preview_prune_unsupported() {
        eprintln!(
            "warning: KMP preview pruning is enabled; unsupported APIs will be omitted and recorded in {}",
            output_directory.join(KMP_SUPPORT_REPORT_FILE).display()
        );
        KmpSupportMode::PreviewPruneUnsupported
    } else {
        KmpSupportMode::Strict
    };
    let coverage = match support_mode {
        KmpSupportMode::Strict => CoverageMode::Complete,
        KmpSupportMode::PreviewPruneUnsupported => CoverageMode::Partial,
        _ => CoverageMode::Complete,
    };

    let module_name = config.kotlin_multiplatform_module_name();
    let output = Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .cargo_toolchain_selector(toolchain_selector)
        .coverage_mode(coverage)
        .kmp_package_name(config.kotlin_multiplatform_package())
        .kmp_module_name(module_name.clone())
        .kmp_min_sdk(config.android_min_sdk())
        .kmp_kotlin_options(kmp_kotlin_options(config, &module_name, &artifact_name))
        .kmp_support_mode(support_mode)
        .render(Target::KotlinMultiplatform)
        .map_err(|error| generation_error("kmp", error))?;

    write_kmp_output(output, &output_directory)
}

fn kmp_kotlin_options(
    config: &Config,
    module_name: &str,
    desktop_fallback_library_name: &str,
) -> KotlinOptions {
    let factory_style = match config.android_kotlin_factory_style() {
        KotlinFactoryStyle::Constructors => BindgenFactoryStyle::Constructors,
        KotlinFactoryStyle::CompanionMethods => BindgenFactoryStyle::CompanionMethods,
    };

    KotlinOptions {
        factory_style,
        api_style: BindgenKotlinApiStyle::TopLevel,
        module_object_name: Some(module_name.to_string()),
        library_name: Some(boltffi_bindgen::load_library_name(
            &config.resolved_android_kotlin_library_name(),
        )),
        desktop_jni_library_name: Some(boltffi_bindgen::library_name(
            &config.resolved_android_kotlin_desktop_library_name(),
        )),
        desktop_fallback_library_name: Some(boltffi_bindgen::library_name(
            desktop_fallback_library_name,
        )),
        desktop_loader: BindgenKotlinDesktopLoader::Bundled,
    }
}

fn write_kmp_output(output: GeneratedOutput, output_directory: &Path) -> Result<()> {
    prepare_kmp_output_directory(output_directory)?;
    Generation::write_output(output, output_directory)
        .map(drop)
        .map_err(|error| generation_error("kmp", error))
}

fn prepare_kmp_output_directory(output_directory: &Path) -> Result<()> {
    fs::create_dir_all(output_directory).map_err(|source| CliError::CreateDirectoryFailed {
        path: output_directory.to_path_buf(),
        source,
    })?;
    remove_stale_kmp_generated_paths(output_directory)
}

fn remove_stale_kmp_generated_paths(output_directory: &Path) -> Result<()> {
    KMP_MANAGED_GENERATED_PATHS
        .iter()
        .map(|relative_path| output_directory.join(relative_path))
        .try_for_each(remove_stale_generated_path)
}

fn remove_stale_generated_path(path: PathBuf) -> Result<()> {
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CliError::ReadFailed { path, source });
        }
    };

    if metadata.is_dir() {
        fs::remove_dir_all(&path).map_err(|source| CliError::WriteFailed { path, source })
    } else {
        fs::remove_file(&path).map_err(|source| CliError::WriteFailed { path, source })
    }
}

fn print_coverage(target: &str, output: &GeneratedOutput) {
    let unsupported = output.coverage().unsupported();
    if unsupported.is_empty() {
        return;
    }

    eprintln!("{target} generation skipped unsupported declarations");
    eprintln!("{:<12} {:<48} reason", "kind", "name");
    unsupported.iter().for_each(|item| {
        eprintln!(
            "{:<12} {:<48} {}",
            item.declaration().kind(),
            item.declaration().name(),
            item.reason()
        );
    });
}

fn generation_error(target: &str, error: GenerationError) -> CliError {
    CliError::CommandFailed {
        command: format!("generate {target}: {error}"),
        status: None,
    }
}

fn target_generation_error(target: &str, build_target: &str, error: GenerationError) -> CliError {
    CliError::CommandFailed {
        command: format!("generate {target} for {build_target}: {error}"),
        status: None,
    }
}

fn target_label(target: &GenerateTarget) -> &'static str {
    match target {
        GenerateTarget::Swift => "swift",
        GenerateTarget::Kotlin => Target::Kotlin.name(),
        GenerateTarget::KotlinMultiplatform => "kmp",
        GenerateTarget::Java => "java",
        GenerateTarget::Header => "header",
        GenerateTarget::Typescript => "typescript",
        GenerateTarget::Dart => "dart",
        GenerateTarget::Python => "python",
        GenerateTarget::CSharp => "csharp",
        GenerateTarget::All => "all",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GenerateOptions, GenerateTarget, prepare_kmp_output_directory, run_ir_generation,
        run_java_generations, write_kmp_output,
    };
    use crate::{cli::CliError, config::Config};
    use boltffi_backend::{FilePath, GeneratedFile, GeneratedOutput};
    use boltffi_bindgen::generate::Generation;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::commands::generate::java::TargetGeneration;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    fn demo_manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo/Cargo.toml")
    }

    #[test]
    fn ir_kmp_prepare_output_removes_managed_paths_and_preserves_native_outputs() {
        let output_directory = unique_temp_dir("boltffi-ir-kmp-generated-cleanup-test");
        let stale_common = output_directory.join("src/commonMain/kotlin/com/old/Old.kt");
        let stale_jvm_c = output_directory.join("src/jvmMain/c/jni_glue.c");
        let stale_report = output_directory.join("boltffi-kmp-support.json");
        let native_resource = output_directory.join("src/jvmMain/resources/native/current/lib.so");
        let android_jnilib = output_directory.join("src/androidMain/jniLibs/arm64-v8a/libdemo.so");

        for path in [
            &stale_common,
            &stale_jvm_c,
            &stale_report,
            &native_resource,
            &android_jnilib,
        ] {
            fs::create_dir_all(path.parent().expect("test path has parent"))
                .expect("create test directory");
            fs::write(path, []).expect("write test file");
        }
        fs::write(output_directory.join("build.gradle.kts"), []).expect("write stale gradle file");

        prepare_kmp_output_directory(&output_directory).expect("cleanup should succeed");

        assert!(!stale_common.exists());
        assert!(!stale_jvm_c.exists());
        assert!(!stale_report.exists());
        assert!(!output_directory.join("build.gradle.kts").exists());
        assert!(native_resource.exists());
        assert!(android_jnilib.exists());

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn ir_kmp_write_output_cleans_managed_paths_before_writing_files() {
        let output_directory = unique_temp_dir("boltffi-ir-kmp-write-cleanup-test");
        let stale_common = output_directory.join("src/commonMain/kotlin/com/old/Old.kt");
        let new_common = output_directory.join("src/commonMain/kotlin/com/new/New.kt");
        let native_resource = output_directory.join("src/jvmMain/resources/native/current/lib.so");
        fs::create_dir_all(stale_common.parent().expect("test path has parent"))
            .expect("create stale common directory");
        fs::write(&stale_common, "stale").expect("write stale common");
        fs::create_dir_all(native_resource.parent().expect("test path has parent"))
            .expect("create native resource directory");
        fs::write(&native_resource, "native").expect("write native resource");

        let output = GeneratedOutput::new(
            vec![GeneratedFile::new(
                FilePath::new("src/commonMain/kotlin/com/new/New.kt").expect("valid file path"),
                "current",
            )],
            Vec::new(),
        );

        write_kmp_output(output, &output_directory).expect("write should clean and emit");

        assert!(!stale_common.exists());
        assert_eq!(
            fs::read_to_string(new_common).expect("read new common"),
            "current"
        );
        assert!(native_resource.exists());

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn ir_generation_accepts_kmp_target_before_cargo_probe() {
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = false
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::KotlinMultiplatform,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: Vec::new(),
            },
        )
        .expect_err("disabled KMP IR generation should fail before cargo probing");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "targets.kotlin_multiplatform.enabled = false"
        ));
    }

    #[test]
    fn ir_generation_accepts_java_target_before_cargo_probe() {
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::Java,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: Vec::new(),
            },
        )
        .expect_err("disabled Java IR generation should fail before cargo probing");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "both targets.java.jvm.enabled and targets.java.android.enabled are false"
        ));
    }

    #[test]
    fn ir_generation_accepts_typescript_target_before_cargo_probe() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.wasm]
enabled = false
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::Typescript,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: Vec::new(),
            },
        )
        .expect_err("disabled TypeScript IR generation should fail before cargo probing");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "targets.wasm.enabled = false"
        ));
    }

    #[test]
    fn java_ir_generation_rejects_an_explicit_cargo_target_before_metadata_building() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.java.jvm]
enabled = true
host_targets = ["current"]
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::Java,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: vec![
                    "--target".to_string(),
                    "x86_64-unknown-linux-gnu".to_string(),
                ],
            },
        )
        .expect_err("explicit Cargo target must not bypass the configured Java matrix");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "generate java resolves desktop targets from targets.java.jvm.host_targets; remove cargo --target 'x86_64-unknown-linux-gnu'"
        ));
    }

    #[test]
    fn java_android_ir_generation_rejects_an_explicit_cargo_target_before_ndk_discovery() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.java.android]
enabled = true
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::Java,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: vec![
                    "--target=aarch64-linux-android".to_string(),
                    "--offline".to_string(),
                ],
            },
        )
        .expect_err("explicit Cargo target must not narrow the configured Android matrix");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "generate java resolves Android targets from targets.android.architectures; remove cargo --target 'aarch64-linux-android'"
        ));
    }

    #[test]
    fn java_android_generation_uses_its_own_minimum_sdk() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.android]
min_sdk = 24

[targets.java.android]
enabled = true
min_sdk = 29
"#,
        );

        assert_eq!(config.android_min_sdk(), 24);
        assert_eq!(config.java_android_min_sdk(), 29);
    }

    #[test]
    fn java_matrix_errors_identify_the_exact_failed_target() {
        let output = unique_temp_dir("boltffi-java-target-error-test");
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.java.jvm]
enabled = true
host_targets = ["current"]
"#,
        );
        let target = "linux-x86_64 [x86_64-unknown-linux-gnu]";
        let error = run_java_generations(
            &config,
            Some(output),
            "demo",
            [TargetGeneration::new(
                target,
                Generation::new("missing-java-target/Cargo.toml"),
            )],
        )
        .expect_err("failed matrix generation must identify its build target");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.starts_with(&format!("generate java for {target}:"))
        ));
    }

    #[test]
    fn ir_generation_requires_kmp_experimental_opt_in() {
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::KotlinMultiplatform,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: Vec::new(),
            },
        )
        .expect_err("KMP IR generation should require experimental opt-in");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("kotlin_multiplatform is experimental")
        ));
    }

    #[test]
    fn ir_kmp_strict_generation_still_fails_closed_for_unsupported_surface() {
        let output_directory = unique_temp_dir("boltffi-ir-kmp-strict-unsupported-test");
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = true
package = "com.boltffi.demo"
"#,
        );

        let error = super::write_kmp(
            &config,
            output_directory.clone(),
            demo_manifest_path(),
            "demo".to_string(),
            Vec::new(),
            None,
        )
        .expect_err("production IR KMP must fail closed for unsupported declarations");

        assert!(
            matches!(
                &error,
                CliError::CommandFailed { command, status: None }
                    if command.contains("generate kmp: render bindings")
                        && command.contains("did not render every declaration")
            ),
            "{error:?}"
        );

        if output_directory.exists() {
            fs::remove_dir_all(output_directory).expect("cleanup generated output");
        }
    }
}
