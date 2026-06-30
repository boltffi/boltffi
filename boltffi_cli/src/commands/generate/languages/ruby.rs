use crate::{
    cli::{CliError, Result},
    commands::generate::generator::{GenerateRequest, LanguageGenerator},
    config::Target,
};

pub struct RubyGenerator;

impl RubyGenerator {
    #[cfg(test)]
    pub(crate) fn generate_from_source_directory(
        _config: &crate::config::Config,
        _output_override: Option<std::path::PathBuf>,
        _source_directory: &std::path::Path,
        _crate_name: &str,
    ) -> Result<()> {
        Err(CliError::CommandFailed {
            command: "legacy Ruby generator is detached; use IR generation".to_string(),
            status: None,
        })
    }
}

impl LanguageGenerator for RubyGenerator {
    const TARGET: Target = Target::Ruby;

    fn generate(_request: &GenerateRequest<'_>) -> Result<()> {
        Err(CliError::CommandFailed {
            command: "Ruby generation uses the IR backend path; run `boltffi generate ruby --experimental`".to_string(),
            status: None,
        })
    }
}
