use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use super::{ActiveCfg, CfgPredicate};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RustcCfgArgs {
    active: ActiveCfg,
}

#[derive(Debug)]
pub enum RustcCfgArgsError {
    NonUnicodeArgument { index: usize, argument: OsString },
    ReadResponseFile { path: PathBuf, source: io::Error },
    UnsupportedShellResponseFile { path: PathBuf },
}

#[derive(Default)]
struct ResponseFileExpander {
    shell_argfiles: bool,
    next_is_unstable_option: bool,
    arguments: Vec<String>,
}

impl RustcCfgArgs {
    pub fn from_args(
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<Option<Self>, RustcCfgArgsError> {
        let arguments = arguments
            .into_iter()
            .map(Into::into)
            .enumerate()
            .map(|(index, argument)| {
                argument
                    .into_string()
                    .map_err(|argument| RustcCfgArgsError::NonUnicodeArgument { index, argument })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let Some((_, arguments)) = arguments.split_first() else {
            return Ok(None);
        };
        let arguments = ResponseFileExpander::expand(arguments.iter().cloned())?;
        if !Self::has_crate_name(&arguments) {
            return Ok(None);
        }
        let active = arguments
            .windows(2)
            .filter(|pair| pair[0] == "--cfg")
            .filter_map(|pair| {
                pair[1]
                    .strip_prefix("--")
                    .is_none()
                    .then_some(pair[1].as_str())
            })
            .chain(
                arguments
                    .iter()
                    .filter_map(|argument| argument.strip_prefix("--cfg=")),
            )
            .fold(ActiveCfg::default(), ActiveCfg::with_rustc_spec);
        Ok(Some(Self { active }))
    }

    pub fn features(&self) -> impl Iterator<Item = &str> {
        self.active.features.iter().map(String::as_str)
    }

    pub fn resolve(&self, mut inherited: ActiveCfg) -> ActiveCfg {
        inherited.names.extend(self.active.names.iter().cloned());
        self.active.values.iter().for_each(|(name, values)| {
            inherited
                .values
                .entry(name.clone())
                .or_default()
                .extend(values.iter().cloned());
        });
        inherited.features.clone_from(&self.active.features);
        inherited.legacy_features.clear();
        inherited
    }

    fn has_crate_name(arguments: &[String]) -> bool {
        arguments.windows(2).any(|pair| {
            pair[0] == "--crate-name" && !pair[1].is_empty() && !pair[1].starts_with('-')
        }) || arguments.iter().any(|argument| {
            argument
                .strip_prefix("--crate-name=")
                .is_some_and(|crate_name| !crate_name.is_empty())
        })
    }
}

impl fmt::Display for RustcCfgArgsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUnicodeArgument { index, argument } => {
                write!(
                    formatter,
                    "rustc argument {index} is not valid Unicode: {argument:?}"
                )
            }
            Self::ReadResponseFile { path, source } => {
                write!(
                    formatter,
                    "failed to read rustc response file {}: {source}",
                    path.display()
                )
            }
            Self::UnsupportedShellResponseFile { path } => {
                write!(
                    formatter,
                    "shell-style rustc response file {} is unsupported",
                    path.display()
                )
            }
        }
    }
}

impl Error for RustcCfgArgsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadResponseFile { source, .. } => Some(source),
            Self::NonUnicodeArgument { .. } | Self::UnsupportedShellResponseFile { .. } => None,
        }
    }
}

impl ResponseFileExpander {
    fn expand(
        arguments: impl IntoIterator<Item = String>,
    ) -> Result<Vec<String>, RustcCfgArgsError> {
        let mut expander = Self::default();
        arguments
            .into_iter()
            .try_for_each(|argument| expander.expand_argument(argument))?;
        Ok(expander.arguments)
    }

    fn expand_argument(&mut self, argument: String) -> Result<(), RustcCfgArgsError> {
        let Some(response_file) = argument.strip_prefix('@') else {
            self.push(argument);
            return Ok(());
        };
        if self.shell_argfiles
            && let Some(path) = response_file.strip_prefix("shell:")
        {
            return Err(RustcCfgArgsError::UnsupportedShellResponseFile {
                path: PathBuf::from(path),
            });
        }
        let path = PathBuf::from(response_file);
        let contents =
            fs::read_to_string(&path).map_err(|source| RustcCfgArgsError::ReadResponseFile {
                path: path.clone(),
                source,
            })?;
        contents
            .lines()
            .map(str::to_owned)
            .for_each(|argument| self.push(argument));
        Ok(())
    }

    fn push(&mut self, argument: String) {
        if self.next_is_unstable_option {
            self.shell_argfiles = argument == "shell-argfiles";
            self.next_is_unstable_option = false;
        } else if let Some(option) = argument.strip_prefix("-Z") {
            if option.is_empty() {
                self.next_is_unstable_option = true;
            } else if option == "shell-argfiles" {
                self.shell_argfiles = true;
            }
        }
        self.arguments.push(argument);
    }
}

