use std::process::Command;

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
        let mut command = self.cargo_command()?;

        if !run_command_streaming(&mut command, on_output.as_ref()) {
            return Err(PackError::BuildFailed {
                targets: vec![self.plan.host_platform.canonical_name().to_string()],
            }
            .into());
        }

        self.existing()
    }

    /// The cdylib build, in the same shape `Builder` gives every other `pack`
    /// backend for `BuildSelection::Expanded`.
    fn cargo_command(&self) -> Result<Command> {
        let mut command = Command::new("cargo");

        if let Some(toolchain_selector) = self.plan.cargo_context.toolchain_selector.as_deref() {
            command.arg(toolchain_selector);
        }

        // `cargo rustc` builds exactly the selected library target, like every
        // other pack backend. See `build.rs`' `Builder::apply_expansion`.
        command.arg("rustc");
        command
            .arg("--manifest-path")
            .arg(&self.plan.cargo_context.cargo_manifest_path);

        if let Some(package_selector) = self.plan.cargo_context.package_selector.as_deref() {
            command.arg("-p").arg(package_selector);
        }

        if self.plan.cargo_context.release {
            command.arg("--release");
        }

        // The expansion's parsed args, not the raw probe arguments: `LibraryCargoArgs`
        // drops `--lib` so appending it below cannot duplicate it, and rejects a `--`
        // tail, a package set and a non-library target up front — all three of which
        // would otherwise reach cargo alongside the `--cfg` tail and fail obscurely.
        command.args(self.plan.expansion.cargo_args().as_slice());
        command.arg("--lib");
        self.plan.expansion.configure_rustc(&mut command)?;

        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::super::plan::PythonPackagingPlan;
    use super::PythonSharedLibraryBuilder;

    /// `pack python` builds the library with the same Cargo arguments every other
    /// backend uses.
    #[test]
    fn builds_the_cdylib_as_an_expansion() {
        let plan = PythonPackagingPlan::fixture(true, "demo_ffi");
        let builder = PythonSharedLibraryBuilder::new(&plan);

        let command = builder.cargo_command().expect("cargo command");

        let args = command
            .get_args()
            .map(OsStr::to_string_lossy)
            .map(|arg| arg.to_string())
            .collect::<Vec<_>>();
        assert_eq!(args.first().map(String::as_str), Some("rustc"));
        assert_eq!(
            args.iter().rev().take(2).rev().cloned().collect::<Vec<_>>(),
            vec!["--lib", "--"],
        );
    }

    /// Passing the *parsed* expansion args rather than the raw probe arguments is
    /// what keeps a caller's own `--lib` from being appended twice, and a `--` tail
    /// from colliding with the `--cfg` one. `LibraryCargoArgs` drops the former and
    /// rejects the latter.
    #[test]
    fn does_not_duplicate_a_caller_supplied_lib_flag() {
        let plan = PythonPackagingPlan::fixture_with_cargo_args(["--lib", "--features", "ffi"]);
        let builder = PythonSharedLibraryBuilder::new(&plan);

        let command = builder.cargo_command().expect("cargo command");

        let args = command
            .get_args()
            .map(OsStr::to_string_lossy)
            .map(|arg| arg.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            args.iter().filter(|arg| *arg == "--lib").count(),
            1,
            "{args:?}"
        );
        assert_eq!(
            args.iter().filter(|arg| *arg == "--").count(),
            1,
            "{args:?}"
        );
    }
}
