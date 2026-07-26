use std::path::{Path, PathBuf};

use crate::cli::{CliError, Result};
use crate::config::{Config, Experimental};
use boltffi_bindgen::target::Target;
use boltffi_bindgen::{ir, scan_crate_with_pointer_width};

#[derive(Debug, Clone)]
pub struct SourceCrate {
    source_directory: PathBuf,
    crate_name: String,
}

impl SourceCrate {
    pub fn current(config: &Config) -> Self {
        let source_directory = std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .unwrap_or_else(|_| PathBuf::from("."));

        Self {
            source_directory,
            crate_name: config.library_name().to_string(),
        }
    }

    #[cfg(test)]
    pub fn new(source_directory: impl AsRef<Path>, crate_name: impl Into<String>) -> Self {
        Self {
            source_directory: source_directory.as_ref().to_path_buf(),
            crate_name: crate_name.into(),
        }
    }

    pub fn source_directory(&self) -> &Path {
        &self.source_directory
    }

    pub fn crate_name(&self) -> &str {
        &self.crate_name
    }
}

/// Controls how the crate scanner should treat pointer-sized layout decisions.
///
/// Some bindings are generated for a concrete data model and must resolve
/// `usize`-shaped layout as either 32-bit or 64-bit during scanning. Other
/// bindings are intentionally platform-agnostic and must not be specialized
/// that early.
///
/// This enum makes that choice explicit at the call site instead of passing
/// around a loose `Option<u8>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPointerWidth {
    /// Do not specialize pointer-sized layout during scanning.
    ///
    /// Use this when the generated surface must remain valid across multiple
    /// pointer widths, or when the backend handles the distinction later.
    Flexible,

    /// Specialize using an explicit pointer width in bits.
    ///
    /// This is used for backends with a known target data model, such as a
    /// fixed 32-bit or 64-bit ABI.
    Fixed(u8),
}

impl ScanPointerWidth {
    fn resolved_bits(self) -> Option<u8> {
        match self {
            Self::Flexible => None,
            Self::Fixed(pointer_width_bits) => Some(pointer_width_bits),
        }
    }
}

pub struct LoweredCrate {
    pub ffi_contract: ir::FfiContract,
    pub abi_contract: ir::AbiContract,
}

pub struct GenerateRequest<'a> {
    config: &'a Config,
    output_override: Option<PathBuf>,
    source_crate: SourceCrate,
}

impl<'a> GenerateRequest<'a> {
    pub fn new(
        config: &'a Config,
        output_override: Option<PathBuf>,
        source_crate: SourceCrate,
    ) -> Self {
        Self {
            config,
            output_override,
            source_crate,
        }
    }

    pub fn for_current_crate(config: &'a Config, output_override: Option<PathBuf>) -> Self {
        Self::new(config, output_override, SourceCrate::current(config))
    }

    pub fn config(&self) -> &'a Config {
        self.config
    }

    pub fn output_override(&self) -> Option<&Path> {
        self.output_override.as_deref()
    }

    pub fn lowered_crate(&self, pointer_width: ScanPointerWidth) -> Result<LoweredCrate> {
        let mut scanned_module = scan_crate_with_pointer_width(
            self.source_crate.source_directory(),
            self.source_crate.crate_name(),
            pointer_width.resolved_bits(),
        )
        .map_err(|error| CliError::CommandFailed {
            command: format!("scan_crate: {error}"),
            status: None,
        })?;

        let ffi_contract = ir::build_contract(&mut scanned_module);
        let abi_contract = ir::Lowerer::new(&ffi_contract).to_abi_contract();

        Ok(LoweredCrate {
            ffi_contract,
            abi_contract,
        })
    }

    pub fn ensure_output_directory(&self, output_directory: &Path) -> Result<()> {
        std::fs::create_dir_all(output_directory).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: output_directory.to_path_buf(),
                source,
            }
        })
    }

    pub fn write_output(&self, output_path: &Path, source_code: impl AsRef<[u8]>) -> Result<()> {
        std::fs::write(output_path, source_code).map_err(|source| CliError::WriteFailed {
            path: output_path.to_path_buf(),
            source,
        })
    }
}

pub trait LanguageGenerator {
    const TARGET: Target;

    fn generate(request: &GenerateRequest<'_>) -> Result<()>;
}

pub fn run_generator<Generator: LanguageGenerator>(
    request: &GenerateRequest<'_>,
    experimental_flag: bool,
) -> Result<()> {
    if Experimental::is_target_experimental(Generator::TARGET) {
        let enabled_in_config = request
            .config()
            .experimental
            .contains(&Generator::TARGET.name().to_string());

        if !experimental_flag && !enabled_in_config {
            return Err(CliError::CommandFailed {
                command: format!(
                    "{} is experimental, use --experimental flag or add \"{}\" to [experimental]",
                    Generator::TARGET.name(),
                    Generator::TARGET.name()
                ),
                status: None,
            });
        }
    }

    Generator::generate(request)
}
