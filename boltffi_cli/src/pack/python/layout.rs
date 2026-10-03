use std::path::{Path, PathBuf};

use crate::cli::{CliError, Result};
use crate::target::NativeHostPlatform;

/// Name of the compiled CPython bridge extension inside the generated package.
pub const NATIVE_EXTENSION_MODULE: &str = "_native";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonPackageLayout {
    pub root_directory: PathBuf,
    pub package_directory: PathBuf,
    pub wheel_directory: PathBuf,
    pub pyproject_path: PathBuf,
    pub setup_script_path: PathBuf,
    pub package_init_path: PathBuf,
    pub package_stub_path: PathBuf,
    pub typed_marker_path: PathBuf,
    pub native_bridge_source_path: PathBuf,
}

impl PythonPackageLayout {
    pub fn new(root_directory: impl AsRef<Path>, module_name: &str) -> Self {
        let root_directory = root_directory.as_ref().to_path_buf();
        let wheel_directory = root_directory.join("wheelhouse");
        Self::with_wheel_directory(root_directory, wheel_directory, module_name)
    }

    pub fn with_wheel_directory(
        root_directory: impl AsRef<Path>,
        wheel_directory: impl AsRef<Path>,
        module_name: &str,
    ) -> Self {
        let root_directory = root_directory.as_ref().to_path_buf();
        let package_directory = root_directory.join(module_name);
        let wheel_directory = wheel_directory.as_ref().to_path_buf();

        Self {
            wheel_directory,
            pyproject_path: root_directory.join("pyproject.toml"),
            setup_script_path: root_directory.join("setup.py"),
            package_init_path: package_directory.join("__init__.py"),
            package_stub_path: package_directory.join("__init__.pyi"),
            typed_marker_path: package_directory.join("py.typed"),
            native_bridge_source_path: package_directory
                .join(format!("{NATIVE_EXTENSION_MODULE}.c")),
            root_directory,
            package_directory,
        }
    }

    pub fn packaged_shared_library_path(
        &self,
        host_platform: NativeHostPlatform,
        artifact_name: &str,
    ) -> PathBuf {
        self.package_directory
            .join(host_platform.shared_library_filename(artifact_name))
    }

    pub fn validate_generated_sources(&self) -> Result<()> {
        [
            &self.pyproject_path,
            &self.setup_script_path,
            &self.package_init_path,
            &self.package_stub_path,
            &self.typed_marker_path,
            &self.native_bridge_source_path,
        ]
        .into_iter()
        .find(|path| !path.exists())
        .cloned()
        .map_or(Ok(()), |path| Err(CliError::FileNotFound(path)))
    }

    pub fn validate_wheel_directory_safety(&self) -> Result<()> {
        if self.root_directory.starts_with(&self.wheel_directory) {
            return Err(CliError::CommandFailed {
                command: format!(
                    "targets.python.wheel.output '{}' must not be the same as or contain targets.python.output '{}'",
                    self.wheel_directory.display(),
                    self.root_directory.display()
                ),
                status: None,
            });
        }

        if self.package_directory.starts_with(&self.wheel_directory) {
            return Err(CliError::CommandFailed {
                command: format!(
                    "targets.python.wheel.output '{}' must not be the same as or contain the generated Python package directory '{}'",
                    self.wheel_directory.display(),
                    self.package_directory.display()
                ),
                status: None,
            });
        }

        if self.wheel_directory.starts_with(&self.package_directory) {
            return Err(CliError::CommandFailed {
                command: format!(
                    "targets.python.wheel.output '{}' must not be inside the generated Python package directory '{}'",
                    self.wheel_directory.display(),
                    self.package_directory.display()
                ),
                status: None,
            });
        }

        if let Some(setuptools_directory) = self.setuptools_state_directory_containing_wheels() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "targets.python.wheel.output '{}' must not be inside the setuptools build directory '{}', which is removed before each wheel build",
                    self.wheel_directory.display(),
                    setuptools_directory.display()
                ),
                status: None,
            });
        }

        Ok(())
    }

    /// The `build/` or `*.egg-info` directory of the source root that holds the
    /// wheel directory, if any.
    fn setuptools_state_directory_containing_wheels(&self) -> Option<PathBuf> {
        let first_component = self
            .wheel_directory
            .strip_prefix(&self.root_directory)
            .ok()?
            .components()
            .next()?;
        let first_path = Path::new(first_component.as_os_str());
        let is_setuptools_state = first_path == Path::new("build")
            || first_path
                .extension()
                .is_some_and(|extension| extension == "egg-info");

        is_setuptools_state.then(|| self.root_directory.join(first_path))
    }

    pub fn prepare_wheel_directory(&self) -> Result<()> {
        self.validate_wheel_directory_safety()?;

        match std::fs::remove_dir_all(&self.wheel_directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(CliError::WriteFailed {
                    path: self.wheel_directory.clone(),
                    source,
                });
            }
        }

        std::fs::create_dir_all(&self.wheel_directory).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: self.wheel_directory.clone(),
                source,
            }
        })
    }

    /// Removes the setuptools `build/` directory and this distribution's
    /// `.egg-info` from the source root. setuptools reuses them by mtime alone,
    /// so a leftover tree can put a stale `_native` extension or stale files
    /// into the next wheel.
    pub fn remove_setuptools_build_state(&self, distribution_name: &str) -> Result<()> {
        std::iter::once(self.root_directory.join("build"))
            .chain(
                setuptools_egg_info_names(distribution_name)
                    .into_iter()
                    .map(|name| self.root_directory.join(name)),
            )
            .try_for_each(|path| match std::fs::remove_dir_all(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(CliError::WriteFailed { path, source }),
            })
    }

    pub fn remove_packaged_native_libraries(&self) -> Result<()> {
        std::fs::read_dir(&self.package_directory)
            .map_err(|source| CliError::ReadFailed {
                path: self.package_directory.clone(),
                source,
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| CliError::ReadFailed {
                path: self.package_directory.clone(),
                source,
            })?
            .into_iter()
            .filter(|path| Self::is_packaged_native_library(path))
            .try_for_each(|path| match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(CliError::WriteFailed { path, source }),
            })
    }

    fn is_packaged_native_library(path: &Path) -> bool {
        path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "dll" | "dylib" | "so"))
    }
}

