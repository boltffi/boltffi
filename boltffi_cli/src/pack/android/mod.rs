mod link;

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
use crate::target::{Architecture, BuiltLibrary, Platform, RustTarget};

use super::{
    discover_built_libraries_for_targets, missing_built_libraries, print_cargo_line,
    resolve_build_cargo_args,
};

pub(crate) use self::link::{AndroidPackScope, AndroidPackageLayout, AndroidPackager};

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

    let parts = AndroidPackParts::for_run(config, &options)?;
    ensure_android_kotlin_desktop_no_build_supported(
        options.execution.no_build,
        parts.desktop_natives,
    )?;
    let android_targets = selected_android_targets(config, &options.architectures)?;

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let binding_expansion = (!options.execution.no_build)
        .then(|| BindingExpansion::resolve(config, &build_cargo_args))
        .transpose()?;
    let build_profile =
        crate::build::resolve_build_profile(options.execution.release, &build_cargo_args);

    if let Some(binding_expansion) = binding_expansion.as_ref().filter(|_| parts.architectures) {
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

    if parts.architectures {
        package_android_architectures(
            config,
            &options,
            binding_expansion.as_ref(),
            &build_profile,
            &android_targets,
            reporter,
        )?;
    }

    if parts.desktop_natives {
        package_android_kotlin_desktop_natives(
            config,
            &options,
            binding_expansion.as_ref(),
            reporter,
        )?;
    }

    Ok(())
}

/// Which parts of `pack android` a run builds and packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AndroidPackParts {
    /// The selected Android architectures, packaged into jniLibs. Off for
    /// `--desktop-only`, which leaves the jniLibs already on disk as they are.
    architectures: bool,
    /// The Kotlin desktop natives, where the configuration enables them and
    /// `--skip-desktop` does not turn them off.
    desktop_natives: bool,
}

impl AndroidPackParts {
    /// A run that would pack neither part would build and package nothing and
    /// still succeed, so it is refused the same way an architecture missing
    /// from the configuration is.
    fn for_run(config: &Config, options: &PackAndroidOptions) -> Result<Self> {
        // the parser already keeps these apart; options built in code must too
        if options.desktop_only && options.skip_desktop {
            return Err(CliError::CommandFailed {
                command: "pack android cannot combine --desktop-only with --skip-desktop"
                    .to_string(),
                status: None,
            });
        }

        let parts = Self {
            architectures: !options.desktop_only,
            desktop_natives: should_package_android_kotlin_desktop_natives(
                config,
                options.skip_desktop,
            ),
        };
        if !parts.architectures && !parts.desktop_natives {
            return Err(CliError::CommandFailed {
                command: "pack android --desktop-only needs targets.android.kotlin.desktop_pack.enabled = true with the bundled desktop_loader".to_string(),
                status: None,
            });
        }

        Ok(parts)
    }
}

fn package_android_architectures(
    config: &Config,
    options: &PackAndroidOptions,
    binding_expansion: Option<&BindingExpansion>,
    build_profile: &crate::build::CargoBuildProfile,
    android_targets: &[RustTarget],
    reporter: &Reporter,
) -> Result<()> {
    let libraries = match binding_expansion {
        Some(expansion) => BuiltLibrary::discover_for_targets(
            expansion.target_directory(),
            expansion.artifact_name(),
            build_profile.output_directory_name(),
            android_targets,
        ),
        None => discover_built_libraries_for_targets(
            &config.crate_artifact_name(),
            build_profile.output_directory_name(),
            android_targets,
        )?,
    };
    let android_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|library| library.target.platform() == Platform::Android)
        .collect();

    let missing_targets = missing_built_libraries(android_targets, &android_libraries);
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

    let packager = AndroidPackager::new(config, android_libraries, build_profile.is_release_like())
        .with_scope(AndroidPackScope::of(config, android_targets));
    let step = reporter.step("Packaging jniLibs");
    let output = packager.package()?;
    step.finish_success();

    if !output.kept_abis.is_empty() {
        reporter.warning(&format!(
            "kept jniLibs for {} from an earlier run without relinking them; pack those too if the bindings changed",
            output.kept_abis.join(", ")
        ));
    }

    Ok(())
}

