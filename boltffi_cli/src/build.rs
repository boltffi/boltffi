use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use crate::cli::Result;
use crate::config::Config;
use crate::target::{Platform, RustTarget};
use crate::toolchain::{AndroidToolchain, AndroidToolchainError};

pub type OutputCallback = Box<dyn Fn(&str) + Send>;

/// Describes which program and subcommand to use for `cargo build`.
///
/// | Config / CLI value | Resolved invocation |
/// |--------------------|---------------------|
/// | (none)             | `cargo build`       |
/// | `"zigbuild"`       | `cargo zigbuild`    |
/// | `"cross"`          | `cross build`       |
/// | `"cargo zigbuild"` | `cargo zigbuild`    |
/// | `"cross build"`    | `cross build`       |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoBuildCommand {
    /// Standard `cargo build`.
    Cargo,
    /// `cargo zigbuild` — uses Zig as a linker for glibc-versioned Linux targets.
    Zigbuild,
    /// `cross build` — Docker-based cross-compilation via pre-configured containers.
    Cross,
    /// A custom `"program subcommand"` pair not directly known to BoltFFI.
    Custom { program: String, subcommand: String },
}

impl CargoBuildCommand {
    /// Parse a build command override string.
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        // Explicit "program subcommand" form.
        if let Some((program, subcommand)) = s.split_once(' ') {
            let program = program.trim();
            let subcommand = subcommand.trim();
            if !program.is_empty() && !subcommand.is_empty() {
                return Self::from_parts(program, subcommand);
            }
        }
        // Single token.
        Self::from_token(s)
    }

    /// The executable name for this build command.
    pub fn program(&self) -> &str {
        match self {
            Self::Cargo | Self::Zigbuild => "cargo",
            Self::Cross => "cross",
            Self::Custom { program, .. } => program.as_str(),
        }
    }

    /// The subcommand passed after the program name.
    pub fn subcommand(&self) -> &str {
        match self {
            Self::Cargo | Self::Cross => "build",
            Self::Zigbuild => "zigbuild",
            Self::Custom { subcommand, .. } => subcommand.as_str(),
        }
    }

    /// Whether this build command uses `cargo` as its program and should therefore
    /// receive toolchain selectors like `+nightly`.
    pub fn is_cargo_program(&self) -> bool {
        matches!(self, Self::Cargo | Self::Zigbuild)
    }

    /// Whether this build command compiles inside a container (e.g. `cross`),
    /// so the cross toolchain — including the C compilers used by build scripts
    /// like `ring`/`zstd-sys` — lives in the image rather than on the host. When
    /// true, the `--print=native-static-libs` probe must run through this same
    /// command so it executes in the container, and the host cross-probe linker
    /// shim must NOT be injected.
    pub fn runs_in_container(&self) -> bool {
        matches!(self, Self::Cross)
    }

    /// The subcommand used to run the `--print=native-static-libs` probe.
    ///
    /// The probe needs raw `rustc` flags, so it always uses the `rustc`
    /// subcommand: `cargo rustc` for host builds (`cargo`/`zigbuild`) and
    /// `cross rustc` for containerized builds — the latter runs the probe inside
    /// the cross image where the target C toolchain is available.
    pub fn probe_subcommand(&self) -> &str {
        match self {
            Self::Cargo | Self::Zigbuild | Self::Cross => "rustc",
            Self::Custom { .. } => "rustc",
        }
    }

    fn from_token(s: &str) -> Self {
        match s {
            "cross" => Self::Cross,
            "zigbuild" => Self::Zigbuild,
            _ => Self::Custom {
                program: "cargo".to_string(),
                subcommand: s.to_string(),
            },
        }
    }

    fn from_parts(program: &str, subcommand: &str) -> Self {
        match (program, subcommand) {
            ("cargo", "build") => Self::Cargo,
            ("cargo", "zigbuild") => Self::Zigbuild,
            ("cross", "build") => Self::Cross,
            (p, s) => Self::Custom {
                program: p.to_string(),
                subcommand: s.to_string(),
            },
        }
    }
}

