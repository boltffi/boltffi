use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::build::{OutputCallback, run_command_streaming};
use crate::cli::{CliError, Result};
use crate::pack::{format_command_for_log, print_verbose_detail};
use crate::reporter::Step;

use super::build::BuiltPythonSharedLibrary;
use super::layout::NATIVE_EXTENSION_MODULE;
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
        }

        let output = Command::new(executable)
            .args([
                "-c",
                "import json, pathlib, sys; print(json.dumps({'resolved_executable': str(pathlib.Path(sys.executable).resolve()), 'prefix': sys.prefix, 'version_major': sys.version_info[0], 'version_minor': sys.version_info[1]}))",
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

    fn wheel_command(&self, source_root: &Path, wheel_directory: &Path) -> Command {
        let mut command = Command::new(&self.executable);
        command.current_dir(source_root);
        command
            .args(["-m", "pip", "wheel", ".", "--wheel-dir"])
            .arg(wheel_directory)
            .arg("--no-deps");
        command
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
        self.plan
            .layout
            .remove_setuptools_build_state(&self.plan.distribution_name)?;
        let existing_wheels = self.current_wheels()?;
        let mut command = interpreter.wheel_command(
            &self.plan.layout.root_directory,
            &self.plan.layout.wheel_directory,
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

        let built_wheel = self.new_wheel(interpreter, &existing_wheels)?;
        self.verify_wheel_contents(&built_wheel.wheel_path)?;
        Ok(built_wheel)
    }

    /// `pip wheel` can succeed without the compiled bridge extension or the
    /// staged shared library in the wheel; fail the pack instead.
    fn verify_wheel_contents(&self, wheel_path: &Path) -> Result<()> {
        verify_wheel_contents(
            wheel_path,
            &self.plan.module_name,
            NATIVE_EXTENSION_MODULE,
            &self
                .plan
                .host_platform
                .shared_library_filename(&self.plan.cargo_context.artifact_name),
        )
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

/// Checks that the wheel has `<package>/<extension_module>.*.so` (or `.pyd`) and
/// `<package>/<shared_library>`. A Windows debug interpreter names the extension
/// `<extension_module>_d.*.pyd`, which is accepted too.
fn verify_wheel_contents(
    wheel_path: &Path,
    package: &str,
    extension_module: &str,
    shared_library: &str,
) -> Result<()> {
    let archive = std::fs::File::open(wheel_path)
        .map_err(|source| CliError::ReadFailed {
            path: wheel_path.to_path_buf(),
            source,
        })
        .and_then(|file| {
            zip::ZipArchive::new(file).map_err(|source| CliError::CommandFailed {
                command: format!("failed to read wheel '{}': {source}", wheel_path.display()),
                status: None,
            })
        })?;

    let extension_prefix = format!("{package}/{extension_module}.");
    let debug_extension_prefix = format!("{package}/{extension_module}_d.");
    let has_extension = archive.file_names().any(|name| {
        (name.starts_with(&extension_prefix) && (name.ends_with(".so") || name.ends_with(".pyd")))
            || (name.starts_with(&debug_extension_prefix) && name.ends_with(".pyd"))
    });
    let shared_library_entry = format!("{package}/{shared_library}");
    let has_shared_library = archive
        .file_names()
        .any(|name| name == shared_library_entry);

    let missing = [
        (!has_extension).then(|| format!("the compiled {package}.{extension_module} extension")),
        (!has_shared_library).then_some(shared_library_entry),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    missing
        .is_empty()
        .then_some(())
        .ok_or_else(|| CliError::CommandFailed {
            command: format!(
                "python wheel '{}' is missing {}",
                wheel_path.display(),
                missing.join(" and ")
            ),
            status: None,
        })
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
    use std::path::PathBuf;

    use super::{
        PythonInterpreter, PythonInterpreterIdentity, PythonWheelBuilder, verify_wheel_contents,
    };
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

    fn write_wheel(directory: &std::path::Path, entries: &[&str]) -> PathBuf {
        let wheel_path = directory.join("demo-0.1.0-cp313-cp313-linux_x86_64.whl");
        let mut writer =
            zip::ZipWriter::new(std::fs::File::create(&wheel_path).expect("create wheel"));
        entries.iter().for_each(|entry| {
            writer
                .start_file(*entry, zip::write::SimpleFileOptions::default())
                .expect("start wheel entry");
        });
        writer.finish().expect("finish wheel");
        wheel_path
    }

    #[test]
    fn accepts_wheel_with_extension_and_shared_library() {
        let directory = tempfile::tempdir().expect("create wheel directory");

        [
            "demo_ffi/_native.cpython-313-x86_64-linux-gnu.so",
            "demo_ffi/_native.cp313-win_amd64.pyd",
            "demo_ffi/_native_d.cp313-win_amd64.pyd",
            "demo_ffi/_native_d.pyd",
        ]
        .into_iter()
        .for_each(|extension| {
            let wheel_path = write_wheel(
                directory.path(),
                &["demo_ffi/__init__.py", extension, "demo_ffi/libdemo_ffi.so"],
            );

            verify_wheel_contents(&wheel_path, "demo_ffi", "_native", "libdemo_ffi.so")
                .expect("expected complete wheel");
        });
    }

    #[test]
    fn rejects_wheel_without_extension_or_shared_library() {
        let directory = tempfile::tempdir().expect("create wheel directory");
        let wheel_path = write_wheel(
            directory.path(),
            &[
                "demo_ffi/__init__.py",
                "demo_ffi/_native.c",
                "demo_ffi/_native_d.cpython-313-x86_64-linux-gnu.so",
                "demo_ffi/_native_debug.cp313-win_amd64.pyd",
                "other/_native.cpython-313-x86_64-linux-gnu.so",
                "other/libdemo_ffi.so",
            ],
        );

        let error = verify_wheel_contents(&wheel_path, "demo_ffi", "_native", "libdemo_ffi.so")
            .expect_err("expected incomplete wheel rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("missing the compiled demo_ffi._native extension")
                    && command.contains("and demo_ffi/libdemo_ffi.so")
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
            },
            PythonInterpreter {
                command: "python3".to_string(),
                executable: PathBuf::from("/usr/bin/python3.12"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.12"),
                    prefix: PathBuf::from("/usr"),
                },
            },
            PythonInterpreter {
                command: "python3.13".to_string(),
                executable: PathBuf::from("/usr/bin/python3.13"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.13"),
                    prefix: PathBuf::from("/usr"),
                },
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
            },
            PythonInterpreter {
                command: "/usr/bin/python3.12".to_string(),
                executable: PathBuf::from("/usr/bin/python3.12"),
                identity: PythonInterpreterIdentity {
                    resolved_executable: PathBuf::from("/usr/bin/python3.12"),
                    prefix: PathBuf::from("/usr"),
                },
            },
        ]);

        assert_eq!(interpreters.len(), 2);
    }
}
