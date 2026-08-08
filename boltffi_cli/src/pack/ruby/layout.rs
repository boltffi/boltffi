use super::plan::RubyPackagingPlan;
use super::platform::RubyPackTarget;
use crate::cli::{CliError, Result};
use crate::config::Config;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RubyStagingLayout {
    pub source_root: PathBuf,
    pub target_root: PathBuf,
}

impl RubyStagingLayout {
    pub fn new(config: &Config, plan: &RubyPackagingPlan) -> Result<Self> {
        let source_root = absolutize(config.ruby_output())?;
        let target_root = absolutize(config.ruby_pack_output())?;

        if source_root == plan.source_directory {
            return Err(CliError::CommandFailed {
                command: "targets.ruby.output must not be equal to the source workspace root"
                    .to_string(),
                status: None,
            });
        }
        if target_root == plan.source_directory {
            return Err(CliError::CommandFailed {
                command: "targets.ruby.pack.output must not be equal to the source workspace root"
                    .to_string(),
                status: None,
            });
        }

        Ok(Self {
            source_root,
            target_root,
        })
    }

    pub fn ext_dir(&self, plan: &RubyPackagingPlan) -> PathBuf {
        self.source_root.join("ext").join(&plan.crate_name)
    }

    pub fn target_staging_dir(
        &self,
        _plan: &RubyPackagingPlan,
        target: &RubyPackTarget,
    ) -> PathBuf {
        self.target_root
            .join(".pack")
            .join(&target.rubygems_platform)
    }
}

fn absolutize(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|current_dir| current_dir.join(path))
            .map_err(|source| CliError::CommandFailed {
                command: format!("current_dir: {source}"),
                status: None,
            })
    }
}
