use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use boltffi_backend::target::kmp::{
    KMP_GENERATED_C_HEADER_DIR, KMP_SUPPORT_REPORT_SCHEMA_VERSION, KmpPlatform, KmpSupportMetadata,
    KmpSupportMode,
};
use boltffi_bindgen::target::Target;

use crate::build::BindingExpansion;
use crate::cli::{CliError, Result};
use crate::commands::generate::{
    run_generate_c_header_with_manifest, run_generate_kmp_with_manifest,
};
use crate::commands::pack::PackKmpOptions;
use crate::config::Config;
use crate::pack::PackError;
use crate::pack::android::{AndroidBindingMode, AndroidPackager, build_android_targets};
use crate::pack::java::link::{build_jvm_native_library, compile_jni_library_with_layout};
use crate::pack::java::outputs::remove_stale_structured_jvm_outputs;
use crate::pack::java::prepare_kmp_jvm_packaging;
use crate::pack::{missing_built_libraries, resolve_build_cargo_args};
use crate::reporter::Reporter;
use crate::target::Platform;

mod layout;
mod plan;

use plan::KmpPackagingPlan;

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

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let selected_crate = BindingExpansion::resolve(config, &build_cargo_args)?;

    let step = reporter.step("Validating JVM toolchains");
    let prepared_jvm_packaging = prepare_kmp_jvm_packaging(
        config,
        options.execution.release,
        &options.execution.cargo_args,
        &selected_crate,
    )?;
    step.finish_success();

    let plan = KmpPackagingPlan::new(config, prepared_jvm_packaging)?;
    let fallback_header_name = plan.fallback_header_name();

    let header_name = if options.execution.regenerate {
        let generation_cargo_args = plan.generation_cargo_args(options.execution.release);
        let generation_toolchain_selector = plan.generation_toolchain_selector().map(str::to_owned);

        let step = reporter.step("Generating Kotlin Multiplatform bindings");
        run_generate_kmp_with_manifest(
            config,
            Some(plan.layout().output_root().clone()),
            plan.manifest_path().to_path_buf(),
            plan.artifact_name().to_string(),
            generation_cargo_args.clone(),
            generation_toolchain_selector.clone(),
        )?;
        step.finish_success();

        let header_name = read_kmp_jni_header_name(plan.layout(), &fallback_header_name)?;

        let step = reporter.step("Generating JVM C header");
        run_generate_c_header_with_manifest(
            plan.layout().jvm_jni_dir().clone(),
            plan.manifest_path().to_path_buf(),
            header_name.clone(),
            generation_cargo_args.clone(),
            generation_toolchain_selector.clone(),
        )?;
        step.finish_success();

        let step = reporter.step("Generating Android C header");
        run_generate_c_header_with_manifest(
            plan.layout().android_jni_dir().clone(),
            plan.manifest_path().to_path_buf(),
            header_name.clone(),
            generation_cargo_args,
            generation_toolchain_selector,
        )?;
        step.finish_success();

        header_name
    } else {
        read_kmp_jni_header_name(plan.layout(), &fallback_header_name)?
    };

    remove_stale_kmp_root_headers(plan.layout())?;
    verify_kmp_support_metadata(config, plan.layout())?;
    package_kmp_android_libraries(
        config,
        &options,
        &plan,
        &header_name,
        &selected_crate,
        reporter,
    )?;

    let kmp_jvm_layout = plan.layout().jvm_native_layout(config, &header_name)?;
    for packaging_target in &plan.jvm_packaging().packaging_targets {
        let host_target = packaging_target.cargo_context.host_target;
        let step = reporter.step(&format!(
            "Building Rust library for {}",
            host_target.canonical_name()
        ));
        let build_artifacts = build_jvm_native_library(
            packaging_target,
            options.execution.release,
            Some(&selected_crate),
            &step,
        )?;
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

    remove_stale_structured_jvm_outputs(
        plan.layout().jvm_native_resource_root(),
        &plan.jvm_packaging().host_targets,
    )?;

    reporter.finish();
    Ok(())
}