impl ActiveCfg {
    fn with_rustc_spec(mut self, specification: &str) -> Self {
        match syn::parse_str::<CfgPredicate>(specification) {
            Ok(CfgPredicate::Name(name)) => {
                self.names.insert(name);
            }
            Ok(CfgPredicate::Value { name, value }) => {
                if name == "feature" {
                    self.features.insert(value);
                } else {
                    self.values.entry(name).or_default().insert(value);
                }
            }
            _ => {}
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{RustcCfgArgs, RustcCfgArgsError};
    use crate::ActiveCfg;

    struct ResponseFile {
        path: PathBuf,
    }

    impl ResponseFile {
        fn new(contents: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("boltffi-rustc-response-{}-{stamp}", process::id()));
            fs::write(&path, contents).expect("write rustc response file");
            Self { path }
        }

        fn argument(&self) -> String {
            format!("@{}", self.path.display())
        }
    }

    impl Drop for ResponseFile {
        fn drop(&mut self) {
            fs::remove_file(&self.path).ok();
        }
    }

    fn rustc(arguments: &[&str]) -> RustcCfgArgs {
        RustcCfgArgs::from_args(arguments.iter().copied())
            .expect("valid rustc arguments")
            .expect("rustc invocation")
    }

    fn matches(active: &ActiveCfg, attribute: &str) -> bool {
        let source = format!("{attribute} struct Demo;");
        active
            .configure(syn::parse_str(&source).expect("valid source"))
            .expect("cfg configuration")
            .syntax()
            .items
            .iter()
            .any(|item| matches!(item, syn::Item::Struct(_)))
    }

    #[test]
    fn extracts_split_and_inline_cfg_arguments() {
        let rustc = rustc(&[
            "rustc",
            "--crate-name",
            "demo",
            "--cfg",
            "feature=\"native-ffi\"",
            "--cfg=unix",
            "--cfg",
            "target_os=\"macos\"",
        ]);
        let active = rustc.resolve(ActiveCfg::default());

        assert_eq!(rustc.features().collect::<Vec<_>>(), ["native-ffi"]);
        assert!(matches(&active, "#[cfg(feature = \"native-ffi\")]"));
        assert!(!matches(&active, "#[cfg(feature = \"native_ffi\")]"));
        assert!(matches(&active, "#[cfg(unix)]"));
        assert!(matches(&active, "#[cfg(target_os = \"macos\")]"));
    }

    #[test]
    fn inline_crate_name_preserves_exact_features() {
        let rustc = rustc(&["rustc", "--crate-name=demo", "--cfg=feature=\"native-ffi\""]);
        let active = rustc.resolve(ActiveCfg::default());

        assert!(matches(&active, "#[cfg(feature = \"native-ffi\")]"));
        assert!(!matches(&active, "#[cfg(feature = \"native_ffi\")]"));
    }

    #[test]
    fn inline_crate_name_with_empty_features_replaces_fallbacks() {
        let rustc = rustc(&["rustc", "--crate-name=demo"]);
        let inherited = ActiveCfg::default().with_features(["stale", "legacy"]);
        let active = rustc.resolve(inherited);

        assert!(!matches(&active, "#[cfg(feature = \"stale\")]"));
        assert!(!matches(&active, "#[cfg(feature = \"legacy\")]"));
    }

    #[test]
    fn expands_line_based_response_files() {
        let response =
            ResponseFile::new("--crate-name\ndemo\n--cfg\nfeature=\"native-ffi\"\n--cfg=unix\n");
        let argument = response.argument();
        let rustc = rustc(&["rustc", &argument]);
        let active = rustc.resolve(ActiveCfg::default());

        assert!(matches(&active, "#[cfg(feature = \"native-ffi\")]"));
        assert!(matches(&active, "#[cfg(unix)]"));
    }

    #[test]
    fn does_not_expand_the_executable_argument() {
        let missing = std::env::temp_dir().join("boltffi-missing-rustc-executable");
        fs::remove_file(&missing).ok();
        let executable = format!("@{}", missing.display());

        let rustc = RustcCfgArgs::from_args([executable, "--crate-name=demo".to_owned()])
            .expect("executable is not a response file");

        assert!(rustc.is_some());
    }

    #[test]
    fn reports_response_file_read_failures() {
        let missing = std::env::temp_dir().join("boltffi-missing-rustc-response");
        fs::remove_file(&missing).ok();
        let argument = format!("@{}", missing.display());
        let error = RustcCfgArgs::from_args(["rustc".to_owned(), argument])
            .expect_err("missing response file must reject");

        assert!(matches!(
            error,
            RustcCfgArgsError::ReadResponseFile { path, .. } if path == missing
        ));
    }

    #[test]
    fn rejects_active_shell_response_files() {
        let path = Path::new("arguments.txt");
        let error = RustcCfgArgs::from_args(["rustc", "-Zshell-argfiles", "@shell:arguments.txt"])
            .expect_err("shell response file must reject");

        assert!(matches!(
            error,
            RustcCfgArgsError::UnsupportedShellResponseFile { path: error_path }
                if error_path == path
        ));
    }

    #[test]
    fn malformed_crate_name_arguments_are_not_compiler_configuration() {
        [
            vec!["rustc", "--crate-name"],
            vec!["rustc", "--crate-name", "--cfg", "feature=\"ffi\""],
            vec!["rustc", "--crate-name="],
        ]
        .into_iter()
        .try_for_each(|arguments| {
            RustcCfgArgs::from_args(arguments).map(|rustc| assert!(rustc.is_none()))
        })
        .expect("valid non-compiler arguments");
    }

    #[test]
    fn unrelated_process_arguments_are_not_compiler_configuration() {
        assert!(
            RustcCfgArgs::from_args(["boltffi", "generate", "swift"])
                .expect("valid process arguments")
                .is_none()
        );
    }
}
