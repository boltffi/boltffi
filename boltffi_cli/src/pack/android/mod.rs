mod link;

use std::path::{Path, PathBuf};

use crate::build::{
    BindingExpansion, BuildOptions, BuildSelection, Builder, OutputCallback, all_successful,
    failed_targets,
};
use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackAndroidOptions;
use crate::config::{Config, KotlinDesktopLoader};
use crate::pack::PackError;
use crate::pack::java::link::{
    JvmNativePackageLayout, build_jvm_native_library, compile_jni_library_with_layout,
};
use crate::pack::java::outputs::remove_stale_structured_jvm_outputs;
use crate::pack::java::prepare_android_kotlin_jvm_packaging;
use crate::pack::symbols::{
    ensure_debug_symbols_profile_has_debuginfo, ensure_existing_debug_symbol_artifacts_are_usable,
};
use crate::reporter::Reporter;
use crate::target::{BuiltLibrary, Platform, RustTarget};

use super::{missing_built_libraries, print_cargo_line, resolve_build_cargo_args};

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

    ensure_android_kotlin_desktop_no_build_supported(config, options.execution.no_build)?;

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let binding_expansion = (!options.execution.no_build)
        .then(|| BindingExpansion::resolve(config, &build_cargo_args))
        .transpose()?;
    let build_profile =
        crate::build::resolve_build_profile(options.execution.release, &build_cargo_args);
    let android_targets = config.android_targets();

    if let Some(binding_expansion) = binding_expansion.as_ref() {
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
            binding_expansion,
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
                cargo_args: build_cargo_args.clone(),
                deny_skipped: options.execution.deny_skipped,
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
                cargo_args: build_cargo_args.clone(),
                deny_skipped: options.execution.deny_skipped,
            },
        )?;
        step.finish_success();
    }

    let libraries = match binding_expansion.as_ref() {
        Some(expansion) => AndroidBuildDirectories::for_expansion(expansion).discover(
            expansion.artifact_name(),
            build_profile.output_directory_name(),
            &android_targets,
        ),
        None => discover_prebuilt_android_libraries(
            &super::cargo_target_directory()?,
            &config.crate_artifact_name(),
            build_profile.output_directory_name(),
            &android_targets,
        ),
    };
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

    package_android_kotlin_desktop_natives(config, &options, binding_expansion.as_ref(), reporter)?;

    Ok(())
}

fn ensure_android_kotlin_desktop_no_build_supported(config: &Config, no_build: bool) -> Result<()> {
    if no_build && should_package_android_kotlin_desktop_natives(config) {
        return Err(CliError::CommandFailed {
            command: "pack android --no-build is unsupported while Kotlin desktop native packaging is enabled; rerun without --no-build".to_string(),
            status: None,
        });
    }

    Ok(())
}

fn should_package_android_kotlin_desktop_natives(config: &Config) -> bool {
    config.android_kotlin_desktop_pack_enabled()
        && matches!(
            config.android_kotlin_desktop_loader(),
            KotlinDesktopLoader::Bundled
        )
}

fn package_android_kotlin_desktop_natives(
    config: &Config,
    options: &PackAndroidOptions,
    binding_expansion: Option<&BindingExpansion>,
    reporter: &Reporter,
) -> Result<()> {
    if !should_package_android_kotlin_desktop_natives(config) {
        return Ok(());
    }
    let binding_expansion = binding_expansion.ok_or_else(|| CliError::CommandFailed {
        command: "Kotlin desktop native packaging requires a Binding IR build".to_string(),
        status: None,
    })?;

    let step = reporter.step("Validating Kotlin desktop JVM toolchains");
    let prepared_jvm_packaging = prepare_android_kotlin_jvm_packaging(
        config,
        options.execution.release,
        &options.execution.cargo_args,
        binding_expansion,
    )?;
    step.finish_success();

    let layout = android_kotlin_desktop_native_layout(config)?;

    for packaging_target in &prepared_jvm_packaging.packaging_targets {
        let host_target = packaging_target.cargo_context.host_target;
        let step = reporter.step(&format!(
            "Building Kotlin desktop Rust library for {}",
            host_target.canonical_name()
        ));
        let build_artifacts = build_jvm_native_library(
            packaging_target,
            options.execution.release,
            Some(binding_expansion),
            &step,
        )?;
        step.finish_success();

        let step = reporter.step(&format!(
            "Compiling Kotlin desktop JNI library for {}",
            host_target.canonical_name()
        ));
        compile_jni_library_with_layout(packaging_target, &build_artifacts, &layout, &step)?;
        step.finish_success();
    }

    remove_stale_structured_jvm_outputs(
        &config.android_kotlin_desktop_pack_output(),
        &prepared_jvm_packaging.host_targets,
    )?;

    Ok(())
}

