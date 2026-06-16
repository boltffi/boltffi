use std::path::PathBuf;

use crate::cli::{CliError, Result};
use crate::commands::generate::{
    run_generate_header_with_output_from_source_dir,
    run_generate_kmp_with_output_from_source_dir_and_desktop_fallback_library_name,
};
use crate::commands::pack::PackKmpOptions;
use crate::config::{Config, Target};
use crate::pack::PackError;
use crate::pack::android::{AndroidBindingMode, AndroidPackager, build_android_targets};
use crate::pack::java::link::{
    JvmNativePackageLayout, build_jvm_native_library, compile_jni_library_with_layout,
};
use crate::pack::java::{
    generate_jvm_header, prepare_kmp_jvm_packaging, selected_jvm_package_artifact_name,
    selected_jvm_package_source_directory,
};
use crate::pack::{
    discover_built_libraries_for_targets, missing_built_libraries, resolve_build_cargo_args,
};
use crate::reporter::Reporter;
use crate::target::Platform;

pub(crate) fn pack_kmp(
    config: &Config,
    options: PackKmpOptions,
    reporter: &Reporter,
) -> Result<()> {
    ensure_kmp_packaging_enabled(config, options.experimental)?;
    ensure_kmp_no_build_supported(
        config,
        options.execution.no_build,
        options.experimental,
        "pack kmp",
    )?;

    reporter.section("🧩", "Packing Kotlin Multiplatform");

    let step = reporter.step("Validating JVM toolchains");
    let prepared_jvm_packaging = prepare_kmp_jvm_packaging(
        config,
        options.execution.release,
        &options.execution.cargo_args,
    )?;
    step.finish_success();

    let source_directory =
        selected_jvm_package_source_directory(&prepared_jvm_packaging.packaging_targets)?;
    let artifact_name =
        selected_jvm_package_artifact_name(&prepared_jvm_packaging.packaging_targets)?;
    let source_crate_name = config.library_name();
    let jni_dir = kmp_jvm_jni_dir(config);

    if options.execution.regenerate {
        let step = reporter.step("Generating Kotlin Multiplatform bindings");
        run_generate_kmp_with_output_from_source_dir_and_desktop_fallback_library_name(
            config,
            Some(config.kotlin_multiplatform_output()),
            &source_directory,
            source_crate_name,
            artifact_name,
        )?;
        step.finish_success();

        let step = reporter.step("Generating JVM C header");
        generate_jvm_header(
            &source_directory,
            source_crate_name,
            &jni_dir,
            source_crate_name,
        )?;
        step.finish_success();

        let step = reporter.step("Generating Android C header");
        run_generate_header_with_output_from_source_dir(
            config,
            Some(config.android_header_output()),
            &source_directory,
            source_crate_name,
        )?;
        step.finish_success();
    }

    package_kmp_android_libraries(config, &options, reporter)?;

    let kmp_jvm_layout = kmp_jvm_native_layout(config, source_crate_name);
    for packaging_target in &prepared_jvm_packaging.packaging_targets {
        let host_target = packaging_target.cargo_context.host_target;
        let step = reporter.step(&format!(
            "Building Rust library for {}",
            host_target.canonical_name()
        ));
        let build_artifacts =
            build_jvm_native_library(packaging_target, options.execution.release, &step)?;
        step.finish_success();

        let step = reporter.step(&format!(
            "Compiling JVM JNI library for {}",
            host_target.canonical_name()
        ));
        compile_jni_library_with_layout(
            packaging_target,
            &build_artifacts,
            &kmp_jvm_layout,
            &step,
        )?;
        step.finish_success();
    }

    // Sibling `native/<arch>/` directories are intentionally preserved; each
    // rebuilt arch cleans only its own subdir (see compile_jni_library_with_layout).

    reporter.finish();
    Ok(())
}

fn ensure_kmp_packaging_enabled(config: &Config, experimental_flag: bool) -> Result<()> {
    if !config.is_kotlin_multiplatform_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.kotlin_multiplatform.enabled = false".to_string(),
            status: None,
        });
    }

    if config.should_process(Target::KotlinMultiplatform, experimental_flag) {
        return Ok(());
    }

    Err(CliError::CommandFailed {
        command: format!(
            "{} is experimental, use --experimental flag or add \"{}\" to [experimental]",
            Target::KotlinMultiplatform.name(),
            Target::KotlinMultiplatform.name()
        ),
        status: None,
    })
}

fn package_kmp_android_libraries(
    config: &Config,
    options: &PackKmpOptions,
    reporter: &Reporter,
) -> Result<()> {
    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile =
        crate::build::resolve_build_profile(options.execution.release, &build_cargo_args);
    let android_targets = config.android_targets();

    let step = reporter.step("Building Android targets for Kotlin Multiplatform");
    build_android_targets(
        config,
        &android_targets,
        options.execution.release,
        &build_cargo_args,
        None,
        &step,
    )?;
    step.finish_success();

    let libraries = discover_built_libraries_for_targets(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
        &android_targets,
    )?;
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

    let packager = AndroidPackager::new(
        config,
        android_libraries,
        build_profile.is_release_like(),
        AndroidBindingMode::KotlinMultiplatform,
    );
    let step = reporter.step("Packaging Android jniLibs for Kotlin Multiplatform");
    packager.package()?;
    step.finish_success();

    Ok(())
}

