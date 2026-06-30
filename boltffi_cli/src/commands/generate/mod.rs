mod generator;
mod header;
mod ir;
mod languages;

use std::path::{Path, PathBuf};

#[cfg(test)]
use boltffi_bindgen::CHeaderLowerer;
#[cfg(test)]
use generator::ScanPointerWidth;
use generator::{GenerateRequest, run_generator};
use header::HeaderGenerator;
use languages::{CSharpGenerator, DartGenerator, JavaGenerator, TypeScriptGenerator};

use boltffi_bindgen::target::Target;

use crate::cli::Result;
use crate::config::Config;

pub enum GenerateTarget {
    Swift,
    Kotlin,
    KotlinMultiplatform,
    Java,
    Header,
    Typescript,
    Dart,
    Python,
    CSharp,
    All,
}

pub struct GenerateOptions {
    pub target: GenerateTarget,
    pub output: Option<PathBuf>,
    pub experimental: bool,
    pub ir: bool,
    pub cargo_args: Vec<String>,
}

pub fn run_generate_with_output(config: &Config, options: GenerateOptions) -> Result<()> {
    if options.ir {
        return ir::run_ir_generation(config, &options);
    }

    let legacy_request = || GenerateRequest::for_current_crate(config, options.output.clone());

    match &options.target {
        GenerateTarget::Swift => ir::run_ir_generation(config, &options),
        GenerateTarget::Kotlin => ir::run_ir_generation(config, &options),
        GenerateTarget::KotlinMultiplatform => ir::run_ir_generation(config, &options),
        GenerateTarget::Java => {
            run_generator::<JavaGenerator>(&legacy_request(), options.experimental)
        }
        GenerateTarget::Header => {
            run_generator::<HeaderGenerator>(&legacy_request(), options.experimental)
        }
        GenerateTarget::Typescript => {
            run_generator::<TypeScriptGenerator>(&legacy_request(), options.experimental)
        }
        GenerateTarget::Dart => {
            run_generator::<DartGenerator>(&legacy_request(), options.experimental)
        }
        GenerateTarget::Python => ir::run_ir_generation(config, &options),
        GenerateTarget::CSharp => {
            run_generator::<CSharpGenerator>(&legacy_request(), options.experimental)
        }
        GenerateTarget::All => {
            let request = legacy_request();

            if config.should_process(Target::Swift, options.experimental) {
                ir::run_ir_generation(
                    config,
                    &GenerateOptions {
                        target: GenerateTarget::Swift,
                        output: options.output.clone(),
                        experimental: options.experimental,
                        ir: true,
                        cargo_args: options.cargo_args.clone(),
                    },
                )?;
            }

            if config.should_process(Target::Kotlin, options.experimental) {
                ir::run_ir_generation(
                    config,
                    &GenerateOptions {
                        target: GenerateTarget::Kotlin,
                        output: options.output.clone(),
                        experimental: options.experimental,
                        ir: true,
                        cargo_args: options.cargo_args.clone(),
                    },
                )?;
            }

            if config.should_process(Target::KotlinMultiplatform, options.experimental) {
                ir::run_ir_generation(
                    config,
                    &GenerateOptions {
                        target: GenerateTarget::KotlinMultiplatform,
                        output: options.output.clone(),
                        experimental: options.experimental,
                        ir: true,
                        cargo_args: options.cargo_args.clone(),
                    },
                )?;
            }

            if config.should_process(Target::Java, options.experimental) {
                run_generator::<JavaGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::TypeScript, options.experimental) {
                run_generator::<TypeScriptGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Dart, options.experimental) {
                run_generator::<DartGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Python, options.experimental) {
                ir::run_ir_generation(
                    config,
                    &GenerateOptions {
                        target: GenerateTarget::Python,
                        output: options.output.clone(),
                        experimental: options.experimental,
                        ir: true,
                        cargo_args: options.cargo_args.clone(),
                    },
                )?;
            }

            if config.should_process(Target::CSharp, options.experimental) {
                run_generator::<CSharpGenerator>(&request, options.experimental)?;
            }

            Ok(())
        }
    }
}