impl std::fmt::Display for CargoBuildCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.program(), self.subcommand())
    }
}

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
    pub package: Option<String>,
    pub cargo_args: Vec<String>,
    pub on_output: Option<OutputCallback>,
    /// Override the cargo build command (e.g. `cargo zigbuild` or `cross build`).
    /// When `None`, defaults to `cargo build`.
    pub cargo_build_command: Option<CargoBuildCommand>,
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
        let mut command = self.create_build_command(&command_args);
        command.arg("--target").arg(triple);

        self.apply_common_build_args(&mut command);
        command.args(&command_args.command_args);

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
        let mut cmd = self.create_build_command(&command_args);
        cmd.arg("--target").arg(target.triple());

        self.apply_common_build_args(&mut cmd);
        cmd.args(&command_args.command_args);

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

    fn package_name(&self) -> &str {
        self.options
            .package
            .as_deref()
            .unwrap_or(self.config.library_name())
    }

    fn apply_common_build_args(&self, command: &mut Command) {
        if self.options.release {
            command.arg("--release");
        }

        command.arg("-p").arg(self.package_name());
    }

    fn cargo_build_command_args(&self) -> CargoBuildCommandArgs {
        CargoBuildCommandArgs::from_passthrough_args(&self.options.cargo_args)
    }

    /// Create the base `Command` for a cargo build invocation, respecting any
    /// `cargo_build_command` override set in `BuildOptions`.
    fn create_build_command(&self, command_args: &CargoBuildCommandArgs) -> Command {
        let cmd = self.options.cargo_build_command.as_ref();
        let program = cmd.map(CargoBuildCommand::program).unwrap_or("cargo");
        let subcommand = cmd.map(CargoBuildCommand::subcommand).unwrap_or("build");
        let is_cargo = cmd.map(CargoBuildCommand::is_cargo_program).unwrap_or(true);

        let mut command = Command::new(program);

        // Toolchain selectors (+nightly, +stable, …) are a cargo-specific concept
        // and are not forwarded when using a standalone binary such as `cross`.
        if is_cargo && let Some(toolchain_selector) = command_args.toolchain_selector.as_deref() {
            command.arg(toolchain_selector);
        }

        command.arg(subcommand);
        command
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
        CargoBuildCommand, CargoBuildCommandArgs, CargoBuildProfile, resolve_build_profile,
        run_command_streaming,
    };
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn parses_zigbuild_as_cargo_subcommand() {
        assert_eq!(
            CargoBuildCommand::parse("zigbuild"),
            CargoBuildCommand::Zigbuild
        );
    }

    #[test]
    fn parses_cross_as_standalone_binary() {
        assert_eq!(CargoBuildCommand::parse("cross"), CargoBuildCommand::Cross);
    }

    #[test]
    fn parses_explicit_cargo_zigbuild() {
        assert_eq!(
            CargoBuildCommand::parse("cargo zigbuild"),
            CargoBuildCommand::Zigbuild
        );
    }

    #[test]
    fn parses_explicit_cross_build() {
        assert_eq!(
            CargoBuildCommand::parse("cross build"),
            CargoBuildCommand::Cross
        );
    }

    #[test]
    fn parses_explicit_cargo_build_as_cargo_variant() {
        assert_eq!(
            CargoBuildCommand::parse("cargo build"),
            CargoBuildCommand::Cargo
        );
    }

    #[test]
    fn parses_unknown_token_as_custom_cargo_subcommand() {
        assert_eq!(
            CargoBuildCommand::parse("nextest"),
            CargoBuildCommand::Custom {
                program: "cargo".to_string(),
                subcommand: "nextest".to_string(),
            }
        );
    }

    #[test]
    fn parses_unknown_program_subcommand_pair_as_custom() {
        assert_eq!(
            CargoBuildCommand::parse("my-tool build"),
            CargoBuildCommand::Custom {
                program: "my-tool".to_string(),
                subcommand: "build".to_string(),
            }
        );
    }

    #[test]
    fn display_formats_program_and_subcommand() {
        assert_eq!(CargoBuildCommand::Cargo.to_string(), "cargo build");
        assert_eq!(CargoBuildCommand::Zigbuild.to_string(), "cargo zigbuild");
        assert_eq!(CargoBuildCommand::Cross.to_string(), "cross build");
    }

    #[test]
    fn cargo_and_zigbuild_are_cargo_programs() {
        assert!(CargoBuildCommand::Cargo.is_cargo_program());
        assert!(CargoBuildCommand::Zigbuild.is_cargo_program());
        assert!(!CargoBuildCommand::Cross.is_cargo_program());
    }

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
