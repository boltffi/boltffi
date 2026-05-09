use std::path::{Path, PathBuf};

use boltffi_bindgen::render::csharp::{CSharpEmitter, CSharpOptions};

use crate::cli::{CliError, Result};
use crate::commands::generate::generator::{
    GenerateRequest, LanguageGenerator, ScanPointerWidth, SourceCrate,
};
use crate::config::{Config, Target};

pub struct CSharpGenerator;

impl CSharpGenerator {
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
            &CSharpOptions::default(),
        );

        output.files.iter().try_for_each(|file| {
            request.write_output(&output_directory.join(&file.file_name), &file.source)
        })
    }
}