pub fn run_generate_java_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    JavaGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

#[cfg(test)]
pub fn run_generate_header_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    let output_directory = output
        .as_ref()
        .cloned()
        .unwrap_or_else(|| config.android_header_output());
    let request = GenerateRequest::new(
        config,
        output,
        generator::SourceCrate::new(source_directory, crate_name),
    );

    let output_path = output_directory.join(format!("{}.h", config.library_name()));

    request.ensure_output_directory(&output_directory)?;
    let lowered_crate = request.lowered_crate(ScanPointerWidth::Flexible)?;
    let header_source =
        CHeaderLowerer::new(&lowered_crate.ffi_contract, &lowered_crate.abi_contract).generate();

    request.write_output(&output_path, header_source)
}

pub fn run_generate_python_with_manifest(
    config: &Config,
    output: Option<PathBuf>,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
) -> Result<()> {
    ir::run_python_generation(config, output, manifest_path, artifact_name, cargo_args)
}

pub fn run_generate_kmp_with_manifest(
    config: &Config,
    output: Option<PathBuf>,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    ir::run_kmp_generation(
        config,
        output,
        manifest_path,
        artifact_name,
        cargo_args,
        toolchain_selector,
    )
}

pub fn run_generate_c_header_with_manifest(
    output_directory: PathBuf,
    manifest_path: PathBuf,
    header_name: String,
    cargo_args: Vec<String>,
    toolchain_selector: Option<String>,
) -> Result<()> {
    ir::run_c_header_generation(
        output_directory,
        manifest_path,
        header_name,
        cargo_args,
        toolchain_selector,
    )
}

