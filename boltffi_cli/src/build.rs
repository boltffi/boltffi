mod expansion;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use crate::cli::Result;
use crate::config::Config;
use crate::target::{Platform, RustTarget};
use crate::toolchain::{AndroidToolchain, AndroidToolchainError};

pub use self::expansion::BindingExpansion;

pub type OutputCallback = Box<dyn Fn(&str) + Send>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoBuildProfile {
    Debug,
    Release,
    Named(String),
}

impl CargoBuildProfile {
    pub fn cargo_profile_name(&self) -> &str {
        match self {
            Self::Debug => "dev",
            Self::Release => "release",
            Self::Named(profile_name) => profile_name,
        }
    }

    pub fn resolve(default_release: bool, cargo_args: &[String]) -> Self {
        let default_profile = if default_release {
            Self::Release
        } else {
            Self::Debug
        };

        let mut skip_next_argument = false;

        cargo_args.iter().enumerate().fold(
            default_profile,
            |resolved_profile, (index, argument)| {
                if skip_next_argument {
                    skip_next_argument = false;
                    return resolved_profile;
                }

                match argument.as_str() {
                    "--release" => Self::Release,
                    "--profile" => {
                        skip_next_argument = true;
                        cargo_args
                            .get(index + 1)
                            .map(|profile_name| Self::from_profile_name(profile_name))
                            .unwrap_or(resolved_profile)
                    }
                    _ => argument
                        .strip_prefix("--profile=")
                        .filter(|profile_name| !profile_name.is_empty())
                        .map(Self::from_profile_name)
                        .unwrap_or(resolved_profile),
                }
            },
        )
    }

    pub fn output_directory_name(&self) -> &str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
            Self::Named(profile_name) => profile_name,
        }
    }

    pub fn is_release_like(&self) -> bool {
        !matches!(self, Self::Debug)
    }

    fn from_profile_name(profile_name: &str) -> Self {
        match profile_name {
            "debug" | "dev" => Self::Debug,
            "release" => Self::Release,
            _ => Self::Named(profile_name.to_string()),
        }
    }
}

