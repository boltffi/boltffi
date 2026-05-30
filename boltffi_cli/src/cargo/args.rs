use std::path::{Path, PathBuf};

use crate::cli::{CliError, Result};

use super::config;

#[derive(Debug, Clone)]
pub(super) struct CargoArguments {
    raw_arguments: Vec<String>,
    toolchain_selector: Option<String>,
    command_arguments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CargoSelector {
    Package,
    ManifestPath,
    Target,
}

impl CargoSelector {
    fn matches_split_flag(self, argument: &str) -> bool {
        match self {
            Self::Package => matches!(argument, "--package" | "-p"),
            Self::ManifestPath => argument == "--manifest-path",
            Self::Target => argument == "--target",
        }
    }

    fn matches_inline_flag(self, argument: &str) -> bool {
        match self {
            Self::Package => {
                argument.starts_with("--package=")
                    || (argument.starts_with("-p") && argument.len() > 2)
            }
            Self::ManifestPath => argument.starts_with("--manifest-path="),
            Self::Target => argument.starts_with("--target="),
        }
    }
}

impl CargoArguments {
    pub(super) fn new(cargo_args: &[String]) -> Self {
        let toolchain_selector_index = cargo_args
            .iter()
            .position(|argument| argument.starts_with('+') && argument.len() > 1);

        let (toolchain_selector, command_arguments) = toolchain_selector_index
            .map(|index| {
                let toolchain_selector = cargo_args.get(index).cloned();
                let command_arguments = cargo_args
                    .iter()
                    .take(index)
                    .chain(cargo_args.iter().skip(index + 1))
                    .cloned()
                    .collect();
                (toolchain_selector, command_arguments)
            })
            .unwrap_or_else(|| (None, cargo_args.to_vec()));

        Self {
            raw_arguments: cargo_args.to_vec(),
            toolchain_selector,
            command_arguments,
        }
    }

    pub(super) fn metadata_arguments(&self) -> Vec<String> {
        let mut metadata_arguments = Vec::new();
        let mut index = 0;

        while index < self.raw_arguments.len() {
            let argument = &self.raw_arguments[index];
            let takes_value = matches!(
                argument.as_str(),
                "--target-dir" | "--config" | "-Z" | "--manifest-path"
            );
            let keep_current = takes_value
                || matches!(argument.as_str(), "--locked" | "--offline" | "--frozen")
                || argument.starts_with("--target-dir=")
                || argument.starts_with("--config=")
                || argument.starts_with("-Z")
                || argument.starts_with("--manifest-path=");

            if keep_current {
                metadata_arguments.push(argument.clone());
                if takes_value
                    && !argument.contains('=')
                    && let Some(value) = self.raw_arguments.get(index + 1)
                {
                    metadata_arguments.push(value.clone());
                    index += 1;
                }
            }

            index += 1;
        }

        metadata_arguments
    }

    pub(super) fn toolchain_selector(&self) -> Option<&str> {
        self.toolchain_selector.as_deref()
    }

    pub(super) fn manifest_path(&self, working_directory: &Path) -> Result<PathBuf> {
        if let Some(manifest_path) =
            self.command_arguments
                .iter()
                .enumerate()
                .find_map(|(index, argument)| {
                    argument
                        .strip_prefix("--manifest-path=")
                        .map(PathBuf::from)
                        .or_else(|| {
                            (argument == "--manifest-path")
                                .then(|| self.command_arguments.get(index + 1).map(PathBuf::from))
                                .flatten()
                        })
                })
        {
            return Self::canonical_manifest_path(working_directory, manifest_path);
        }

        Self::canonical_manifest_path(working_directory, working_directory.join("Cargo.toml"))
    }

    pub(super) fn package_selector(&self) -> Option<&str> {
        let mut package_selector = None;
        let mut index = 0;

        while index < self.command_arguments.len() {
            let argument = &self.command_arguments[index];

            if let Some(selector) = argument.strip_prefix("--package=") {
                package_selector = Some(selector);
            } else if let Some(selector) = argument.strip_prefix("-p") {
                if !selector.is_empty() {
                    package_selector = Some(selector);
                } else if let Some(value) = self.command_arguments.get(index + 1) {
                    package_selector = Some(value.as_str());
                    index += 1;
                }
            } else if argument == "--package"
                && let Some(value) = self.command_arguments.get(index + 1)
            {
                package_selector = Some(value.as_str());
                index += 1;
            }

            index += 1;
        }

        package_selector
    }

    pub(super) fn target_selector(&self) -> Option<&str> {
        let mut target_selector = None;
        let mut index = 0;

        while index < self.command_arguments.len() {
            let argument = &self.command_arguments[index];

            if let Some(selector) = argument.strip_prefix("--target=") {
                if !selector.is_empty() {
                    target_selector = Some(selector);
                }
            } else if argument == "--target"
                && let Some(value) = self.command_arguments.get(index + 1)
            {
                target_selector = Some(value.as_str());
                index += 1;
            }

            index += 1;
        }

        target_selector
    }

    pub(super) fn probe_command_arguments(&self) -> Vec<String> {
        let without_package_selector =
            CargoArguments::new(&self.command_arguments_without_package_selector());
        let without_manifest_path_selector = CargoArguments::new(
            &without_package_selector.command_arguments_without_manifest_path_selector(),
        );
        CargoArguments::new(
            &without_manifest_path_selector.command_arguments_without_target_selector(),
        )
        .command_arguments
        .clone()
    }

    pub(super) fn configured_build_target(&self, working_directory: &Path) -> Option<String> {
        config::configured_build_target(&self.raw_arguments, Some(working_directory))
    }

    pub(super) fn has_explicit_manifest_path(&self) -> bool {
        self.command_arguments.iter().any(|argument| {
            argument == "--manifest-path" || argument.starts_with("--manifest-path=")
        })
    }

    #[cfg(test)]
    pub(super) fn command_arguments(&self) -> &[String] {
        &self.command_arguments
    }

    pub(super) fn command_arguments_without_package_selector(&self) -> Vec<String> {
        self.command_arguments_without_selector(CargoSelector::Package)
    }

    pub(super) fn command_arguments_without_manifest_path_selector(&self) -> Vec<String> {
        self.command_arguments_without_selector(CargoSelector::ManifestPath)
    }

    pub(super) fn command_arguments_without_target_selector(&self) -> Vec<String> {
        self.command_arguments_without_selector(CargoSelector::Target)
    }

    fn command_arguments_without_selector(&self, selector: CargoSelector) -> Vec<String> {
        let mut filtered_arguments = Vec::new();
        let mut index = 0;

        while index < self.raw_arguments.len() {
            let argument = &self.raw_arguments[index];

            if selector.matches_split_flag(argument) {
                index += 1;
                if self.raw_arguments.get(index).is_some() {
                    index += 1;
                }
                continue;
            }

            if selector.matches_inline_flag(argument) {
                index += 1;
                continue;
            }

            filtered_arguments.push(argument.clone());
            index += 1;
        }

        filtered_arguments
    }

    fn canonical_manifest_path(
        working_directory: &Path,
        manifest_path: PathBuf,
    ) -> Result<PathBuf> {
        let manifest_path = if manifest_path.is_absolute() {
            manifest_path
        } else {
            working_directory.join(manifest_path)
        };

        canonicalize_manifest_path(&manifest_path).map_err(|source| CliError::CommandFailed {
            command: format!(
                "canonicalize manifest path {}: {source}",
                manifest_path.display()
            ),
            status: None,
        })
    }
}

#[cfg(windows)]
fn canonicalize_manifest_path(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

#[cfg(not(windows))]
fn canonicalize_manifest_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}
