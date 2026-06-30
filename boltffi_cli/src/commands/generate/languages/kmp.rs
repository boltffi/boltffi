use std::fs;
use std::path::{Path, PathBuf};

#[cfg(test)]
use boltffi_bindgen::KotlinOptions;
#[cfg(not(test))]
use boltffi_bindgen::render::kmp::KMP_SUPPORT_REPORT_FILE;
#[cfg(test)]
use boltffi_bindgen::render::kmp::{
    KMP_SUPPORT_REPORT_FILE, KMPEmitter, KMPOptions, KmpSupportPolicy,
};
#[cfg(test)]
use boltffi_bindgen::render::kotlin::{
    FactoryStyle as BindgenFactoryStyle, KotlinApiStyle, KotlinDesktopLoader,
};

#[cfg(test)]
use boltffi_bindgen::target::Target;

use crate::cli::{CliError, Result};
#[cfg(test)]
use crate::commands::generate::generator::SourceCrate;
#[cfg(test)]
use crate::commands::generate::generator::{GenerateRequest, LanguageGenerator, ScanPointerWidth};
#[cfg(test)]
use crate::config::KotlinFactoryStyle as ConfigKotlinFactoryStyle;

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

#[cfg(test)]
pub struct KMPGenerator;

#[cfg(test)]
impl KMPGenerator {
    pub fn generate_from_source_directory_with_desktop_fallback_library_name(
        config: &crate::config::Config,
        output_override: Option<PathBuf>,
        source_directory: &Path,
        crate_name: &str,
        desktop_fallback_library_name: Option<&str>,
    ) -> Result<()> {
        let request = GenerateRequest::new(
            config,
            output_override,
            SourceCrate::new(source_directory, crate_name),
        );

        Self::generate_with_desktop_fallback_library_name(&request, desktop_fallback_library_name)
    }

    fn kotlin_options(
        request: &GenerateRequest<'_>,
        module_name: &str,
        desktop_fallback_library_name: Option<&str>,
    ) -> KotlinOptions {
        let factory_style = match request.config().android_kotlin_factory_style() {
            ConfigKotlinFactoryStyle::Constructors => BindgenFactoryStyle::Constructors,
            ConfigKotlinFactoryStyle::CompanionMethods => BindgenFactoryStyle::CompanionMethods,
        };
        let desktop_fallback_library_name =
            desktop_fallback_library_name.unwrap_or_else(|| request.source_crate().crate_name());

        KotlinOptions {
            factory_style,
            api_style: KotlinApiStyle::TopLevel,
            module_object_name: Some(module_name.to_string()),
            library_name: Some(boltffi_bindgen::load_library_name(
                &request.config().resolved_android_kotlin_library_name(),
            )),
            desktop_jni_library_name: Some(boltffi_bindgen::library_name(
                &request
                    .config()
                    .resolved_android_kotlin_desktop_library_name(),
            )),
            desktop_fallback_library_name: Some(boltffi_bindgen::library_name(
                desktop_fallback_library_name,
            )),
            desktop_loader: KotlinDesktopLoader::Bundled,
        }
    }

    fn generate_with_desktop_fallback_library_name(
        request: &GenerateRequest<'_>,
        desktop_fallback_library_name: Option<&str>,
    ) -> Result<()> {
        if !request.config().is_kotlin_multiplatform_enabled() {
            return Err(CliError::CommandFailed {
                command: "targets.kotlin_multiplatform.enabled = false".to_string(),
                status: None,
            });
        }

        let output_directory = request
            .output_override()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| request.config().kotlin_multiplatform_output());
        request.ensure_output_directory(&output_directory)?;

        let lowered_crate = request.lowered_crate(ScanPointerWidth::Flexible)?;
        let module_name = request.config().kotlin_multiplatform_module_name();
        let support_policy = if request
            .config()
            .kotlin_multiplatform_preview_prune_unsupported()
        {
            eprintln!(
                "warning: KMP preview pruning is enabled; unsupported APIs will be omitted and recorded in {}",
                output_directory.join(KMP_SUPPORT_REPORT_FILE).display()
            );
            KmpSupportPolicy::PreviewPruneUnsupported
        } else {
            KmpSupportPolicy::Strict
        };
        let kmp_output = KMPEmitter::emit(
            &lowered_crate.ffi_contract,
            &lowered_crate.abi_contract,
            KMPOptions {
                package_name: request.config().kotlin_multiplatform_package(),
                module_name: module_name.clone(),
                min_sdk: request.config().android_min_sdk(),
                support_policy,
                kotlin_options: Self::kotlin_options(
                    request,
                    &module_name,
                    desktop_fallback_library_name,
                ),
            },
        )
        .map_err(|error| CliError::CommandFailed {
            command: format!("generate kmp: {error}"),
            status: None,
        })?;

        prepare_kmp_output_directory(request, &output_directory)?;

        kmp_output.files.iter().try_for_each(|output_file| {
            let output_path = output_directory.join(&output_file.relative_path);

            if let Some(parent_directory) = output_path.parent() {
                request.ensure_output_directory(parent_directory)?;
            }

            request.write_output(&output_path, &output_file.contents)
        })
    }
}

#[cfg(test)]
fn prepare_kmp_output_directory(
    request: &GenerateRequest<'_>,
    output_directory: &Path,
) -> Result<()> {
    request.ensure_output_directory(output_directory)?;
    remove_stale_kmp_generated_paths(output_directory)
}

pub(crate) fn remove_stale_kmp_generated_paths(output_directory: &Path) -> Result<()> {
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

#[cfg(test)]
impl LanguageGenerator for KMPGenerator {
    const TARGET: Target = Target::KotlinMultiplatform;

    fn generate(request: &GenerateRequest<'_>) -> Result<()> {
        Self::generate_with_desktop_fallback_library_name(request, None)
    }
}

#[cfg(test)]
mod tests {
    use super::remove_stale_kmp_generated_paths;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    #[test]
    fn stale_kmp_generated_cleanup_preserves_packaged_native_outputs() {
        let output_directory = unique_temp_dir("boltffi-kmp-generated-cleanup-test");
        let stale_common = output_directory.join("src/commonMain/kotlin/com/old/Old.kt");
        let stale_jvm_c = output_directory.join("src/jvmMain/c/jni_glue.c");
        let native_resource = output_directory.join("src/jvmMain/resources/native/current/lib.so");
        let android_jnilib = output_directory.join("src/androidMain/jniLibs/arm64-v8a/libdemo.so");

        for path in [
            &stale_common,
            &stale_jvm_c,
            &native_resource,
            &android_jnilib,
        ] {
            fs::create_dir_all(path.parent().expect("test path has parent"))
                .expect("create test directory");
            fs::write(path, []).expect("write test file");
        }
        fs::write(output_directory.join("build.gradle.kts"), []).expect("write stale gradle file");

        remove_stale_kmp_generated_paths(&output_directory).expect("cleanup should succeed");

        assert!(!stale_common.exists());
        assert!(!stale_jvm_c.exists());
        assert!(!output_directory.join("build.gradle.kts").exists());
        assert!(native_resource.exists());
        assert!(android_jnilib.exists());

        fs::remove_dir_all(output_directory).expect("cleanup temp output");
    }
}