pub fn resolve_build_profile(default_release: bool, cargo_args: &[String]) -> CargoBuildProfile {
    CargoBuildProfile::resolve(default_release, cargo_args)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CargoBuildCommandArgs {
    toolchain_selector: Option<String>,
    command_args: Vec<String>,
}

impl CargoBuildCommandArgs {
    fn from_passthrough_args(cargo_args: &[String]) -> Self {
        let toolchain_selector_index = cargo_args
            .iter()
            .position(|argument| is_toolchain_selector(argument));

        let (toolchain_selector, command_args) = toolchain_selector_index
            .map(|index| {
                let toolchain_selector = cargo_args.get(index).cloned();
                let command_args = cargo_args
                    .iter()
                    .take(index)
                    .chain(cargo_args.iter().skip(index + 1))
                    .cloned()
                    .collect();
                (toolchain_selector, command_args)
            })
            .unwrap_or_else(|| (None, cargo_args.to_vec()));

        Self {
            toolchain_selector,
            command_args,
        }
    }
}

fn is_toolchain_selector(argument: &str) -> bool {
    argument.starts_with('+') && argument.len() > 1
}

#[derive(Default)]
pub struct BuildOptions {
    pub release: bool,
    pub selection: BuildSelection,
    pub on_output: Option<OutputCallback>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildSelection {
    Default {
        cargo_args: Vec<String>,
    },
    Package {
        package: String,
        cargo_args: Vec<String>,
    },
    Expanded(Box<BindingExpansion>),
}

impl Default for BuildSelection {
    fn default() -> Self {
        Self::Default {
            cargo_args: Vec::new(),
        }
    }
}

pub struct Builder<'a> {
    config: &'a Config,
    options: BuildOptions,
}

pub struct BuildResult {
    pub triple: String,
    pub success: bool,
}

impl<'a> Builder<'a> {
    pub fn new(config: &'a Config, options: BuildOptions) -> Self {
        Self { config, options }
    }

    pub fn build_targets(&self, targets: &[RustTarget]) -> Result<Vec<BuildResult>> {
        let android_toolchain = targets
            .iter()
            .any(|target| target.platform() == Platform::Android)
            .then(|| {
                AndroidToolchain::discover(
                    self.config.android_min_sdk(),
                    self.config.android_ndk_version(),
                )
            })
            .transpose()?;

        targets
            .iter()
            .map(|target| self.build_single_target(target, android_toolchain.as_ref()))
            .collect()
    }

    pub fn build_android(&self, targets: &[RustTarget]) -> Result<Vec<BuildResult>> {
        self.build_targets(targets)
    }

    pub fn build_wasm_with_triple(&self, triple: &str) -> Result<Vec<BuildResult>> {
        let command_args = self.cargo_build_command_args();
        let mut command = Command::new("cargo");
        self.apply_cargo_build_prefix(&mut command, &command_args);
        command.arg("--target").arg(triple);

        self.apply_common_build_args(&mut command);
        command.args(&command_args.command_args);
        self.apply_expansion(&mut command)?;

        let success = run_command_streaming(&mut command, self.options.on_output.as_ref());

        Ok(vec![BuildResult {
            triple: triple.to_string(),
            success,
        }])
    }

    fn build_single_target(
        &self,
        target: &RustTarget,
        android_toolchain: Option<&AndroidToolchain>,
    ) -> Result<BuildResult> {
        let command_args = self.cargo_build_command_args();
        let mut cmd = Command::new("cargo");
        self.apply_cargo_build_prefix(&mut cmd, &command_args);
        cmd.arg("--target").arg(target.triple());

        self.apply_common_build_args(&mut cmd);
        cmd.args(&command_args.command_args);
        self.apply_expansion(&mut cmd)?;

        if target.platform() == Platform::Android {
            android_toolchain
                .ok_or(AndroidToolchainError::NdkNotFound.into())
                .and_then(|toolchain| toolchain.configure_cargo_for_target(&mut cmd, target))?;
        }

        let success = run_command_streaming(&mut cmd, self.options.on_output.as_ref());

        Ok(BuildResult {
            triple: target.triple().to_string(),
            success,
        })
    }

    fn package_spec(&self) -> &str {
        match &self.options.selection {
            BuildSelection::Default { .. } => self.config.library_name(),
            BuildSelection::Package { package, .. } => package,
            BuildSelection::Expanded(expansion) => expansion.package_id(),
        }
    }

    fn apply_common_build_args(&self, command: &mut Command) {
        if self.options.release {
            command.arg("--release");
        }

        if let BuildSelection::Expanded(expansion) = &self.options.selection {
            command
                .arg("--manifest-path")
                .arg(expansion.cargo_manifest_path());
        }

        command.arg("-p").arg(self.package_spec());
    }

    fn apply_expansion(&self, command: &mut Command) -> Result<()> {
        match &self.options.selection {
            BuildSelection::Expanded(expansion) => {
                command.arg("--lib");
                expansion.configure_rustc(command)
            }
            BuildSelection::Default { .. } | BuildSelection::Package { .. } => Ok(()),
        }
    }

    fn cargo_build_command_args(&self) -> CargoBuildCommandArgs {
        match &self.options.selection {
            BuildSelection::Default { cargo_args } | BuildSelection::Package { cargo_args, .. } => {
                CargoBuildCommandArgs::from_passthrough_args(cargo_args)
            }
            BuildSelection::Expanded(expansion) => CargoBuildCommandArgs {
                toolchain_selector: expansion.toolchain_selector().map(str::to_owned),
                command_args: expansion.cargo_args().as_slice().to_vec(),
            },
        }
    }

    fn apply_cargo_build_prefix(
        &self,
        command: &mut Command,
        command_args: &CargoBuildCommandArgs,
    ) {
        if let Some(toolchain_selector) = command_args.toolchain_selector.as_deref() {
            command.arg(toolchain_selector);
        }

        command.arg(
            if matches!(&self.options.selection, BuildSelection::Expanded(_)) {
                "rustc"
            } else {
                "build"
            },
        );
    }
}

pub(crate) fn run_command_streaming(cmd: &mut Command, on_output: Option<&OutputCallback>) -> bool {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (tx, rx) = mpsc::channel();
    let stdout_tx = tx.clone();
    let stderr_tx = tx.clone();

    let stdout_handle = stdout.map(|out| {
        thread::spawn(move || {
            for line in BufReader::new(out)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = stdout_tx.send(line);
            }
        })
    });

    let stderr_handle = stderr.map(|err| {
        thread::spawn(move || {
            for line in BufReader::new(err)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = stderr_tx.send(line);
            }
        })
    });

    drop(tx);

    for line in rx {
        if let Some(cb) = on_output {
            cb(&line);
        }
    }

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    child.wait().map(|s| s.success()).unwrap_or(false)
}
pub fn count_successful(results: &[BuildResult]) -> usize {
    results.iter().filter(|r| r.success).count()
}

