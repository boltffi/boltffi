use std::path::{Component, Path, PathBuf};

use boltffi_bindgen::render::java::{JavaEmitter, JavaOptions, JavaVersion};
use boltffi_bindgen::render::jni::{JniEmitter, JniLowerer, JvmBindingStyle};

use boltffi_bindgen::target::Target;

use crate::cli::{CliError, Result};
use crate::commands::generate::generator::{
    GenerateRequest, LanguageGenerator, ScanPointerWidth, SourceCrate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JavaGenerationMode {
    Jvm,
    Android,
}

impl JavaGenerationMode {
    fn scan_pointer_width(self) -> ScanPointerWidth {
        match self {
            Self::Jvm => ScanPointerWidth::Host,
            Self::Android => ScanPointerWidth::Flexible,
        }
    }
}

pub struct JavaGenerator;

impl JavaGenerator {
    pub fn generate_from_source_directory(
        config: &crate::config::Config,
        output_override: Option<PathBuf>,
        source_directory: &Path,
        crate_name: &str,
        cargo_args: Vec<String>,
    ) -> Result<()> {
        let request = GenerateRequest::new(
            config,
            output_override,
            SourceCrate::new(source_directory, crate_name),
        )
        .cargo_args(cargo_args);

        Self::generate(&request)
    }

    fn normalized_output_path(path: &Path) -> PathBuf {
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current_directory| current_directory.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };

        absolute_path
            .components()
            .fold(PathBuf::new(), |mut normalized_path, component| {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if normalized_path.file_name().is_some() {
                            normalized_path.pop();
                        }
                    }
                    _ => normalized_path.push(component.as_os_str()),
                }

                normalized_path
            })
    }

    fn generation_mode(
        request: &GenerateRequest<'_>,
        output_directory: &Path,
    ) -> JavaGenerationMode {
        let jvm_enabled = request.config().is_java_jvm_enabled();
        let android_enabled = request.config().is_java_android_enabled();
        let normalized_output_directory = Self::normalized_output_path(output_directory);
        let normalized_jvm_output =
            Self::normalized_output_path(&request.config().java_jvm_output());
        let normalized_android_output =
            Self::normalized_output_path(&request.config().java_android_output());

        if normalized_output_directory == normalized_jvm_output {
            return JavaGenerationMode::Jvm;
        }

        if normalized_output_directory == normalized_android_output {
            return JavaGenerationMode::Android;
        }

        match (jvm_enabled, android_enabled) {
            (true, false) | (true, true) => JavaGenerationMode::Jvm,
            (false, true) => JavaGenerationMode::Android,
            (false, false) => JavaGenerationMode::Jvm,
        }
    }
}

impl LanguageGenerator for JavaGenerator {
    const TARGET: Target = Target::Java;

    fn generate(request: &GenerateRequest<'_>) -> Result<()> {
        let jvm_enabled = request.config().is_java_jvm_enabled();
        let android_enabled = request.config().is_java_android_enabled();

        if !jvm_enabled && !android_enabled {
            return Err(CliError::CommandFailed {
                command: "both targets.java.jvm.enabled and targets.java.android.enabled are false"
                    .to_string(),
                status: None,
            });
        }

        let package_name = request.config().java_package();
        let package_path = package_name.replace('.', "/");
        let module_name = request.config().java_module_name();
        let output_directory = request
            .output_override()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                if jvm_enabled {
                    request.config().java_jvm_output()
                } else {
                    request.config().java_android_output()
                }
            });
        let java_directory = output_directory.join(&package_path);
        let jni_directory = output_directory.join("jni");

        request.ensure_output_directory(&java_directory)?;
        request.ensure_output_directory(&jni_directory)?;

        let generation_mode = Self::generation_mode(request, &output_directory);
        let lowered_crate = request.lowered_crate(generation_mode.scan_pointer_width())?;
        let library_name = match generation_mode {
            JavaGenerationMode::Jvm => {
                boltffi_bindgen::library_name(request.source_crate().crate_name())
            }
            JavaGenerationMode::Android => {
                boltffi_bindgen::load_library_name(request.source_crate().crate_name())
            }
        };
        let java_output = JavaEmitter::emit(
            &lowered_crate.ffi_contract,
            &lowered_crate.abi_contract,
            package_name.clone(),
            module_name.clone(),
            JavaOptions {
                library_name: Some(library_name),
                min_java_version: JavaVersion(request.config().java_min_version().unwrap_or(8)),
                desktop_loader: matches!(generation_mode, JavaGenerationMode::Jvm),
            },
        );

        java_output.files.iter().try_for_each(|java_file| {
            request.write_output(
                &java_directory.join(&java_file.file_name),
                &java_file.source,
            )
        })?;

        let jni_module = JniLowerer::new(
            &lowered_crate.ffi_contract,
            &lowered_crate.abi_contract,
            package_name,
            module_name,
        )
        .with_jvm_binding_style(JvmBindingStyle::Java)
        .lower();
        let jni_source = JniEmitter::emit(&jni_module);
        let jni_path = jni_directory.join("jni_glue.c");

        request.write_output(&jni_path, jni_source)
    }
}
