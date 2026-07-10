use std::process::Command;

use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BINDING_METADATA_FEATURES_ENV, BindingMetadataSurface,
};

use crate::build::{OutputCallback, run_command_streaming};
use crate::cli::{CliError, Result};
use crate::pack::{PackError, print_cargo_line};
use crate::reporter::Step;

use super::plan::PythonPackagingPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltPythonSharedLibrary {
    pub source_path: std::path::PathBuf,
}

pub struct PythonSharedLibraryBuilder<'a> {
    plan: &'a PythonPackagingPlan,
}

impl<'a> PythonSharedLibraryBuilder<'a> {
    pub fn new(plan: &'a PythonPackagingPlan) -> Self {
        Self { plan }
    }

    pub fn existing(&self) -> Result<BuiltPythonSharedLibrary> {
        let source_path = self.plan.built_shared_library_path();
        source_path
            .exists()
            .then_some(BuiltPythonSharedLibrary { source_path })
            .ok_or(CliError::FileNotFound(
                self.plan.built_shared_library_path(),
            ))
    }

    pub fn build(&self, step: &Step) -> Result<BuiltPythonSharedLibrary> {
        let verbose = step.is_verbose();
        let on_output: Option<OutputCallback> =
            verbose.then(|| Box::new(|line: &str| print_cargo_line(line)) as OutputCallback);
        let mut command = Command::new("cargo");

        if let Some(toolchain_selector) = self.plan.cargo_context.toolchain_selector.as_deref() {
            command.arg(toolchain_selector);
        }

        command.arg("build");
        command
            .arg("--manifest-path")
            .arg(&self.plan.cargo_context.cargo_manifest_path);

        if let Some(package_selector) = self.plan.cargo_context.package_selector.as_deref() {
            command.arg("-p").arg(package_selector);
        }

        if self.plan.cargo_context.release {
            command.arg("--release");
        }

        command.args(&self.plan.cargo_context.cargo_command_args);
        command.env(BINDING_EXPANSION_BUILD_ENV, "1");
        command.env(
            BINDING_EXPANSION_ROOT_ENV,
            self.plan
                .cargo_context
                .manifest_path
                .parent()
                .ok_or_else(|| CliError::CommandFailed {
                    command: format!(
                        "manifest path '{}' has no parent directory",
                        self.plan.cargo_context.manifest_path.display()
                    ),
                    status: None,
                })?,
        );
        command.env(
            BINDING_EXPANSION_SOURCE_ENV,
            &self.plan.cargo_context.library_source_path,
        );
        command.env(
            BINDING_EXPANSION_SURFACE_ENV,
            BindingMetadataSurface::Native.as_str(),
        );
        command.env(
            BINDING_METADATA_FEATURES_ENV,
            self.plan.cargo_context.features.env_value(),
        );

        if !run_command_streaming(&mut command, on_output.as_ref()) {
            return Err(PackError::BuildFailed {
                targets: vec![self.plan.host_platform.canonical_name().to_string()],
            }
            .into());
        }

        self.existing()
    }
}