pub fn all_successful(results: &[BuildResult]) -> bool {
    results.iter().all(|r| r.success)
}

pub fn failed_targets(results: &[BuildResult]) -> Vec<String> {
    results
        .iter()
        .filter(|r| !r.success)
        .map(|r| r.triple.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        BindingExpansion, BuildOptions, BuildSelection, Builder, CargoBuildCommandArgs,
        CargoBuildProfile, resolve_build_profile, run_command_streaming,
    };
    use crate::config::Config;
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn resolves_release_profile_from_passthrough_args() {
        let cargo_args = vec!["--locked".to_string(), "--release".to_string()];

        assert_eq!(
            resolve_build_profile(false, &cargo_args),
            CargoBuildProfile::Release
        );
    }

    #[test]
    fn resolves_named_profile_from_passthrough_args() {
        let cargo_args = vec!["--profile".to_string(), "mobile-release".to_string()];

        assert_eq!(
            resolve_build_profile(false, &cargo_args),
            CargoBuildProfile::Named("mobile-release".to_string())
        );
    }

    #[test]
    fn resolves_debug_profile_from_dev_profile_name() {
        let cargo_args = vec!["--profile=dev".to_string()];

        assert_eq!(
            resolve_build_profile(true, &cargo_args),
            CargoBuildProfile::Debug
        );
    }

    #[test]
    fn extracts_toolchain_selector_from_passthrough_args() {
        let cargo_args = vec![
            "--locked".to_string(),
            "+nightly".to_string(),
            "--features".to_string(),
            "mobile".to_string(),
        ];

        let command_args = CargoBuildCommandArgs::from_passthrough_args(&cargo_args);

        assert_eq!(
            command_args.toolchain_selector,
            Some("+nightly".to_string())
        );
        assert_eq!(
            command_args.command_args,
            vec![
                "--locked".to_string(),
                "--features".to_string(),
                "mobile".to_string()
            ]
        );
    }

    #[test]
    fn expanded_build_uses_only_the_selected_library_invocation() {
        let config: Config = toml::from_str(
            r#"
[package]
name = "demo"
"#,
        )
        .unwrap();
        let expansion = BindingExpansion::fixture(
            "/external/workspace/Cargo.toml",
            "/external/workspace/demo/Cargo.toml",
            ["--features".to_string(), "ffi".to_string()],
        );
        let builder = Builder::new(
            &config,
            BuildOptions {
                release: false,
                selection: BuildSelection::Expanded(Box::new(expansion)),
                on_output: None,
            },
        );
        let command_args = builder.cargo_build_command_args();
        let mut command = Command::new("cargo");
        builder.apply_cargo_build_prefix(&mut command, &command_args);
        builder.apply_common_build_args(&mut command);
        command.args(&command_args.command_args);
        builder.apply_expansion(&mut command).unwrap();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(arguments.first().map(String::as_str), Some("+nightly"));
        assert_eq!(arguments.get(1).map(String::as_str), Some("rustc"));
        assert!(arguments.windows(2).any(|arguments| {
            arguments == ["--manifest-path", "/external/workspace/Cargo.toml"]
        }));
        assert!(
            arguments
                .windows(2)
                .any(|arguments| arguments == ["--features", "ffi"])
        );
        assert_eq!(
            &arguments[arguments.len() - 4..],
            ["--lib", "--", "--cfg", "boltffi_binding_expansion"]
        );
    }

    #[test]
    fn streaming_command_returns_after_child_exits() {
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let mut command = if cfg!(windows) {
                let mut cmd = Command::new("cmd");
                cmd.args(["/C", "echo", "ok"]);
                cmd
            } else {
                let mut cmd = Command::new("sh");
                cmd.args(["-c", "printf ok"]);
                cmd
            };

            let result = run_command_streaming(&mut command, None);
            let _ = tx.send(result);
        });

        let result = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("run_command_streaming should finish once the child exits");
        assert!(result);
    }
}
