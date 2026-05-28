use std::path::{Path, PathBuf};

use boltffi_bindgen::render::csharp::{CSharpEmitter, CSharpOptions};

use crate::cli::{CliError, Result};
use crate::commands::generate::generator::{
    GenerateRequest, LanguageGenerator, ScanPointerWidth, SourceCrate,
};
use crate::config::{Config, Target};

pub struct CSharpGenerator;

impl CSharpGenerator {
    fn csharp_options(request: &GenerateRequest<'_>) -> CSharpOptions {
        CSharpOptions {
            namespace: request.config().csharp_namespace().map(str::to_string),
            library_name: Some(boltffi_bindgen::library_name(
                request.source_crate().crate_name(),
            )),
        }
    }

    pub fn generate_from_source_directory(
        config: &Config,
        output: Option<PathBuf>,
        source_directory: &Path,
        crate_name: &str,
    ) -> Result<()> {
        let request = GenerateRequest::new(
            config,
            output,
            SourceCrate::new(source_directory, crate_name),
        );
        Self::generate(&request)
    }
}

impl LanguageGenerator for CSharpGenerator {
    const TARGET: Target = Target::CSharp;

    fn generate(request: &GenerateRequest<'_>) -> Result<()> {
        if !request.config().is_csharp_enabled() {
            return Err(CliError::CommandFailed {
                command: "targets.csharp.enabled = false".to_string(),
                status: None,
            });
        }

        let output_directory = request
            .output_override()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| request.config().csharp_output());

        request.ensure_output_directory(&output_directory)?;

        let lowered_crate = request.lowered_crate(ScanPointerWidth::Host)?;
        let output = CSharpEmitter::emit(
            &lowered_crate.ffi_contract,
            &lowered_crate.abi_contract,
            &Self::csharp_options(request),
        );

        output.files.iter().try_for_each(|file| {
            request.write_output(&output_directory.join(&file.file_name), &file.source)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CSharpConfig, CargoConfig, PackageConfig, TargetsConfig};

    fn config(namespace: Option<String>) -> Config {
        Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "logical-package".to_string(),
                crate_name: Some("native-crate".to_string()),
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig {
                csharp: CSharpConfig {
                    enabled: true,
                    namespace,
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    #[test]
    fn csharp_options_use_configured_namespace_and_source_crate_library_name() {
        let config = config(Some("CounterApp.Shared".to_string()));
        let request = GenerateRequest::new(&config, None, SourceCrate::new(".", "native-crate"));

        let options = CSharpGenerator::csharp_options(&request);

        assert_eq!(options.namespace.as_deref(), Some("CounterApp.Shared"));
        assert_eq!(options.library_name.unwrap().as_str(), "native_crate");
    }
}