pub fn run_generate_csharp_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    CSharpGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_bindgen::render::kmp::{
        KMP_SUPPORT_REPORT_FILE, KmpSupportPolicy, KmpSupportReport,
    };

    use super::languages::KMPGenerator;
    use crate::config::Config;

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

    fn demo_source_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo")
    }

    #[test]
    fn kotlin_multiplatform_generate_uses_ir_route_without_ir_flag() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        let error = super::run_generate_with_output(
            &config,
            super::GenerateOptions {
                target: super::GenerateTarget::KotlinMultiplatform,
                output: None,
                experimental: false,
                ir: false,
                cargo_args: vec![
                    "--manifest-path".to_string(),
                    "/definitely/missing/boltffi/Cargo.toml".to_string(),
                ],
            },
        )
        .expect_err("production KMP generation should use IR cargo selection");

        assert!(
            matches!(error, crate::cli::CliError::CommandFailed { command, .. }
                if command.contains("cargo metadata --format-version 1 --no-deps"))
        );
    }

    #[test]
    fn header_from_source_directory_supports_kmp_only_config() {
        let output_directory = unique_temp_dir("boltffi-kmp-header-generate-test");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"
version = "0.1.0"

[targets.apple]
enabled = false

[targets.android]
enabled = false

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        super::run_generate_header_with_output_from_source_dir(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
        )
        .expect("kmp-only header generation should succeed");

        let header_path = output_directory.join("demo.h");
        let header = fs::read_to_string(&header_path).expect("header should be readable");

        assert!(header.contains("boltffi"));
        assert!(header.contains("BoltFFICallbackHandle"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }

    #[test]
    fn kotlin_multiplatform_generate_writes_kmp_sources() {
        let output_directory = unique_temp_dir("boltffi-kmp-generate-test");
        let stale_common_path = output_directory.join("src/commonMain/kotlin/com/old/Stale.kt");
        let staged_native_path =
            output_directory.join("src/jvmMain/resources/native/current/libdemo.so");
        fs::create_dir_all(stale_common_path.parent().expect("stale path has parent"))
            .expect("create stale source directory");
        fs::write(&stale_common_path, "package com.old\n").expect("write stale source");
        fs::create_dir_all(staged_native_path.parent().expect("native path has parent"))
            .expect("create native resource directory");
        fs::write(&staged_native_path, []).expect("write native resource");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = true
package = "com.boltffi.demo"
preview_prune_unsupported = true

[targets.android.kotlin.type_mappings]
Email = { type = "java.net.URI", conversion = "url_string" }
"#,
        );

        KMPGenerator::generate_from_source_directory_with_desktop_fallback_library_name(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
            None,
        )
        .expect("kotlin multiplatform generate should succeed");

        let common_path = output_directory.join("src/commonMain/kotlin/com/boltffi/demo/Demo.kt");
        let jvm_actual_path =
            output_directory.join("src/jvmMain/kotlin/com/boltffi/demo/DemoJvmActual.kt");
        let android_actual_path =
            output_directory.join("src/androidMain/kotlin/com/boltffi/demo/DemoAndroidActual.kt");
        let jvm_internal_path =
            output_directory.join("src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt");
        let jni_glue_path = output_directory.join("src/jvmMain/c/jni_glue.c");
        let build_gradle_path = output_directory.join("build.gradle.kts");
        let settings_gradle_path = output_directory.join("settings.gradle.kts");
        let support_report_path = output_directory.join(KMP_SUPPORT_REPORT_FILE);

        let common = fs::read_to_string(&common_path).expect("common source should be readable");
        let jvm_actual =
            fs::read_to_string(&jvm_actual_path).expect("jvm actual should be readable");
        let android_actual =
            fs::read_to_string(&android_actual_path).expect("android actual should be readable");
        let jvm_internal =
            fs::read_to_string(&jvm_internal_path).expect("jvm source should be readable");
        let jni_glue = fs::read_to_string(&jni_glue_path).expect("jni glue should be readable");
        let build_gradle =
            fs::read_to_string(&build_gradle_path).expect("gradle file should be readable");
        let settings_gradle =
            fs::read_to_string(&settings_gradle_path).expect("settings file should be readable");
        let support_report: KmpSupportReport = serde_json::from_str(
            &fs::read_to_string(&support_report_path).expect("support report should be readable"),
        )
        .expect("support report should be valid JSON");

        assert!(common.contains("package com.boltffi.demo"));
        assert!(common.contains("typealias Email = String"));
        assert!(common.contains(
            "class FfiException(val code: kotlin.Int, message: kotlin.String) : kotlin.Exception(message)"
        ));
        assert!(common.contains("sealed class BoltFFIResult<out T, out E>"));
        assert!(common.contains("data class Point("));
        assert!(common.contains("sealed class MathError : kotlin.Exception()"));
        assert!(common.contains("data class AppError("));
        assert!(common.contains("sealed class ComputeError : kotlin.Exception()"));
        assert!(common.contains("data class Triangle(val a: com.boltffi.demo.Point"));
        assert!(common.contains("data class BenchmarkResponse("));
        assert!(common.contains("val result: BoltFFIResult<DataPoint, ComputeError>"));
        assert!(common.contains("enum class LogLevel(val value: Byte)"));
        assert!(common.contains("expect fun echoBytes"));
        assert!(common.contains("expect fun checkedDivide(a: Int, b: Int): Int"));
        assert!(
            common.contains("expect fun resultToString(v: BoltFFIResult<Int, String>): String")
        );
        assert!(!common.contains("Unsupported in the initial KMP generator slice"));
        assert_eq!(
            support_report.mode,
            KmpSupportPolicy::PreviewPruneUnsupported
        );
        assert!(!support_report.rejected_apis.is_empty());
        assert!(jvm_actual.contains("actual fun echoBytes"));
        assert!(jvm_actual.contains("actual fun checkedDivide(a: Int, b: Int): Int"));
        assert!(jvm_actual.contains("catch (err: com.boltffi.demo.jvm.MathError)"));
        assert!(jvm_actual.contains("catch (err: com.boltffi.demo.jvm.FfiException)"));
        assert!(jvm_actual.contains("private fun MathError.toBoltFfiJvm()"));
        assert!(
            jvm_actual.contains("private fun com.boltffi.demo.jvm.MathError.toBoltFfiCommon()")
        );
        assert!(jvm_actual.contains("com.boltffi.demo.jvm.echoBytes"));
        assert!(jvm_actual.contains("toBoltFfiJvm"));
        assert_eq!(jvm_actual, android_actual);
        assert!(jvm_internal.contains("package com.boltffi.demo.jvm"));
        assert!(jvm_internal.contains("typealias Email = String"));
        assert!(jvm_internal.contains("@JvmStatic external fun"));
        assert!(jni_glue.contains("JNIEXPORT"));
        assert!(build_gradle.contains("kotlin(\"multiplatform\")"));
        assert!(build_gradle.contains("kotlin(\"multiplatform\") version \"2.3.21\""));
        assert!(build_gradle.contains("kotlinx-coroutines-core:1.11.0"));
        assert!(build_gradle.contains("import org.jetbrains.kotlin.gradle.dsl.JvmTarget"));
        assert!(build_gradle.contains("jvmTarget.set(JvmTarget.JVM_1_8)"));
        assert!(build_gradle.contains("androidTarget {"));
        assert!(build_gradle.contains("sourceCompatibility = JavaVersion.VERSION_1_8"));
        assert!(build_gradle.contains("targetCompatibility = JavaVersion.VERSION_1_8"));
        assert!(!build_gradle.contains("repositories {"));
        assert!(settings_gradle.contains("pluginManagement"));
        assert!(settings_gradle.contains("gradlePluginPortal()"));
        assert!(settings_gradle.contains("RepositoriesMode.FAIL_ON_PROJECT_REPOS"));
        assert!(!stale_common_path.exists());
        assert!(staged_native_path.exists());

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }

    #[test]
    fn kotlin_multiplatform_generate_fails_unsupported_surface_by_default() {
        let output_directory = unique_temp_dir("boltffi-kmp-strict-generate-test");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = true
package = "com.boltffi.demo"
"#,
        );

        let error =
            KMPGenerator::generate_from_source_directory_with_desktop_fallback_library_name(
                &config,
                Some(output_directory.clone()),
                &demo_source_directory(),
                "demo",
                None,
            )
            .expect_err("strict KMP generation should reject unsupported demo APIs");

        assert!(
            matches!(error, crate::cli::CliError::CommandFailed { command, status: None }
                if command.contains("unsupported KMP APIs in strict mode"))
        );

        if output_directory.exists() {
            fs::remove_dir_all(output_directory).expect("cleanup generated output");
        }
    }

    #[test]
    fn kotlin_multiplatform_generate_uses_configured_native_load_name() {
        let output_directory = unique_temp_dir("boltffi-kmp-generate-load-name-test");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "my-lib"
version = "0.1.0"

[targets.android.kotlin]
library_name = "configured-library"

[targets.kotlin_multiplatform]
enabled = true
package = "com.boltffi.demo"
module_name = "Demo"
preview_prune_unsupported = true
"#,
        );

        KMPGenerator::generate_from_source_directory_with_desktop_fallback_library_name(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "my-lib",
            None,
        )
        .expect("kotlin multiplatform generate should succeed");

        let jvm_internal_path =
            output_directory.join("src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt");
        let jni_glue_path = output_directory.join("src/jvmMain/c/jni_glue.c");
        let jvm_internal =
            fs::read_to_string(&jvm_internal_path).expect("jvm source should be readable");
        let jni_glue = fs::read_to_string(&jni_glue_path).expect("jni glue should be readable");

        assert!(jvm_internal.contains("val androidLibrary = \"configured-library\""));
        assert!(jvm_internal.contains("val desktopPreferredLibrary = \"configured_library_jni\""));
        assert!(jvm_internal.contains("val desktopFallbackLibrary = \"my_lib\""));
        assert!(jni_glue.contains("#include <boltffi_generated/my-lib.h>"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }
}