/// The `.egg-info` directory names setuptools writes for `distribution_name`:
/// `filename_component(safe_name(name))`. setuptools 69.1 stopped collapsing
/// `-`/`_` in `safe_name`, so both spellings are returned when they differ.
fn setuptools_egg_info_names(distribution_name: &str) -> Vec<String> {
    let safe_name = |keeps: fn(char) -> bool| {
        distribution_name
            .split(|character: char| !keeps(character))
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    };
    let filename_component =
        |name: String| format!("{}.egg-info", name.replace('-', "_").trim_matches('_'));
    let current = filename_component(safe_name(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
    }));
    let legacy = filename_component(safe_name(|character| {
        character.is_ascii_alphanumeric() || character == '.'
    }));

    if current == legacy {
        vec![current]
    } else {
        vec![current, legacy]
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{PythonPackageLayout, setuptools_egg_info_names};
    use crate::cli::CliError;
    use crate::target::NativeHostPlatform;

    #[test]
    fn builds_expected_python_package_paths() {
        let layout = PythonPackageLayout::new(PathBuf::from("dist/python"), "demo_lib");

        assert_eq!(layout.root_directory, PathBuf::from("dist/python"));
        assert_eq!(
            layout.package_directory,
            PathBuf::from("dist/python/demo_lib")
        );
        assert_eq!(
            layout.wheel_directory,
            PathBuf::from("dist/python/wheelhouse")
        );
        assert_eq!(
            layout.pyproject_path,
            PathBuf::from("dist/python/pyproject.toml")
        );
        assert_eq!(
            layout.setup_script_path,
            PathBuf::from("dist/python/setup.py")
        );
        assert_eq!(
            layout.package_init_path,
            PathBuf::from("dist/python/demo_lib/__init__.py")
        );
        assert_eq!(
            layout.package_stub_path,
            PathBuf::from("dist/python/demo_lib/__init__.pyi")
        );
        assert_eq!(
            layout.typed_marker_path,
            PathBuf::from("dist/python/demo_lib/py.typed")
        );
        assert_eq!(
            layout.native_bridge_source_path,
            PathBuf::from("dist/python/demo_lib/_native.c")
        );
        assert_eq!(
            layout.packaged_shared_library_path(NativeHostPlatform::DarwinArm64, "demo_lib"),
            PathBuf::from("dist/python/demo_lib/libdemo_lib.dylib")
        );
    }

    #[test]
    fn rejects_wheel_directory_that_matches_source_root() {
        let layout =
            PythonPackageLayout::with_wheel_directory("dist/python", "dist/python", "demo_lib");

        let error = layout
            .validate_wheel_directory_safety()
            .expect_err("expected unsafe wheel directory rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("targets.python.wheel.output")
        ));
    }

    #[test]
    fn rejects_wheel_directory_that_matches_package_directory() {
        let layout = PythonPackageLayout::with_wheel_directory(
            "dist/python",
            "dist/python/demo_lib",
            "demo_lib",
        );

        let error = layout
            .validate_wheel_directory_safety()
            .expect_err("expected unsafe package wheel directory rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("generated Python package directory")
        ));
    }

    #[test]
    fn rejects_wheel_directory_nested_inside_package_directory() {
        let layout = PythonPackageLayout::with_wheel_directory(
            "dist/python",
            "dist/python/demo_lib/wheelhouse",
            "demo_lib",
        );

        let error = layout
            .validate_wheel_directory_safety()
            .expect_err("expected nested package wheel directory rejection");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("must not be inside the generated Python package directory")
        ));
    }

    #[test]
    fn removes_packaged_native_libraries_without_touching_sources() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let root_directory =
            std::env::temp_dir().join(format!("boltffi-python-package-layout-{unique_suffix}"));
        let layout = PythonPackageLayout::new(&root_directory, "demo_lib");

        fs::create_dir_all(&layout.package_directory).expect("create generated package directory");
        fs::write(layout.package_directory.join("__init__.py"), []).expect("write package init");
        fs::write(layout.package_directory.join("libold.dylib"), []).expect("write stale dylib");
        fs::write(layout.package_directory.join("libother.so"), []).expect("write stale so");
        fs::write(layout.package_directory.join("demo.dll"), []).expect("write stale dll");

        layout
            .remove_packaged_native_libraries()
            .expect("remove stale packaged native libraries");

        assert!(layout.package_directory.join("__init__.py").exists());
        assert!(!layout.package_directory.join("libold.dylib").exists());
        assert!(!layout.package_directory.join("libother.so").exists());
        assert!(!layout.package_directory.join("demo.dll").exists());

        fs::remove_dir_all(root_directory).expect("cleanup generated package directory");
    }

    #[test]
    fn removes_setuptools_build_state_without_touching_sources() {
        let root_directory = tempfile::tempdir().expect("create generated python root");
        let layout = PythonPackageLayout::new(root_directory.path(), "demo_lib");
        let stale_object = root_directory
            .path()
            .join("build/temp.linux-x86_64-cpython-313/demo_lib/_native.o");

        fs::create_dir_all(stale_object.parent().expect("object directory"))
            .expect("create stale build tree");
        fs::write(&stale_object, []).expect("write stale object");
        fs::create_dir_all(root_directory.path().join("demo_lib.egg-info"))
            .expect("create stale egg-info");
        fs::create_dir_all(root_directory.path().join("other_project.egg-info"))
            .expect("create unrelated egg-info");
        fs::create_dir_all(&layout.package_directory).expect("create generated package directory");
        fs::write(&layout.setup_script_path, []).expect("write setup script");
        fs::write(&layout.native_bridge_source_path, []).expect("write native bridge source");

        layout
            .remove_setuptools_build_state("demo-lib")
            .expect("remove setuptools build state");
        layout
            .remove_setuptools_build_state("demo-lib")
            .expect("removing absent build state is a no-op");

        assert!(!root_directory.path().join("build").exists());
        assert!(!root_directory.path().join("demo_lib.egg-info").exists());
        assert!(
            root_directory
                .path()
                .join("other_project.egg-info")
                .exists()
        );
        assert!(layout.setup_script_path.exists());
        assert!(layout.native_bridge_source_path.exists());
    }

    #[test]
    fn derives_setuptools_egg_info_names() {
        [
            ("demo", vec!["demo.egg-info"]),
            ("demo-ffi", vec!["demo_ffi.egg-info"]),
            ("demo_ffi", vec!["demo_ffi.egg-info"]),
            ("_demo-", vec!["demo.egg-info"]),
            ("Demo.Lib", vec!["Demo.Lib.egg-info"]),
            ("my_-crate", vec!["my__crate.egg-info", "my_crate.egg-info"]),
        ]
        .into_iter()
        .for_each(|(distribution_name, expected)| {
            assert_eq!(
                setuptools_egg_info_names(distribution_name),
                expected,
                "{distribution_name}"
            );
        });
    }

    #[test]
    fn rejects_wheel_directory_inside_setuptools_build_state() {
        [
            "dist/python/build",
            "dist/python/build/wheels",
            "dist/python/demo_lib.egg-info/wheels",
        ]
        .into_iter()
        .for_each(|wheel_directory| {
            let layout = PythonPackageLayout::with_wheel_directory(
                "dist/python",
                wheel_directory,
                "demo_lib",
            );

            let error = layout
                .validate_wheel_directory_safety()
                .expect_err("expected setuptools build wheel directory rejection");

            assert!(
                matches!(
                    &error,
                    CliError::CommandFailed { command, status: None }
                        if command.contains("must not be inside the setuptools build directory")
                ),
                "{wheel_directory}: {error:?}"
            );
        });
    }

    #[test]
    fn accepts_wheel_directory_next_to_setuptools_build_state() {
        [
            "dist/python/wheelhouse",
            "dist/python/builds",
            "dist/python/wheelhouse/build",
            "dist/wheels",
            "build",
        ]
        .into_iter()
        .for_each(|wheel_directory| {
            PythonPackageLayout::with_wheel_directory("dist/python", wheel_directory, "demo_lib")
                .validate_wheel_directory_safety()
                .unwrap_or_else(|error| panic!("{wheel_directory}: {error:?}"));
        });
    }
}