fn verify_kmp_support_metadata(config: &Config, layout: &layout::KmpPackageLayout) -> Result<()> {
    let path = layout.support_report_path();
    let contents = fs::read_to_string(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            CliError::FileNotFound(path.to_path_buf())
        } else {
            CliError::ReadFailed {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    let report = serde_json::from_str::<KmpSupportMetadata>(&contents).map_err(|source| {
        CliError::CommandFailed {
            command: format!("read {}: {source}", path.display()),
            status: None,
        }
    })?;

    validate_kmp_support_report(config, &report)
}

fn validate_kmp_support_report(config: &Config, report: &KmpSupportMetadata) -> Result<()> {
    if report.schema_version != KMP_SUPPORT_REPORT_SCHEMA_VERSION {
        return Err(kmp_support_metadata_error(format!(
            "expected schema_version {}, found {}",
            KMP_SUPPORT_REPORT_SCHEMA_VERSION, report.schema_version
        )));
    }

    let expected_policy = kmp_support_policy(config);
    if report.mode != expected_policy {
        return Err(kmp_support_metadata_error(format!(
            "expected mode {:?}, found {:?}",
            expected_policy, report.mode
        )));
    }

    let expected_platforms = KmpPlatform::default_selected()
        .iter()
        .map(|platform| platform.label().to_string())
        .collect::<Vec<_>>();
    if report.selected_platforms != expected_platforms {
        return Err(kmp_support_metadata_error(format!(
            "expected selected_platforms {:?}, found {:?}",
            expected_platforms, report.selected_platforms
        )));
    }

    let expected_package = config.kotlin_multiplatform_package();
    if report.package_name != expected_package {
        return Err(kmp_support_metadata_error(format!(
            "expected package_name {}, found {}",
            expected_package, report.package_name
        )));
    }

    let expected_module = config.kotlin_multiplatform_module_name();
    if report.module_name != expected_module {
        return Err(kmp_support_metadata_error(format!(
            "expected module_name {}, found {}",
            expected_module, report.module_name
        )));
    }

    let expected_min_sdk = config.android_min_sdk();
    if report.min_sdk != expected_min_sdk {
        return Err(kmp_support_metadata_error(format!(
            "expected min_sdk {}, found {}",
            expected_min_sdk, report.min_sdk
        )));
    }

    if report.mode == KmpSupportMode::Strict && !report.rejected_apis.is_empty() {
        return Err(kmp_support_metadata_error(
            "strict support report contains rejected APIs".to_string(),
        ));
    }

    Ok(())
}

fn kmp_support_policy(config: &Config) -> KmpSupportMode {
    if config.kotlin_multiplatform_preview_prune_unsupported() {
        KmpSupportMode::PreviewPruneUnsupported
    } else {
        KmpSupportMode::Strict
    }
}

fn kmp_support_metadata_error(reason: String) -> CliError {
    CliError::CommandFailed {
        command: format!("KMP support metadata mismatch: {reason}"),
        status: None,
    }
}

fn read_kmp_jni_header_name(
    layout: &layout::KmpPackageLayout,
    fallback_header_name: &str,
) -> Result<String> {
    let fallback_header_basename = fallback_header_name;
    let fallback_header_name = kmp_generated_header_name(fallback_header_basename);
    let jvm_header_name =
        read_kmp_jni_header_name_from_glue(layout.jvm_jni_dir(), fallback_header_basename)?;
    let android_header_name =
        read_kmp_jni_header_name_from_glue(layout.android_jni_dir(), fallback_header_basename)?;

    match (jvm_header_name, android_header_name) {
        (Some(jvm_header_name), Some(android_header_name))
            if jvm_header_name == android_header_name =>
        {
            Ok(jvm_header_name)
        }
        (None, None) => Ok(fallback_header_name),
        (Some(jvm_header_name), Some(android_header_name)) => Err(CliError::CommandFailed {
            command: format!(
                "KMP JNI glue includes mismatched generated headers: JVM uses `{jvm_header_name}.h` and Android uses `{android_header_name}.h`; regenerate the KMP bindings"
            ),
            status: None,
        }),
        (Some(jvm_header_name), None) => Err(CliError::CommandFailed {
            command: format!(
                "KMP JNI glue includes mismatched generated headers: JVM uses `{jvm_header_name}.h` but Android has no generated header include; regenerate the KMP bindings"
            ),
            status: None,
        }),
        (None, Some(android_header_name)) => Err(CliError::CommandFailed {
            command: format!(
                "KMP JNI glue includes mismatched generated headers: Android uses `{android_header_name}.h` but JVM has no generated header include; regenerate the KMP bindings"
            ),
            status: None,
        }),
    }
}

fn read_kmp_jni_header_name_from_glue(
    jni_dir: &Path,
    expected_header_basename: &str,
) -> Result<Option<String>> {
    let jni_glue_path = jni_dir.join("jni_glue.c");
    let source = fs::read_to_string(&jni_glue_path).map_err(|source| CliError::ReadFailed {
        path: jni_glue_path.clone(),
        source,
    })?;
    validate_kmp_jni_header_name_from_source(&source, expected_header_basename)
        .map_err(|error| kmp_jni_header_validation_error(error, &jni_glue_path))
}

fn kmp_generated_header_name(header_name: &str) -> String {
    format!("{KMP_GENERATED_C_HEADER_DIR}/{header_name}")
}

fn kmp_generated_header_name_is_qualified(header_name: &str) -> bool {
    header_name
        .strip_prefix(KMP_GENERATED_C_HEADER_DIR)
        .and_then(|header_name| header_name.strip_prefix('/'))
        .is_some_and(kmp_generated_header_basename_is_safe)
}

fn kmp_generated_header_basename_is_safe(header_name: &str) -> bool {
    !header_name.is_empty()
        && header_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[cfg(test)]
fn kmp_jni_header_name_from_source(source: &str) -> Option<String> {
    validate_kmp_jni_header_name_from_source(source, "__boltffi_test_header__")
        .ok()
        .flatten()
}

#[derive(Debug)]
enum KmpJniHeaderValidationError {
    UnsupportedHeader(String),
    DuplicateGeneratedHeader { first: String, second: String },
}

fn validate_kmp_jni_header_name_from_source(
    source: &str,
    expected_header_basename: &str,
) -> std::result::Result<Option<String>, KmpJniHeaderValidationError> {
    let mut generated_header = None;
    let mut expected_preamble_seen = false;

    for header_name in kmp_jni_header_names_from_source(source) {
        if kmp_jni_preamble_header_name(&header_name) {
            if header_name == expected_header_basename {
                if expected_preamble_seen {
                    return Err(KmpJniHeaderValidationError::UnsupportedHeader(header_name));
                }
                expected_preamble_seen = true;
            }
            continue;
        }

        if !kmp_generated_header_name_is_qualified(&header_name) {
            return Err(KmpJniHeaderValidationError::UnsupportedHeader(header_name));
        }

        if let Some(first) = generated_header {
            return Err(KmpJniHeaderValidationError::DuplicateGeneratedHeader {
                first,
                second: header_name,
            });
        }

        generated_header = Some(header_name);
    }

    Ok(generated_header)
}

fn kmp_jni_header_names_from_source(source: &str) -> impl Iterator<Item = String> + '_ {
    source.lines().filter_map(|line| {
        let header = line.trim().strip_prefix("#include <")?.strip_suffix('>')?;
        let header_name = header.strip_suffix(".h")?;
        Some(header_name.to_string())
    })
}

fn kmp_jni_preamble_header_name(header_name: &str) -> bool {
    matches!(
        header_name,
        "jni"
            | "stdint"
            | "stdbool"
            | "stdio"
            | "stdlib"
            | "string"
            | "limits"
            | "stdatomic"
            | "pthread"
    )
}

fn kmp_jni_header_validation_error(
    error: KmpJniHeaderValidationError,
    jni_glue_path: &Path,
) -> CliError {
    let command = match error {
        KmpJniHeaderValidationError::UnsupportedHeader(header_name) => format!(
            "KMP JNI glue includes unsupported generated header `{header_name}.h` in {}; regenerate the KMP bindings so generated C headers live under `{KMP_GENERATED_C_HEADER_DIR}/` and stale generated includes are removed",
            jni_glue_path.display()
        ),
        KmpJniHeaderValidationError::DuplicateGeneratedHeader { first, second } => format!(
            "KMP JNI glue includes multiple generated headers `{first}.h` and `{second}.h` in {}; regenerate the KMP bindings",
            jni_glue_path.display()
        ),
    };
    CliError::CommandFailed {
        command,
        status: None,
    }
}

fn remove_stale_kmp_root_headers(layout: &layout::KmpPackageLayout) -> Result<()> {
    remove_stale_kmp_root_headers_in_dir(layout.jvm_jni_dir())?;
    remove_stale_kmp_root_headers_in_dir(layout.android_jni_dir())
}

fn remove_stale_kmp_root_headers_in_dir(jni_dir: &Path) -> Result<()> {
    let entries = match fs::read_dir(jni_dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CliError::ReadFailed {
                path: jni_dir.to_path_buf(),
                source,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|source| CliError::ReadFailed {
            path: jni_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CliError::ReadFailed {
            path: path.clone(),
            source,
        })?;
        if file_type.is_file() && path.extension() == Some(OsStr::new("h")) {
            fs::remove_file(&path).map_err(|source| CliError::WriteFailed { path, source })?;
        }
    }

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
    plan: &KmpPackagingPlan,
    header_name: &str,
    binding_expansion: &BindingExpansion,
    reporter: &Reporter,
) -> Result<()> {
    let android_targets = config.android_targets();

    let step = reporter.step("Building Android targets for Kotlin Multiplatform");
    build_android_targets(
        config,
        &android_targets,
        options.execution.release,
        binding_expansion,
        &step,
    )?;
    step.finish_success();

    let libraries = crate::target::BuiltLibrary::discover_for_targets(
        plan.target_directory(),
        plan.artifact_name(),
        plan.build_profile().output_directory_name(),
        &android_targets,
    );
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

    let packager = AndroidPackager::new_with_layout(
        config,
        android_libraries,
        plan.build_profile().is_release_like(),
        AndroidBindingMode::KotlinMultiplatform,
        plan.layout().android_native_layout(header_name),
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

#[cfg(test)]
mod tests {
    use super::layout::KmpPackageLayout;
    use super::{
        ensure_kmp_no_build_supported, ensure_kmp_packaging_enabled,
        kmp_jni_header_name_from_source, validate_kmp_support_report, verify_kmp_support_metadata,
    };
    use crate::cli::CliError;
    use crate::config::Config;
    use boltffi_backend::target::kmp::{
        KMP_SUPPORT_REPORT_SCHEMA_VERSION, KmpPlatform, KmpSupportApiMetadata, KmpSupportMetadata,
        KmpSupportMode,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
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

    fn write_kmp_jni_glue(output_directory: &Path, source_set: &str, source: &str) -> PathBuf {
        let jni_glue = output_directory.join(format!("src/{source_set}/c/jni_glue.c"));
        fs::create_dir_all(jni_glue.parent().expect("jni glue path has parent"))
            .expect("create jni glue directory");
        fs::write(&jni_glue, source).expect("write jni glue");
        jni_glue
    }

    fn write_matching_kmp_jni_glue(output_directory: &Path, source: &str) {
        write_kmp_jni_glue(output_directory, "jvmMain", source);
        write_kmp_jni_glue(output_directory, "androidMain", source);
    }

    #[test]
    fn kmp_jni_header_name_uses_generated_package_include() {
        let jni_glue = r#"
#include <jni.h>
#include <stdint.h>
#include <stdbool.h>
#include <boltffi_generated/workspace_member.h>
"#;

        assert_eq!(
            kmp_jni_header_name_from_source(jni_glue).as_deref(),
            Some("boltffi_generated/workspace_member")
        );
    }

    #[test]
    fn kmp_jni_header_parser_uses_last_generated_include() {
        let jni_glue = r#"
#include <jni.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include <boltffi_generated/string.h>
"#;

        assert_eq!(
            kmp_jni_header_name_from_source(jni_glue).as_deref(),
            Some("boltffi_generated/string")
        );
    }

    #[test]
    fn kmp_jni_header_name_falls_back_for_comment_only_glue() {
        let output_directory = unique_temp_dir("boltffi-kmp-comment-only-glue-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "workspace-member"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        write_matching_kmp_jni_glue(&output_directory, "// No JNI functions emitted.\n");

        assert_eq!(
            super::read_kmp_jni_header_name(&layout, "workspace_member")
                .expect("comment-only glue should use fallback header"),
            "boltffi_generated/workspace_member"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn kmp_jni_header_name_accepts_path_qualified_system_header_basename() {
        let output_directory = unique_temp_dir("boltffi-kmp-path-qualified-header-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "jni"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        write_matching_kmp_jni_glue(
            &output_directory,
            "#include <jni.h>\n#include <stdint.h>\n#include <boltffi_generated/jni.h>\n",
        );

        assert_eq!(
            super::read_kmp_jni_header_name(&layout, "jni")
                .expect("path-qualified generated header should not shadow preamble headers"),
            "boltffi_generated/jni"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn kmp_jni_header_name_rejects_duplicate_preamble_named_stale_include() {
        let output_directory = unique_temp_dir("boltffi-kmp-duplicate-preamble-header-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "jni"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        let jni_glue = write_kmp_jni_glue(
            &output_directory,
            "jvmMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <jni.h>\n",
        );
        write_kmp_jni_glue(
            &output_directory,
            "androidMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <jni.h>\n",
        );

        let error = super::read_kmp_jni_header_name(&layout, "jni")
            .expect_err("duplicate preamble-named stale include should fail");

        assert!(
            matches!(&error, CliError::CommandFailed { command, status: None }
                if command.contains("KMP JNI glue includes unsupported generated header `jni.h`")
                    && command.contains(&jni_glue.display().to_string())),
            "{error:?}"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn kmp_jni_header_name_rejects_stale_unqualified_generated_include() {
        let output_directory = unique_temp_dir("boltffi-kmp-stale-unqualified-header-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        write_kmp_jni_glue(
            &output_directory,
            "jvmMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <boltffi_generated/demo.h>\n",
        );
        let android_jni_glue = write_kmp_jni_glue(
            &output_directory,
            "androidMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <demo.h>\n",
        );

        let error = super::read_kmp_jni_header_name(&layout, "demo")
            .expect_err("stale unqualified glue should fail");

        assert!(
            matches!(&error, CliError::CommandFailed { command, status: None }
                if command.contains("KMP JNI glue includes unsupported generated header `demo.h`")
                    && command.contains(&android_jni_glue.display().to_string())),
            "{error:?}"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn kmp_jni_header_name_rejects_path_like_generated_include() {
        let output_directory = unique_temp_dir("boltffi-kmp-path-like-header-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        let jni_glue = write_kmp_jni_glue(
            &output_directory,
            "jvmMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <boltffi_generated/..\\jni.h>\n",
        );
        write_kmp_jni_glue(
            &output_directory,
            "androidMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <boltffi_generated/..\\jni.h>\n",
        );

        let error = super::read_kmp_jni_header_name(&layout, "demo")
            .expect_err("path-like generated include should fail");

        assert!(
            matches!(&error, CliError::CommandFailed { command, status: None }
                if command.contains("KMP JNI glue includes unsupported generated header `boltffi_generated/..\\jni.h`")
                    && command.contains(&jni_glue.display().to_string())),
            "{error:?}"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn kmp_jni_header_name_rejects_stale_include_before_valid_generated_include() {
        let output_directory = unique_temp_dir("boltffi-kmp-stale-before-valid-header-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        let jni_glue = write_kmp_jni_glue(
            &output_directory,
            "jvmMain",
            "#include <jni.h>\n#include <demo.h>\n#include <boltffi_generated/demo.h>\n",
        );
        write_kmp_jni_glue(
            &output_directory,
            "androidMain",
            "#include <jni.h>\n#include <boltffi_generated/demo.h>\n",
        );

        let error = super::read_kmp_jni_header_name(&layout, "demo")
            .expect_err("stale include before valid generated include should fail");

        assert!(
            matches!(&error, CliError::CommandFailed { command, status: None }
                if command.contains("KMP JNI glue includes unsupported generated header `demo.h`")
                    && command.contains(&jni_glue.display().to_string())),
            "{error:?}"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn kmp_jni_header_name_rejects_mismatched_jvm_android_generated_includes() {
        let output_directory = unique_temp_dir("boltffi-kmp-mismatched-header-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        write_kmp_jni_glue(
            &output_directory,
            "jvmMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <boltffi_generated/demo.h>\n",
        );
        write_kmp_jni_glue(
            &output_directory,
            "androidMain",
            "#include <jni.h>\n#include <stdint.h>\n#include <boltffi_generated/other.h>\n",
        );

        let error = super::read_kmp_jni_header_name(&layout, "demo")
            .expect_err("mismatched glue should fail");

        assert!(
            matches!(&error, CliError::CommandFailed { command, status: None }
                if command.contains("JVM uses `boltffi_generated/demo.h`")
                    && command.contains("Android uses `boltffi_generated/other.h`")),
            "{error:?}"
        );

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    #[test]
    fn remove_stale_kmp_root_headers_removes_flat_headers_only() {
        let output_directory = unique_temp_dir("boltffi-kmp-root-header-cleanup-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);
        let jvm_flat_header = output_directory.join("src/jvmMain/c/jni.h");
        let android_flat_header = output_directory.join("src/androidMain/c/stdint.h");
        let jvm_glue = output_directory.join("src/jvmMain/c/jni_glue.c");
        let android_glue = output_directory.join("src/androidMain/c/jni_glue.c");
        let jvm_nested_header = output_directory.join("src/jvmMain/c/boltffi_generated/demo.h");
        let android_nested_header =
            output_directory.join("src/androidMain/c/boltffi_generated/demo.h");

        for path in [
            &jvm_flat_header,
            &android_flat_header,
            &jvm_glue,
            &android_glue,
            &jvm_nested_header,
            &android_nested_header,
        ] {
            fs::create_dir_all(path.parent().expect("test path has parent"))
                .expect("create test directory");
            fs::write(path, []).expect("write test file");
        }

        super::remove_stale_kmp_root_headers(&layout).expect("cleanup should succeed");

        assert!(!jvm_flat_header.exists());
        assert!(!android_flat_header.exists());
        assert!(jvm_glue.exists());
        assert!(android_glue.exists());
        assert!(jvm_nested_header.exists());
        assert!(android_nested_header.exists());

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }

    fn support_report(config: &Config, mode: KmpSupportMode) -> KmpSupportMetadata {
        KmpSupportMetadata {
            schema_version: KMP_SUPPORT_REPORT_SCHEMA_VERSION,
            mode,
            selected_platforms: KmpPlatform::default_selected()
                .iter()
                .map(|platform| platform.label().to_string())
                .collect(),
            package_name: config.kotlin_multiplatform_package(),
            module_name: config.kotlin_multiplatform_module_name(),
            min_sdk: config.android_min_sdk(),
            admitted_apis: vec![KmpSupportApiMetadata {
                kind: "function".to_string(),
                name: "ping".to_string(),
                reason: None,
            }],
            rejected_apis: Vec::new(),
            generator_version: "test".to_string(),
        }
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
        let layout = KmpPackageLayout::from_config(&config);
        let android_layout = layout.android_native_layout("demo_lib");
        let jvm_layout = layout.jvm_native_layout(&config, "demo-lib").unwrap();

        assert_eq!(layout.output_root(), &PathBuf::from("dist/kmp"));
        assert_eq!(
            android_layout.jni_glue_path,
            PathBuf::from("dist/kmp/src/androidMain/c/jni_glue.c")
        );
        assert_eq!(
            android_layout.header_include_dir,
            PathBuf::from("dist/kmp/src/androidMain/c")
        );
        assert_eq!(android_layout.header_name, "demo_lib");
        assert_eq!(
            android_layout.jnilibs_path,
            PathBuf::from("dist/kmp/src/androidMain/jniLibs")
        );
        assert_eq!(
            layout.android_jni_dir(),
            &PathBuf::from("dist/kmp/src/androidMain/c")
        );
        assert_eq!(
            layout.jvm_jni_dir(),
            &PathBuf::from("dist/kmp/src/jvmMain/c")
        );
        assert_eq!(
            layout.jvm_native_resource_root(),
            &PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert_eq!(
            layout.support_report_path(),
            &PathBuf::from("dist/kmp/boltffi-kmp-support.json")
        );
        assert_eq!(jvm_layout.jni_dir, PathBuf::from("dist/kmp/src/jvmMain/c"));
        assert_eq!(jvm_layout.header_name, "demo-lib");
        assert_eq!(jvm_layout.jni_library_name.as_str(), "demo_lib_jni");
        assert_eq!(
            jvm_layout.native_output_root,
            PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert!(jvm_layout.flat_output_root.is_none());
        assert!(!jvm_layout.strip_symbols);
        assert!(!jvm_layout.debug_symbols_enabled);
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
        let layout = KmpPackageLayout::from_config(&config);
        let jvm_layout = layout.jvm_native_layout(&config, "demo").unwrap();

        assert_eq!(
            layout.jvm_jni_dir(),
            &PathBuf::from("dist/kmp/src/jvmMain/c")
        );
        assert_eq!(
            layout.jvm_native_resource_root(),
            &PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert_eq!(jvm_layout.jni_dir, PathBuf::from("dist/kmp/src/jvmMain/c"));
        assert_eq!(jvm_layout.header_name, "demo");
        assert_eq!(
            jvm_layout.jni_library_name.as_str(),
            "configured_library_jni"
        );
        assert_eq!(
            jvm_layout.native_output_root,
            PathBuf::from("dist/kmp/src/jvmMain/resources/native")
        );
        assert!(jvm_layout.flat_output_root.is_none());
        assert!(!jvm_layout.strip_symbols);
        assert!(!jvm_layout.debug_symbols_enabled);
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
        let layout = KmpPackageLayout::from_config(&config)
            .jvm_native_layout(&config, "demo")
            .unwrap();

        assert!(config.java_jvm_strip_symbols());
        assert!(config.java_jvm_debug_symbols_enabled());
        assert!(layout.flat_output_root.is_none());
        assert!(!layout.strip_symbols);
        assert!(!layout.debug_symbols_enabled);
    }

    #[test]
    fn kmp_support_metadata_accepts_matching_strict_report() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"
package = "com.example.demo"
module_name = "Demo"
"#,
        );
        let report = support_report(&config, KmpSupportMode::Strict);

        validate_kmp_support_report(&config, &report).expect("strict report should match config");
    }

    #[test]
    fn kmp_support_metadata_accepts_matching_preview_report() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"
package = "com.example.demo"
module_name = "Demo"
preview_prune_unsupported = true
"#,
        );
        let mut report = support_report(&config, KmpSupportMode::PreviewPruneUnsupported);
        report.rejected_apis.push(KmpSupportApiMetadata {
            kind: "class".to_string(),
            name: "Service".to_string(),
            reason: Some("class APIs are not supported".to_string()),
        });

        validate_kmp_support_report(&config, &report).expect("preview report should match config");
    }

    #[test]
    fn kmp_support_metadata_rejects_preview_report_when_config_is_strict() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"
package = "com.example.demo"
module_name = "Demo"
"#,
        );
        let report = support_report(&config, KmpSupportMode::PreviewPruneUnsupported);

        let error = validate_kmp_support_report(&config, &report)
            .expect_err("preview report should not package under strict config");

        assert!(
            matches!(error, CliError::CommandFailed { command, status: None }
                if command.contains("KMP support metadata mismatch")
                    && command.contains("expected mode Strict"))
        );
    }

    #[test]
    fn kmp_support_metadata_rejects_missing_report_file() {
        let output_directory = unique_temp_dir("boltffi-kmp-missing-support-report-test");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
package = "com.example.demo"
module_name = "Demo"
"#,
            output_directory.display()
        ));
        let layout = KmpPackageLayout::from_config(&config);

        let error = verify_kmp_support_metadata(&config, &layout)
            .expect_err("missing support report should fail packaging");

        assert!(matches!(
            error,
            CliError::FileNotFound(path) if path == *layout.support_report_path()
        ));

        if output_directory.exists() {
            fs::remove_dir_all(output_directory).expect("cleanup generated output");
        }
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
