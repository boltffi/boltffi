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
use crate::target::{BuiltLibrary, Platform};

use super::{
    discover_built_libraries_for_targets, missing_built_libraries, print_cargo_line,
    resolve_build_cargo_args,
};

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
                ir: false,
                cargo_args: build_cargo_args.clone(),
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
                ir: false,
                cargo_args: build_cargo_args.clone(),
            },
        )?;
        step.finish_success();
    }

    let libraries = match binding_expansion.as_ref() {
        Some(expansion) => BuiltLibrary::discover_for_targets(
            expansion.target_directory(),
            expansion.artifact_name(),
            build_profile.output_directory_name(),
            &android_targets,
        ),
        None => discover_built_libraries_for_targets(
            &config.crate_artifact_name(),
            build_profile.output_directory_name(),
            &android_targets,
        )?,
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
        android_kotlin_desktop_native_layout, should_package_android_kotlin_desktop_natives,
    };
    use crate::config::Config;
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
            &bundled_enabled
        ));
        assert!(!should_package_android_kotlin_desktop_natives(
            &bundled_disabled
        ));
        assert!(!should_package_android_kotlin_desktop_natives(
            &system_loader
        ));
    }
}
