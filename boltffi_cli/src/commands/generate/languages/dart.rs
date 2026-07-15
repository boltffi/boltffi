use boltffi_bindgen::render::dart::{DartEmitter, DartLowerer};

use boltffi_bindgen::target::Target;

use crate::cli::{CliError, Result};
use crate::commands::generate::generator::{GenerateRequest, LanguageGenerator, ScanPointerWidth};

pub struct DartGenerator;

impl LanguageGenerator for DartGenerator {
    const TARGET: Target = Target::Dart;

    fn generate(request: &GenerateRequest<'_>) -> Result<()> {
        if !request.config().is_dart_enabled() {
            return Err(CliError::CommandFailed {
                command: "targets.dart.enabled = false".to_string(),
                status: None,
            });
        }

        let output_directory = request
            .output_override()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| request.config().targets.dart.output.clone());

        request.ensure_output_directory(&output_directory)?;

        let lowered_crate = request.lowered_crate(ScanPointerWidth::Fixed(32))?;
        let dart_library = DartLowerer::new(
            &lowered_crate.ffi_contract,
            &lowered_crate.abi_contract,
            &request.config().package.name,
        )
        .library();
        let package = DartEmitter::emit(&dart_library, &request.config().crate_artifact_name());

        let package_dir = output_directory.join(&request.config().package.name);
        request.ensure_output_directory(&package_dir)?;

        let package_hook_dir = package_dir.join("hook");
        request.ensure_output_directory(&package_hook_dir)?;

        let package_lib_dir = package_dir.join("lib");
        request.ensure_output_directory(&package_lib_dir)?;

        let package_pubspec = package_dir.join("pubspec.yaml");
        request.write_output(&package_pubspec, &package.pubspec)?;

        let package_build_dart = package_hook_dir.join("build.dart");
        request.write_output(&package_build_dart, &package.build)?;

        let package_lib_file =
            package_lib_dir.join(format!("{}.dart", request.config().package.name));
        request.write_output(&package_lib_file, &package.lib)?;

        Ok(())
    }
}