fn android_kotlin_desktop_native_layout(config: &Config) -> Result<JvmNativePackageLayout> {
    JvmNativePackageLayout::kotlin_desktop(
        config,
        config.android_kotlin_output().join("jni"),
        config.library_name(),
        config.android_kotlin_desktop_pack_output(),
    )
}

pub(crate) fn build_android_targets(
    config: &Config,
    targets: &[RustTarget],
    release: bool,
    binding_expansion: &BindingExpansion,
    step: &crate::reporter::Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        selection: BuildSelection::Expanded(Box::new(binding_expansion.clone())),
        on_output,
        extra_env: Vec::new(),
    };
    let builder = Builder::new(config, build_options);
    let directories = AndroidBuildDirectories::for_expansion(binding_expansion);
    let results = if directories.per_target {
        builder
            .build_targets_concurrently(targets, |target| directories.target_directory(target))?
    } else {
        builder.build_android(targets)?
    };

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}

/// Where [`build_android_targets`] builds each Android target.
///
/// Every target gets a cargo target directory of its own under the crate's,
/// so that their builds can run at the same time; see
/// [`Builder::build_targets_concurrently`]. Cargo arguments that already choose
/// a target directory keep it, and the targets then build one after another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AndroidBuildDirectories {
    target_directory: PathBuf,
    per_target: bool,
}

impl AndroidBuildDirectories {
    pub(crate) fn for_expansion(binding_expansion: &BindingExpansion) -> Self {
        Self::new(
            binding_expansion.target_directory(),
            binding_expansion.cargo_args().as_slice(),
        )
    }

    fn new(target_directory: &Path, cargo_args: &[String]) -> Self {
        let chooses_target_directory = cargo_args
            .iter()
            .any(|arg| arg == "--target-dir" || arg.starts_with("--target-dir="));
        Self {
            target_directory: target_directory.to_path_buf(),
            per_target: !chooses_target_directory,
        }
    }

    /// The cargo target directory `target` builds in.
    pub(crate) fn target_directory(&self, target: &RustTarget) -> PathBuf {
        if self.per_target {
            per_target_directory(&self.target_directory, target)
        } else {
            self.target_directory.clone()
        }
    }

    /// The libraries [`build_android_targets`] built for `targets`.
    pub(crate) fn discover(
        &self,
        artifact_name: &str,
        profile_directory_name: &str,
        targets: &[RustTarget],
    ) -> Vec<BuiltLibrary> {
        targets
            .iter()
            .filter_map(|target| {
                let path = target.library_path_for_profile(
                    &self.target_directory(target),
                    artifact_name,
                    profile_directory_name,
                );
                path.exists().then_some(BuiltLibrary {
                    target: *target,
                    path,
                })
            })
            .collect()
    }
}

fn per_target_directory(target_directory: &Path, target: &RustTarget) -> PathBuf {
    target_directory
        .join("boltffi")
        .join("android")
        .join(target.triple())
        .join("cargo")
}

