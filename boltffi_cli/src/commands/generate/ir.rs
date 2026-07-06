use std::{
    fs,
    path::{Path, PathBuf},
};

use boltffi_backend::target::kmp::{KMP_SUPPORT_REPORT_FILE, KmpSupportMode};
use boltffi_backend::target::kotlin::{
    KotlinApiStyle as BackendKotlinApiStyle, KotlinDesktopLoader as BackendKotlinDesktopLoader,
    KotlinFactoryStyle as BackendKotlinFactoryStyle,
};
use boltffi_backend::{CoverageMode, GeneratedOutput};
use boltffi_bindgen::generate::{Generation, GenerationError};
use boltffi_bindgen::target::Target;

use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::config::{
    Config, KotlinFactoryStyle, SpmLayout,
    targets::kotlin::{KotlinApiStyle, KotlinDesktopLoader},
};

use super::{GenerateOptions, GenerateTarget, languages::remove_stale_kmp_generated_paths};

pub fn run_ir_generation(config: &Config, options: &GenerateOptions) -> Result<()> {
    match &options.target {
        GenerateTarget::Swift => generate_swift(config, options),
        GenerateTarget::Python => generate_python(config, options),
        GenerateTarget::Kotlin => generate_kotlin(config, options),
        GenerateTarget::KotlinMultiplatform => generate_kmp(config, options),
        other => Err(CliError::CommandFailed {
            command: format!(
                "--ir is only available for swift, python, kotlin, and kmp, not {}",
                target_label(other)
            ),
            status: None,
        }),
    }
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

    let selected = SelectedCrate::resolve(config, options)?;
    let output_directory = swift_output_directory(config, options);
    let ffi_module = config
        .apple_swift_ffi_module_name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}FFI", config.xcframework_name()));

    Generation::new(selected.manifest_path)
        .cargo_args(selected.cargo_args)
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

fn generate_python(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_python_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.python.enabled = false".to_string(),
            status: None,
        });
    }

    let selected = SelectedCrate::resolve(config, options)?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.python_output());

    write_python(
        config,
        output_directory,
        selected.manifest_path,
        selected.artifact_name,
        selected.cargo_args,
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

    let selected = SelectedCrate::resolve(config, options)?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.android_kotlin_output());

    Generation::new(selected.manifest_path)
        .cargo_args(selected.cargo_args)
        .kotlin_package(config.android_kotlin_package())
        .kotlin_file(config.android_kotlin_module_name())
        .kotlin_api_style(kotlin_api_style(config.android_kotlin_api_style()))
        .kotlin_factory_style(kotlin_factory_style(config.android_kotlin_factory_style()))
        .kotlin_custom_mappings(config.android_kotlin_custom_mappings())
        .kotlin_android_library(config.resolved_android_kotlin_library_name())
        .kotlin_desktop_jni_library(format!(
            "{}_jni",
            config.resolved_android_kotlin_desktop_library_name()
        ))
        .kotlin_desktop_fallback_library(config.resolved_android_kotlin_desktop_library_name())
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
        .cargo_args_for_command("generate")
        .into_iter()
        .chain(options.cargo_args.iter().cloned())
        .collect::<Vec<_>>();
    let cargo = Cargo::current(&cargo_args)?;
    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let package_selector =
        cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
    let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.kotlin_multiplatform_output());

    write_kmp(
        config,
        output_directory,
        package.manifest_path.clone(),
        cargo.probe_command_arguments(),
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
    )
}

fn write_python(
    config: &Config,
    output_directory: PathBuf,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
) -> Result<()> {
    Generation::new(manifest_path)
        .cargo_args(cargo_args)
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
    cargo_args: Vec<String>,
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

    let output = Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .coverage_mode(coverage)
        .kmp_package_name(config.kotlin_multiplatform_package())
        .kmp_module_name(config.kotlin_multiplatform_module_name())
        .kmp_min_sdk(config.android_min_sdk())
        .kmp_support_mode(support_mode)
        .render(Target::KotlinMultiplatform)
        .map_err(|error| generation_error("kmp", error))?;

    write_kmp_output(output, &output_directory)
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

struct SelectedCrate {
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
}

impl SelectedCrate {
    fn resolve(config: &Config, options: &GenerateOptions) -> Result<Self> {
        let cargo_args = config
            .cargo_args_for_command("generate")
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
        Ok(Self {
            manifest_path: package.manifest_path.clone(),
            artifact_name: library_target.name.clone(),
            cargo_args: cargo.probe_command_arguments(),
        })
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
        write_kmp_output,
    };
    use crate::{cli::CliError, config::Config};
    use boltffi_backend::{FilePath, GeneratedFile, GeneratedOutput};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