pub(crate) fn ensure_kmp_no_build_supported(
    config: &Config,
    no_build: bool,
    experimental: bool,
    command_name: &str,
) -> Result<()> {
    if no_build && config.should_process(Target::KotlinMultiplatform, experimental) {
        return Err(CliError::CommandFailed {
            command: format!(
                "{command_name} --no-build is unsupported while Kotlin Multiplatform native packaging is enabled; rerun without --no-build"
            ),
            status: None,
        });
    }

    Ok(())
}

fn kmp_jvm_jni_dir(config: &Config) -> PathBuf {
    config.kotlin_multiplatform_output().join("src/jvmMain/c")
}

fn kmp_jvm_native_resource_root(config: &Config) -> PathBuf {
    config
        .kotlin_multiplatform_output()
        .join("src/jvmMain/resources/native")
}

fn kmp_jvm_native_layout(config: &Config, header_name: &str) -> JvmNativePackageLayout {
    JvmNativePackageLayout {
        jni_dir: kmp_jvm_jni_dir(config),
        header_name: header_name.to_string(),
        jni_library_name: config.resolved_android_kotlin_desktop_library_name(),
        native_output_root: kmp_jvm_native_resource_root(config),
        flat_output_root: None,
        strip_symbols: false,
        debug_symbols_enabled: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_kmp_no_build_supported, ensure_kmp_packaging_enabled, kmp_jvm_jni_dir,
        kmp_jvm_native_layout, kmp_jvm_native_resource_root,
    };
    use crate::cli::CliError;
    use crate::config::Config;
    use std::path::PathBuf;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    #[test]
    fn kmp_jvm_paths_use_generated_kmp_project_layout() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo-lib"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"
module_name = "Demo"
"#,
        );
        let layout = kmp_jvm_native_layout(&config, "demo-lib");

        assert_eq!(
            kmp_jvm_jni_dir(&config),
            PathBuf::from("dist/kmp/src/jvmMain/c")
        );
        assert_eq!(
            kmp_jvm_native_resource_root(&config),
            PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert_eq!(layout.jni_dir, PathBuf::from("dist/kmp/src/jvmMain/c"));
        assert_eq!(layout.header_name, "demo-lib");
        assert_eq!(layout.jni_library_name, "demo_lib");
        assert_eq!(
            layout.native_output_root,
            PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert!(layout.flat_output_root.is_none());
        assert!(!layout.strip_symbols);
        assert!(!layout.debug_symbols_enabled);
    }

    #[test]
    fn kmp_jvm_layout_uses_configured_kotlin_library_name_for_jni_output() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.android.kotlin]
library_name = "configured-library"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"
module_name = "Demo"
"#,
        );
        let layout = kmp_jvm_native_layout(&config, "demo");

        assert_eq!(
            kmp_jvm_jni_dir(&config),
            PathBuf::from("dist/kmp/src/jvmMain/c")
        );
        assert_eq!(
            kmp_jvm_native_resource_root(&config),
            PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert_eq!(layout.jni_dir, PathBuf::from("dist/kmp/src/jvmMain/c"));
        assert_eq!(layout.header_name, "demo");
        assert_eq!(layout.jni_library_name, "configured_library");
        assert_eq!(
            layout.native_output_root,
            PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert!(layout.flat_output_root.is_none());
        assert!(!layout.strip_symbols);
        assert!(!layout.debug_symbols_enabled);
    }

    #[test]
    fn kmp_jvm_layout_does_not_inherit_java_strip_or_debug_symbols_policy() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"

[targets.java.jvm]
strip_symbols = true

[targets.java.jvm.debug_symbols]
enabled = true
"#,
        );
        let layout = kmp_jvm_native_layout(&config, "demo");

        assert!(config.java_jvm_strip_symbols());
        assert!(config.java_jvm_debug_symbols_enabled());
        assert!(layout.flat_output_root.is_none());
        assert!(!layout.strip_symbols);
        assert!(!layout.debug_symbols_enabled);
    }

    #[test]
    fn kmp_packaging_requires_enabled_target() {
        let config = parse_config(
            r#"
[package]
name = "demo"
"#,
        );

        let error = ensure_kmp_packaging_enabled(&config, true).expect_err("target disabled");

        assert!(
            matches!(error, CliError::CommandFailed { command, .. } if command == "targets.kotlin_multiplatform.enabled = false")
        );
    }

    #[test]
    fn kmp_packaging_requires_experimental_opt_in() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        let error = ensure_kmp_packaging_enabled(&config, false).expect_err("missing opt-in");

        assert!(
            matches!(error, CliError::CommandFailed { command, .. } if command.contains("kotlin_multiplatform is experimental"))
        );
    }

    #[test]
    fn kmp_packaging_accepts_config_opt_in() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        ensure_kmp_packaging_enabled(&config, false).expect("config opt-in");
    }

    #[test]
    fn kmp_packaging_accepts_flag_opt_in() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        ensure_kmp_packaging_enabled(&config, true).expect("flag opt-in");
    }

    #[test]
    fn rejects_no_build_when_kmp_packaging_is_enabled() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        let error = ensure_kmp_no_build_supported(&config, true, false, "pack all")
            .expect_err("expected no-build rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("pack all --no-build is unsupported")
                    && command.contains("Kotlin Multiplatform native packaging")
        ));
    }

    #[test]
    fn allows_no_build_when_kmp_packaging_is_not_selected() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        ensure_kmp_no_build_supported(&config, true, false, "pack all")
            .expect("unselected KMP target should not reject no-build");
    }
}