/// The libraries a `--no-build` pack finds for `targets`: for each, the more
/// recently built of what an earlier `pack android` left in its own target
/// directory and what a plain cargo build left in the crate's.
fn discover_prebuilt_android_libraries(
    target_directory: &Path,
    artifact_name: &str,
    profile_directory_name: &str,
    targets: &[RustTarget],
) -> Vec<BuiltLibrary> {
    targets
        .iter()
        .filter_map(|target| {
            [
                per_target_directory(target_directory, target),
                target_directory.to_path_buf(),
            ]
            .into_iter()
            .map(|directory| {
                target.library_path_for_profile(&directory, artifact_name, profile_directory_name)
            })
            .filter_map(|path| {
                let modified = path
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()?;
                Some((modified, path))
            })
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, path)| BuiltLibrary {
                target: *target,
                path,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        AndroidBuildDirectories, android_kotlin_desktop_native_layout,
        discover_prebuilt_android_libraries, should_package_android_kotlin_desktop_natives,
    };
    use crate::build::BindingExpansion;
    use crate::config::Config;
    use crate::target::RustTarget;
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    #[test]
    fn android_kotlin_desktop_layout_uses_kotlin_jni_glue_and_android_output() {
        let config = parse_config(
            r#"
[package]
name = "demo-lib"

[targets.android]
output = "dist/android"

[targets.android.kotlin]
output = "dist/android/kotlin"
library_name = "configured-library"

[targets.java.jvm]
enabled = true
"#,
        );

        let layout = android_kotlin_desktop_native_layout(&config).unwrap();

        assert_eq!(layout.jni_dir, PathBuf::from("dist/android/kotlin/jni"));
        assert_eq!(layout.header_name, "demo-lib");
        assert_eq!(layout.jni_library_name.as_str(), "configured_library_jni");
        assert_eq!(
            layout.native_output_root,
            PathBuf::from("dist/android/desktopJniLibs")
        );
        assert!(layout.flat_output_root.is_none());
        assert!(!layout.strip_symbols);
        assert!(!layout.debug_symbols_enabled);
    }

    #[test]
    fn android_kotlin_desktop_packaging_is_gated_by_desktop_pack_and_bundled_loader() {
        let bundled_enabled = parse_config(
            r#"
[package]
name = "demo"

[targets.android.kotlin.desktop_pack]
enabled = true
"#,
        );
        let bundled_disabled = parse_config(
            r#"
[package]
name = "demo"
"#,
        );
        let system_loader = parse_config(
            r#"
[package]
name = "demo"

[targets.android.kotlin]
desktop_loader = "system"

[targets.android.kotlin.desktop_pack]
enabled = true
"#,
        );

        assert!(should_package_android_kotlin_desktop_natives(
            &bundled_enabled
        ));
        assert!(!should_package_android_kotlin_desktop_natives(
            &bundled_disabled
        ));
        assert!(!should_package_android_kotlin_desktop_natives(
            &system_loader
        ));
    }

    #[test]
    fn android_targets_build_in_a_target_directory_each() {
        let expansion = BindingExpansion::fixture(
            "/external/workspace/Cargo.toml",
            "/external/workspace/demo/Cargo.toml",
            [],
        );
        let directories = AndroidBuildDirectories::for_expansion(&expansion);

        assert_eq!(
            directories.target_directory(&RustTarget::ANDROID_ARM64),
            PathBuf::from("/external/workspace/target/boltffi/android/aarch64-linux-android/cargo")
        );
        assert_ne!(
            directories.target_directory(&RustTarget::ANDROID_ARM64),
            directories.target_directory(&RustTarget::ANDROID_X86_64)
        );
    }

    #[test]
    fn android_targets_keep_a_target_directory_the_cargo_args_choose() {
        let target_directory = Path::new("/external/workspace/target");

        for cargo_args in [
            vec!["--target-dir".to_string(), "/elsewhere".to_string()],
            vec!["--target-dir=/elsewhere".to_string()],
        ] {
            let directories = AndroidBuildDirectories::new(target_directory, &cargo_args);

            assert!(!directories.per_target);
            assert_eq!(
                directories.target_directory(&RustTarget::ANDROID_ARM64),
                target_directory
            );
        }
    }

    #[test]
    fn prebuilt_android_library_is_the_more_recently_built_one() {
        let target_directory =
            std::env::temp_dir().join(format!("boltffi-prebuilt-android-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&target_directory);
        let build = |directory: &Path, target: RustTarget, age: u64| {
            let path = target.library_path_for_profile(directory, "demo", "release");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            File::create(&path)
                .unwrap()
                .set_modified(SystemTime::now() - Duration::from_secs(age))
                .unwrap();
            path
        };
        let per_target =
            |target: RustTarget| super::per_target_directory(&target_directory, &target);
        // arm64: `pack android` built it last; x86_64: a plain cargo build did
        let arm64 = build(
            &per_target(RustTarget::ANDROID_ARM64),
            RustTarget::ANDROID_ARM64,
            10,
        );
        build(&target_directory, RustTarget::ANDROID_ARM64, 60);
        build(
            &per_target(RustTarget::ANDROID_X86_64),
            RustTarget::ANDROID_X86_64,
            60,
        );
        let x86_64 = build(&target_directory, RustTarget::ANDROID_X86_64, 10);

        let libraries = discover_prebuilt_android_libraries(
            &target_directory,
            "demo",
            "release",
            &[
                RustTarget::ANDROID_ARM64,
                RustTarget::ANDROID_X86_64,
                RustTarget::ANDROID_ARMV7,
            ],
        );
        let _ = std::fs::remove_dir_all(&target_directory);

        assert_eq!(
            libraries
                .into_iter()
                .map(|library| (library.target, library.path))
                .collect::<Vec<_>>(),
            vec![
                (RustTarget::ANDROID_ARM64, arm64),
                (RustTarget::ANDROID_X86_64, x86_64),
            ]
        );
    }
}