fn ensure_android_kotlin_desktop_no_build_supported(
    no_build: bool,
    desktop_natives: bool,
) -> Result<()> {
    if no_build && desktop_natives {
        return Err(CliError::CommandFailed {
            command: "pack android --no-build is unsupported while Kotlin desktop native packaging is enabled; rerun without --no-build".to_string(),
            status: None,
        });
    }

    Ok(())
}

/// The configured Android targets, narrowed to `architectures` when it asks for
/// some. An architecture the configuration does not carry is an error rather
/// than an empty build, so a typo cannot quietly produce a partial package.
fn selected_android_targets(
    config: &Config,
    architectures: &[Architecture],
) -> Result<Vec<RustTarget>> {
    let configured = config.android_targets();
    if architectures.is_empty() {
        return Ok(configured);
    }

    let unknown = architectures
        .iter()
        .filter(|architecture| {
            !configured
                .iter()
                .any(|target| target.architecture() == **architecture)
        })
        .map(|architecture| architecture.canonical_name())
        .fold(Vec::new(), |mut unknown, name| {
            if !unknown.contains(&name) {
                unknown.push(name);
            }
            unknown
        });
    if !unknown.is_empty() {
        let (noun, verb) = match unknown.len() {
            1 => ("architecture", "is"),
            _ => ("architectures", "are"),
        };
        return Err(CliError::CommandFailed {
            command: format!(
                "{noun} {} {verb} not configured under targets.android.architectures",
                unknown.join(", ")
            ),
            status: None,
        });
    }

    Ok(configured
        .into_iter()
        .filter(|target| architectures.contains(&target.architecture()))
        .collect())
}

fn should_package_android_kotlin_desktop_natives(config: &Config, skip_desktop: bool) -> bool {
    !skip_desktop
        && config.android_kotlin_desktop_pack_enabled()
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
    targets: &[crate::target::RustTarget],
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
    let results = builder.build_android(targets)?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}

