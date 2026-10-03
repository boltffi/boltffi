use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::build::{OutputCallback, run_command_streaming};
use crate::cli::{CliError, Result};
use crate::pack::{format_command_for_log, print_verbose_detail};
use crate::reporter::Step;

use super::build::BuiltPythonSharedLibrary;
use super::plan::{PythonInterpreterSelection, PythonPackagingPlan};
use super::runtime::PythonRuntimeVersion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonBuiltWheel {
    pub interpreter: String,
    pub wheel_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonBuiltWheelMatrix {
    pub wheels: Vec<PythonBuiltWheel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonInterpreter {
    command: String,
    executable: PathBuf,
    identity: PythonInterpreterIdentity,
    /// `sysconfig`'s `CFLAGS`, which setuptools compiles extensions with; absent
    /// on Windows.
    cflags: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PythonInterpreterIdentity {
    resolved_executable: PathBuf,
    prefix: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonInterpreterRuntime {
    identity: PythonInterpreterIdentity,
    version: PythonRuntimeVersion,
    cflags: Option<String>,
}

impl PythonInterpreter {
    fn discover_default() -> Result<Self> {
        ["python3", "python"]
            .into_iter()
            .find_map(|command| {
                PythonInterpreterSelection::new(command)
                    .ok()
                    .and_then(|selection| Self::discover(&selection).ok())
            })
            .ok_or_else(|| CliError::CommandFailed {
                command: "python packaging requires python3 or python in PATH".to_string(),
                status: None,
            })
    }

    fn discover(selection: &PythonInterpreterSelection) -> Result<Self> {
        let executable = resolve_interpreter_executable(selection.command())?;
        let runtime = Self::probe_runtime(&executable)?;
        Self::ensure_supported_version(selection.command(), runtime.version)?;
        let status = Command::new(&executable)
            .args(["-m", "pip", "--version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|source| CliError::CommandFailed {
                command: format!("{} -m pip --version: {source}", selection.command()),
                status: None,
            })?;

        status
            .success()
            .then_some(Self {
                command: selection.command().to_string(),
                executable,
                identity: runtime.identity,
                cflags: runtime.cflags,
            })
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "python packaging requires pip support for interpreter '{}'",
                    selection.command()
                ),
                status: status.code(),
            })
    }

    fn probe_runtime(executable: &Path) -> Result<PythonInterpreterRuntime> {
        #[derive(Deserialize)]
        struct InterpreterIdentityProbe {
            resolved_executable: PathBuf,
            prefix: PathBuf,
            version_major: u8,
            version_minor: u8,
            cflags: Option<String>,
        }

        let output = Command::new(executable)
            .args([
                "-c",
                "import json, pathlib, sys, sysconfig; print(json.dumps({'resolved_executable': str(pathlib.Path(sys.executable).resolve()), 'prefix': sys.prefix, 'version_major': sys.version_info[0], 'version_minor': sys.version_info[1], 'cflags': sysconfig.get_config_var('CFLAGS')}))",
            ])
            .output()
            .map_err(|source| CliError::CommandFailed {
                command: format!("{} -c <python identity probe>: {source}", executable.display()),
                status: None,
            })?;

        if !output.status.success() {
            return Err(CliError::CommandFailed {
                command: format!("{} -c <python identity probe>", executable.display()),
                status: output.status.code(),
            });
        }

        serde_json::from_slice::<InterpreterIdentityProbe>(&output.stdout)
            .map(|probe| PythonInterpreterRuntime {
                identity: PythonInterpreterIdentity {
                    resolved_executable: probe.resolved_executable,
                    prefix: probe.prefix,
                },
                version: PythonRuntimeVersion::new(probe.version_major, probe.version_minor),
                cflags: probe.cflags,
            })
            .map_err(|source| CliError::CommandFailed {
                command: format!(
                    "{} -c <python identity probe>: failed to parse interpreter identity: {source}",
                    executable.display()
                ),
                status: None,
            })
    }

    fn ensure_supported_version(command: &str, version: PythonRuntimeVersion) -> Result<()> {
        let minimum_supported_version = PythonRuntimeVersion::minimum_supported();

        (version >= minimum_supported_version)
            .then_some(())
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "python packaging requires Python >= {minimum_supported_version}; interpreter '{command}' resolved to Python {version}"
                ),
                status: None,
            })
    }

    fn wheel_command(
        &self,
        source_root: &Path,
        wheel_directory: &Path,
        cflags: Option<String>,
    ) -> Command {
        let mut command = Command::new(&self.executable);
        command.current_dir(source_root);
        command
            .args(["-m", "pip", "wheel", ".", "--wheel-dir"])
            .arg(wheel_directory)
            .arg("--no-deps");
        if let Some(cflags) = cflags {
            command.env("CFLAGS", cflags);
        }
        command
    }

    /// `$CFLAGS` for the extension build, or `None` to leave it alone.
    ///
    /// A debug pack compiles the extension unoptimized, as the dev-profile cdylib
    /// it wraps is. Current setuptools takes `$CFLAGS` in place of the
    /// interpreter's own flags (older releases appended it), so those are passed
    /// through and only the optimization is overridden after them (`-fno-lto`: an
    /// LTO-built interpreter would otherwise still emit fat LTO objects). A
    /// caller's own `$CFLAGS` is left as it is. MSVC reads no `$CFLAGS`, so
    /// Windows is unchanged.
    fn extension_cflags(
        &self,
        optimize_extension: bool,
        caller_cflags: Option<OsString>,
    ) -> Option<String> {
        if optimize_extension || caller_cflags.is_some() {
            return None;
        }
        Some(
            self.cflags
                .iter()
                .map(String::as_str)
                .chain(["-O0 -fno-lto"])
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}

pub struct PythonWheelBuilder<'a> {
    plan: &'a PythonPackagingPlan,
    interpreters: Vec<PythonInterpreter>,
}

