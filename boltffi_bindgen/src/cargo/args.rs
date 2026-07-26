use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryCargoArgs {
    arguments: Vec<String>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LibraryCargoArgsError {
    #[error(
        "Cargo argument `{argument}` selects multiple packages; a library build requires exactly one package"
    )]
    PackageSet { argument: String },
    #[error(
        "Cargo argument `{argument}` selects a non-library target; a library build requires exactly one library target"
    )]
    TargetSet { argument: String },
    #[error(
        "Cargo rustc arguments after `--` are unsupported; BoltFFI owns the rustc arguments for a library build"
    )]
    RustcTail,
}

impl LibraryCargoArgs {
    pub fn parse(
        arguments: impl IntoIterator<Item = String>,
    ) -> Result<Self, LibraryCargoArgsError> {
        arguments
            .into_iter()
            .try_fold(
                (Vec::new(), false),
                |(mut compatible, package_selected), argument| {
                    if argument == "--" {
                        return Err(LibraryCargoArgsError::RustcTail);
                    }
                    if Self::selects_package_set(&argument) {
                        return Err(LibraryCargoArgsError::PackageSet { argument });
                    }
                    if Self::selects_non_library_target(&argument) {
                        return Err(LibraryCargoArgsError::TargetSet { argument });
                    }
                    let selects_package = Self::selects_package(&argument);
                    if package_selected && selects_package {
                        return Err(LibraryCargoArgsError::PackageSet { argument });
                    }
                    if argument != "--lib" {
                        compatible.push(argument);
                    }
                    Ok((compatible, package_selected || selects_package))
                },
            )
            .map(|(arguments, _)| Self { arguments })
    }

    pub fn as_slice(&self) -> &[String] {
        &self.arguments
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.arguments.iter()
    }

    pub fn into_vec(self) -> Vec<String> {
        self.arguments
    }

    fn selects_package_set(argument: &str) -> bool {
        matches!(argument, "--workspace" | "--all" | "--exclude")
            || argument.starts_with("--exclude=")
    }

    fn selects_package(argument: &str) -> bool {
        matches!(argument, "--package" | "-p")
            || argument.starts_with("--package=")
            || (argument.starts_with("-p") && argument.len() > 2)
    }

    fn selects_non_library_target(argument: &str) -> bool {
        matches!(
            argument,
            "--bins"
                | "--bin"
                | "--examples"
                | "--example"
                | "--tests"
                | "--test"
                | "--benches"
                | "--bench"
                | "--all-targets"
        ) || ["--bin=", "--example=", "--test=", "--bench="]
            .iter()
            .any(|prefix| argument.starts_with(prefix))
    }
}

impl<'arguments> IntoIterator for &'arguments LibraryCargoArgs {
    type Item = &'arguments String;
    type IntoIter = std::slice::Iter<'arguments, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.arguments.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{LibraryCargoArgs, LibraryCargoArgsError};

    fn strings(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| argument.to_string())
            .collect()
    }

    #[test]
    fn preserves_single_library_compatible_arguments() {
        let arguments = strings(&[
            "--package",
            "demo",
            "--features",
            "ffi,serde",
            "--features=async",
            "-Fextra",
            "--profile",
            "dist",
            "--profile=shipping",
            "--jobs",
            "4",
            "-j8",
            "--config",
            "build.rustflags=[]",
            "--config=net.offline=true",
            "--offline",
            "--locked",
            "--frozen",
            "--target-dir",
            "target/custom",
            "--target-dir=target/other",
            "--release",
            "--all-features",
            "--no-default-features",
        ]);

        let parsed = LibraryCargoArgs::parse(arguments.clone()).unwrap();

        assert_eq!(parsed.as_slice(), arguments);
    }

    #[test]
    fn removes_the_owned_library_selector() {
        let parsed = LibraryCargoArgs::parse(strings(&["--lib", "--release"])).unwrap();

        assert_eq!(parsed.as_slice(), strings(&["--release"]));
    }

    #[test]
    fn rejects_split_and_inline_package_set_selectors() {
        [
            strings(&["--workspace"]),
            strings(&["--all"]),
            strings(&["--exclude", "demo"]),
            strings(&["--exclude=demo"]),
            strings(&["-p", "first", "-psecond"]),
            strings(&["--package=first", "--package", "second"]),
        ]
        .into_iter()
        .for_each(|arguments| {
            assert!(matches!(
                LibraryCargoArgs::parse(arguments),
                Err(LibraryCargoArgsError::PackageSet { .. })
            ));
        });
    }

    #[test]
    fn rejects_split_and_inline_non_library_target_selectors() {
        [
            strings(&["--bins"]),
            strings(&["--bin", "demo"]),
            strings(&["--bin=demo"]),
            strings(&["--examples"]),
            strings(&["--example", "demo"]),
            strings(&["--example=demo"]),
            strings(&["--tests"]),
            strings(&["--test", "demo"]),
            strings(&["--test=demo"]),
            strings(&["--benches"]),
            strings(&["--bench", "demo"]),
            strings(&["--bench=demo"]),
            strings(&["--all-targets"]),
        ]
        .into_iter()
        .for_each(|arguments| {
            assert!(matches!(
                LibraryCargoArgs::parse(arguments),
                Err(LibraryCargoArgsError::TargetSet { .. })
            ));
        });
    }

    #[test]
    fn rejects_rustc_argument_tails() {
        assert_eq!(
            LibraryCargoArgs::parse(strings(&["--release", "--", "-Copt-level=3"])),
            Err(LibraryCargoArgsError::RustcTail)
        );
    }
}