#[cfg(test)]
mod tests {
    use super::{
        AndroidPackParts, android_kotlin_desktop_native_layout, selected_android_targets,
        should_package_android_kotlin_desktop_natives,
    };
    use crate::cli::CliError;
    use crate::commands::pack::{PackAndroidOptions, PackExecutionOptions};
    use crate::config::Config;
    use crate::target::{Architecture, RustTarget};
    use std::path::PathBuf;

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
            &bundled_enabled,
            false
        ));
        assert!(!should_package_android_kotlin_desktop_natives(
            &bundled_disabled,
            false
        ));
        assert!(!should_package_android_kotlin_desktop_natives(
            &system_loader,
            false
        ));
        // the flag overrides a configuration that would otherwise pack them
        assert!(!should_package_android_kotlin_desktop_natives(
            &bundled_enabled,
            true
        ));
    }

    fn arm64_and_x86_64_config() -> Config {
        parse_config(
            r#"
[package]
name = "demo"

[targets.android]
architectures = ["x86_64", "arm64"]
"#,
        )
    }

    #[test]
    fn android_target_selection_defaults_to_every_configured_architecture() {
        let config = arm64_and_x86_64_config();

        assert_eq!(
            selected_android_targets(&config, &[]).unwrap(),
            config.android_targets()
        );
    }

    #[test]
    fn android_target_selection_keeps_configured_order_and_drops_duplicates() {
        let config = arm64_and_x86_64_config();

        let selected = selected_android_targets(
            &config,
            &[
                Architecture::Arm64,
                Architecture::X86_64,
                Architecture::Arm64,
            ],
        )
        .unwrap();

        assert_eq!(
            selected,
            vec![RustTarget::ANDROID_X86_64, RustTarget::ANDROID_ARM64]
        );
        assert_eq!(
            selected_android_targets(&config, &[Architecture::Arm64]).unwrap(),
            vec![RustTarget::ANDROID_ARM64]
        );
    }

    #[test]
    fn android_target_selection_rejects_unconfigured_architectures() {
        let config = arm64_and_x86_64_config();

        let error = selected_android_targets(
            &config,
            &[
                Architecture::Arm64,
                Architecture::Armv7,
                Architecture::X86,
                Architecture::Armv7,
            ],
        )
        .unwrap_err();

        let CliError::CommandFailed { command, .. } = error else {
            panic!("expected a command failure, got {error:?}");
        };
        assert_eq!(
            command,
            "architectures armv7, x86 are not configured under targets.android.architectures"
        );

        let error = selected_android_targets(&config, &[Architecture::X86]).unwrap_err();
        let CliError::CommandFailed { command, .. } = error else {
            panic!("expected a command failure, got {error:?}");
        };
        assert_eq!(
            command,
            "architecture x86 is not configured under targets.android.architectures"
        );
    }

    fn pack_options(skip_desktop: bool, desktop_only: bool) -> PackAndroidOptions {
        PackAndroidOptions {
            execution: PackExecutionOptions {
                release: false,
                regenerate: true,
                no_build: false,
                deny_skipped: false,
                cargo_args: Vec::new(),
            },
            architectures: Vec::new(),
            skip_desktop,
            desktop_only,
        }
    }

    fn parts(architectures: bool, desktop_natives: bool) -> AndroidPackParts {
        AndroidPackParts {
            architectures,
            desktop_natives,
        }
    }

    #[test]
    fn android_pack_parts_follow_the_desktop_flags() {
        let desktop_enabled = parse_config(
            r#"
[package]
name = "demo"

[targets.android.kotlin.desktop_pack]
enabled = true
"#,
        );
        let desktop_disabled = parse_config(
            r#"
[package]
name = "demo"
"#,
        );

        let for_run = |config: &Config, skip_desktop, desktop_only| {
            AndroidPackParts::for_run(config, &pack_options(skip_desktop, desktop_only))
        };

        // no flag: the configuration decides, as before
        assert_eq!(
            for_run(&desktop_enabled, false, false).unwrap(),
            parts(true, true)
        );
        assert_eq!(
            for_run(&desktop_disabled, false, false).unwrap(),
            parts(true, false)
        );
        // `--skip-desktop` keeps the architectures and drops the desktop natives
        assert_eq!(
            for_run(&desktop_enabled, true, false).unwrap(),
            parts(true, false)
        );
        // `--desktop-only` leaves jniLibs alone and packs only the desktop natives
        assert_eq!(
            for_run(&desktop_enabled, false, true).unwrap(),
            parts(false, true)
        );
    }

    #[test]
    fn android_pack_parts_refuse_a_run_that_packs_nothing() {
        let desktop_disabled = parse_config(
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
        let desktop_enabled = parse_config(
            r#"
[package]
name = "demo"

[targets.android.kotlin.desktop_pack]
enabled = true
"#,
        );

        for config in [&desktop_disabled, &system_loader] {
            let error = AndroidPackParts::for_run(config, &pack_options(false, true)).unwrap_err();
            let CliError::CommandFailed { command, .. } = error else {
                panic!("expected a command failure, got {error:?}");
            };
            assert!(command.starts_with("pack android --desktop-only needs"));
        }
        // the parser keeps the two flags apart; options built in code get the
        // same refusal instead of a message blaming the configuration
        let error =
            AndroidPackParts::for_run(&desktop_enabled, &pack_options(true, true)).unwrap_err();
        let CliError::CommandFailed { command, .. } = error else {
            panic!("expected a command failure, got {error:?}");
        };
        assert_eq!(
            command,
            "pack android cannot combine --desktop-only with --skip-desktop"
        );
    }
}