impl<'a> PythonWheelBuilder<'a> {
    pub fn new(plan: &'a PythonPackagingPlan) -> Result<Self> {
        let discovered_interpreters = if plan.interpreters.is_empty() {
            vec![PythonInterpreter::discover_default()?]
        } else {
            plan.interpreters
                .iter()
                .map(PythonInterpreter::discover)
                .collect::<Result<Vec<_>>>()?
        };
        let interpreters = Self::deduplicate_interpreters(discovered_interpreters);

        Ok(Self { plan, interpreters })
    }

    pub fn build(
        &self,
        shared_library: &BuiltPythonSharedLibrary,
        step: &Step,
    ) -> Result<PythonBuiltWheelMatrix> {
        self.plan.layout.validate_generated_sources()?;
        self.plan.layout.remove_packaged_native_libraries()?;
        self.plan.layout.prepare_wheel_directory()?;
        self.stage_shared_library(shared_library)?;

        let verbose = step.is_verbose();

        let wheels = self
            .interpreters
            .iter()
            .map(|interpreter| self.build_for_interpreter(interpreter, verbose))
            .collect::<Result<Vec<_>>>()?;

        Ok(PythonBuiltWheelMatrix { wheels })
    }

    fn build_for_interpreter(
        &self,
        interpreter: &PythonInterpreter,
        verbose: bool,
    ) -> Result<PythonBuiltWheel> {
        let existing_wheels = self.current_wheels()?;
        let mut command = interpreter.wheel_command(
            &self.plan.layout.root_directory,
            &self.plan.layout.wheel_directory,
            interpreter.extension_cflags(
                self.plan.cargo_context.build_profile.is_release_like(),
                std::env::var_os("CFLAGS"),
            ),
        );

        if verbose {
            print_verbose_detail(&format!(
                "python wheel command [{}]: {}",
                interpreter.command,
                format_command_for_log(&command)
            ));
            let on_output: Option<OutputCallback> =
                Some(Box::new(|line: &str| print_verbose_detail(line)) as OutputCallback);

            if !run_command_streaming(&mut command, on_output.as_ref()) {
                return Err(CliError::CommandFailed {
                    command: format!("{} -m pip wheel", interpreter.command),
                    status: None,
                });
            }
        } else {
            let output = command.output().map_err(|source| CliError::CommandFailed {
                command: format!("{} -m pip wheel: {source}", interpreter.command),
                status: None,
            })?;

            if !output.status.success() {
                let failure_output = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .chain(String::from_utf8_lossy(&output.stderr).lines())
                    .filter(|line| !line.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ");

                return Err(CliError::CommandFailed {
                    command: format!("{} -m pip wheel: {}", interpreter.command, failure_output),
                    status: output.status.code(),
                });
            }
        }

        self.new_wheel(interpreter, &existing_wheels)
    }

    fn new_wheel(
        &self,
        interpreter: &PythonInterpreter,
        existing_wheels: &[PathBuf],
    ) -> Result<PythonBuiltWheel> {
        let new_wheels = self
            .current_wheels()?
            .into_iter()
            .filter(|wheel_path| !existing_wheels.contains(wheel_path))
            .collect::<Vec<_>>();

        match new_wheels.as_slice() {
            [wheel_path] => Ok(PythonBuiltWheel {
                interpreter: interpreter.command.clone(),
                wheel_path: wheel_path.clone(),
            }),
            [] => Err(CliError::CommandFailed {
                command: format!(
                    "python packaging did not produce a wheel for interpreter '{}'",
                    interpreter.command
                ),
                status: None,
            }),
            _ => Err(CliError::CommandFailed {
                command: format!(
                    "python packaging produced multiple new wheels for interpreter '{}'",
                    interpreter.command
                ),
                status: None,
            }),
        }
    }

    fn current_wheels(&self) -> Result<Vec<PathBuf>> {
        std::fs::read_dir(&self.plan.layout.wheel_directory)
            .map_err(|source| CliError::ReadFailed {
                path: self.plan.layout.wheel_directory.clone(),
                source,
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| CliError::ReadFailed {
                path: self.plan.layout.wheel_directory.clone(),
                source,
            })
            .map(|paths| {
                paths
                    .into_iter()
                    .filter(|path| path.extension().is_some_and(|extension| extension == "whl"))
                    .collect()
            })
    }

    fn stage_shared_library(&self, shared_library: &BuiltPythonSharedLibrary) -> Result<PathBuf> {
        let packaged_shared_library_path = self.plan.packaged_shared_library_path();
        std::fs::copy(&shared_library.source_path, &packaged_shared_library_path).map_err(
            |source| CliError::CopyFailed {
                from: shared_library.source_path.clone(),
                to: packaged_shared_library_path.clone(),
                source,
            },
        )?;
        Ok(packaged_shared_library_path)
    }

    fn deduplicate_interpreters(interpreters: Vec<PythonInterpreter>) -> Vec<PythonInterpreter> {
        let mut seen_interpreter_identities = HashSet::new();

        interpreters
            .into_iter()
            .filter(|interpreter| seen_interpreter_identities.insert(interpreter.identity.clone()))
            .collect()
    }
}

fn resolve_interpreter_executable(command: &str) -> Result<PathBuf> {
    let command_path = Path::new(command);

    if command_path.components().count() > 1 || command_path.is_absolute() {
        let executable = command_path
            .exists()
            .then(|| absolutize_interpreter_path(command_path))
            .ok_or_else(|| CliError::CommandFailed {
                command: format!("python interpreter '{}' does not exist", command),
                status: None,
            })?;

        return Ok(executable);
    }

    which::which(command).map_err(|_| CliError::CommandFailed {
        command: format!("python interpreter '{}' not found in PATH", command),
        status: None,
    })
}

fn absolutize_interpreter_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current_directory| current_directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    use super::{PythonInterpreter, PythonInterpreterIdentity, PythonWheelBuilder};
    use crate::cli::CliError;
    use crate::pack::python::runtime::PythonRuntimeVersion;

    #[test]
    fn rejects_interpreters_older_than_supported_python_floor() {
        let error = PythonInterpreter::ensure_supported_version(
            "python3.9",
            PythonRuntimeVersion::new(3, 9),
        )
        .expect_err("expected unsupported Python interpreter version");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("requires Python >= 3.10")
                    && command.contains("interpreter 'python3.9'")
                    && command.contains("resolved to Python 3.9")
        ));
    }

    #[test]
    fn deduplicates_interpreter_aliases_after_resolution() {
        let interpreters = PythonWheelBuilder::deduplicate_interpreters(vec![
            PythonInterpreter {
                command: "python".to_string(),
                executable: PathBuf::from("/usr/bin/python3.12"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.12"),
                    prefix: PathBuf::from("/usr"),
                },
                cflags: None,
            },
            PythonInterpreter {
                command: "python3".to_string(),
                executable: PathBuf::from("/usr/bin/python3.12"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.12"),
                    prefix: PathBuf::from("/usr"),
                },
                cflags: None,
            },
            PythonInterpreter {
                command: "python3.13".to_string(),
                executable: PathBuf::from("/usr/bin/python3.13"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.13"),
                    prefix: PathBuf::from("/usr"),
                },
                cflags: None,
            },
        ]);

        assert_eq!(
            interpreters
                .iter()
                .map(|interpreter| interpreter.command.as_str())
                .collect::<Vec<_>>(),
            vec!["python", "python3.13"]
        );
    }

    #[test]
    fn preserves_virtualenv_interpreters_with_shared_base_executable() {
        let interpreters = PythonWheelBuilder::deduplicate_interpreters(vec![
            PythonInterpreter {
                command: "/tmp/.venv/bin/python".to_string(),
                executable: PathBuf::from("/tmp/.venv/bin/python"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.12"),
                    prefix: PathBuf::from("/tmp/.venv"),
                },
                cflags: None,
            },
            PythonInterpreter {
                command: "/usr/bin/python3.12".to_string(),
                executable: PathBuf::from("/usr/bin/python3.12"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.12"),
                    prefix: PathBuf::from("/usr"),
                },
                cflags: None,
            },
        ]);

        assert_eq!(interpreters.len(), 2);
    }

    fn interpreter(cflags: Option<&str>) -> PythonInterpreter {
        PythonInterpreter {
            command: "python3".to_string(),
            executable: PathBuf::from("/usr/bin/python3"),
            identity: PythonInterpreterIdentity {
                resolved_executable: PathBuf::from("/usr/bin/python3"),
                prefix: PathBuf::from("/usr"),
            },
            cflags: cflags.map(str::to_string),
        }
    }

    /// The interpreter's flags survive (on a universal2 macOS build they carry
    /// the `-arch` pair), and only the optimization is overridden after them.
    #[test]
    fn debug_extension_build_keeps_the_interpreter_flags_but_not_its_optimization() {
        let cflags = interpreter(Some("-DNDEBUG -O3 -arch arm64 -arch x86_64 -flto=auto"))
            .extension_cflags(false, None);

        assert_eq!(
            cflags.as_deref(),
            Some("-DNDEBUG -O3 -arch arm64 -arch x86_64 -flto=auto -O0 -fno-lto"),
        );
        assert_eq!(
            interpreter(None).extension_cflags(false, None).as_deref(),
            Some("-O0 -fno-lto"),
        );
    }

    #[test]
    fn release_and_caller_cflags_leave_the_extension_flags_alone() {
        let interpreter = interpreter(Some("-O3"));

        assert_eq!(interpreter.extension_cflags(true, None), None);
        assert_eq!(
            interpreter.extension_cflags(false, Some(OsString::from("-O2 -g"))),
            None,
        );
    }

    #[test]
    fn wheel_command_sets_cflags_only_when_given() {
        let cflags = |cflags: Option<&str>| {
            interpreter(None)
                .wheel_command(
                    Path::new("/tmp/pkg"),
                    Path::new("/tmp/wheelhouse"),
                    cflags.map(str::to_string),
                )
                .get_envs()
                .find(|(key, _)| *key == OsStr::new("CFLAGS"))
                .and_then(|(_, value)| value.map(OsStr::to_os_string))
        };

        assert_eq!(cflags(Some("-O0")), Some(OsString::from("-O0")));
        assert_eq!(cflags(None), None);
    }
}
