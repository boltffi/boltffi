use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

use tempfile::TempDir;
use walkdir::WalkDir;

use crate::cli::{CliError, Result};

pub struct Package {
    output: PathBuf,
    staging: TempDir,
}

impl Package {
    pub fn new(output: &Path) -> Result<Self> {
        fs::create_dir_all(output).map_err(|source| CliError::CreateDirectoryFailed {
            path: output.to_owned(),
            source,
        })?;
        let output = output
            .canonicalize()
            .map_err(|source| CliError::ReadFailed {
                path: output.to_owned(),
                source,
            })?;
        let parent = output.parent().unwrap_or(&output);
        let staging = tempfile::Builder::new()
            .prefix(".boltffi-wasm-")
            .tempdir_in(parent)
            .map_err(|source| CliError::CreateDirectoryFailed {
                path: parent.to_owned(),
                source,
            })?;
        let package = Self { output, staging };
        fs::create_dir(package.path()).map_err(|source| CliError::CreateDirectoryFailed {
            path: package.path(),
            source,
        })?;
        Ok(package)
    }

    pub fn path(&self) -> PathBuf {
        self.staging.path().join("package")
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    pub fn copy(&self, source: &Path, relative: impl AsRef<Path>) -> Result<()> {
        let destination = self.path().join(relative);
        let parent = destination.parent().expect("package file has a parent");
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
            path: parent.to_owned(),
            source,
        })?;
        fs::copy(source, &destination)
            .map(drop)
            .map_err(|error| CliError::CopyFailed {
                from: source.to_owned(),
                to: destination,
                source: error,
            })
    }

    pub fn files(&self) -> Result<BTreeSet<PathBuf>> {
        let staged = self.path();
        WalkDir::new(&staged)
            .into_iter()
            .try_fold(BTreeSet::new(), |mut files, entry| {
                let entry = entry.map_err(|error| CliError::CommandFailed {
                    command: format!("read staged Wasm package: {error}"),
                    status: None,
                })?;
                if entry.file_type().is_file() {
                    files.insert(
                        entry
                            .path()
                            .strip_prefix(&staged)
                            .expect("staged entry")
                            .to_owned(),
                    );
                }
                Ok(files)
            })
    }

    pub fn publish(self) -> Result<()> {
        let staged = self.path();
        let mut installed_files = self.files()?;
        let lock_path = self.output.join(".boltffi-pack.lock");
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| CliError::WriteFailed {
                path: lock_path,
                source,
            })?;
        lock.try_lock().map_err(|error| CliError::CommandFailed {
            command: format!("cannot lock Wasm output {}: {error}", self.output.display()),
            status: None,
        })?;
        let manifest_name = PathBuf::from(".boltffi-wasm-files.json");
        let manifest_path = self.output.join(&manifest_name);
        let previous: BTreeSet<PathBuf> = match fs::read(&manifest_path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|error| CliError::CommandFailed {
                    command: format!(
                        "invalid Wasm package manifest {}: {error}",
                        manifest_path.display()
                    ),
                    status: None,
                })?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeSet::new(),
            Err(source) => {
                return Err(CliError::ReadFailed {
                    path: manifest_path,
                    source,
                });
            }
        };
        if previous.iter().any(|path| {
            path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
        }) {
            return Err(CliError::CommandFailed {
                command: "Wasm package manifest contains a nonlocal path".to_owned(),
                status: None,
            });
        }
        let staged_manifest = staged.join(&manifest_name);
        fs::write(
            &staged_manifest,
            serde_json::to_vec(&installed_files).expect("paths serialize"),
        )
        .map_err(|source| CliError::WriteFailed {
            path: staged_manifest,
            source,
        })?;
        installed_files.insert(manifest_name);
        let mut changed_files = installed_files.union(&previous);
        changed_files.clone().try_for_each(|relative| {
            let mut parent = self.output.clone();
            relative
                .parent()
                .into_iter()
                .flat_map(Path::components)
                .try_for_each(|component| {
                    parent.push(component);
                    match fs::symlink_metadata(&parent) {
                        Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => {
                            Err(CliError::CommandFailed {
                                command: format!(
                                    "Wasm package directory {} is not a regular directory",
                                    parent.display()
                                ),
                                status: None,
                            })
                        }
                        Ok(_) => Ok(()),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(source) => Err(CliError::ReadFailed {
                            path: parent.clone(),
                            source,
                        }),
                    }
                })
        })?;
        let backup = self.staging.path().join("backup");
        let mut installed = Vec::new();
        let mut replaced = Vec::new();
        let result = changed_files.try_for_each(|relative| {
            let destination = self.output.join(relative);
            let parent = destination.parent().expect("package file has a parent");
            fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
                path: parent.to_owned(),
                source,
            })?;
            match fs::symlink_metadata(&destination) {
                Ok(metadata) if !metadata.file_type().is_file() => {
                    return Err(CliError::CommandFailed {
                        command: format!(
                            "refusing to replace non-file Wasm output {}",
                            destination.display()
                        ),
                        status: None,
                    });
                }
                Ok(_) => {
                    let saved = backup.join(relative);
                    fs::create_dir_all(saved.parent().expect("backup parent")).map_err(
                        |source| CliError::CreateDirectoryFailed {
                            path: backup.clone(),
                            source,
                        },
                    )?;
                    fs::rename(&destination, &saved).map_err(|source| CliError::WriteFailed {
                        path: destination.clone(),
                        source,
                    })?;
                    replaced.push(relative.clone());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(CliError::ReadFailed {
                        path: destination.clone(),
                        source,
                    });
                }
            }
            if installed_files.contains(relative) {
                fs::rename(staged.join(relative), &destination).map_err(|source| {
                    CliError::WriteFailed {
                        path: destination,
                        source,
                    }
                })?;
                installed.push(relative.clone());
            }
            Ok::<_, CliError>(())
        });
        if let Err(primary) = result {
            let mut failures = Vec::new();
            installed.iter().rev().for_each(|relative| {
                if let Err(error) = fs::remove_file(self.output.join(relative)) {
                    failures.push(error.to_string());
                }
            });
            replaced.iter().rev().for_each(|relative| {
                if let Err(error) = fs::rename(backup.join(relative), self.output.join(relative)) {
                    failures.push(error.to_string());
                }
            });
            if !failures.is_empty() {
                let retained = self.staging.keep();
                return Err(CliError::CommandFailed {
                    command: format!(
                        "{primary}; rollback failed: {}; backup retained at {}",
                        failures.join("; "),
                        retained.display()
                    ),
                    status: None,
                });
            }
            return Err(primary);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Package;
    use std::fs;

    #[test]
    fn publishing_replaces_generated_files_and_preserves_user_files() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("dist");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("user.txt"), "keep").unwrap();
        let first = Package::new(&output).unwrap();
        fs::write(first.path().join("entry.js"), "old").unwrap();
        fs::write(first.path().join("old_glue.js"), "obsolete").unwrap();
        first.publish().unwrap();
        let second = Package::new(&output).unwrap();
        fs::write(second.path().join("entry.js"), "new").unwrap();
        fs::create_dir(second.path().join("snippets")).unwrap();
        fs::write(second.path().join("snippets/local.js"), "local").unwrap();
        assert_eq!(fs::read_to_string(output.join("entry.js")).unwrap(), "old");
        second.publish().unwrap();
        assert_eq!(fs::read_to_string(output.join("entry.js")).unwrap(), "new");
        assert_eq!(fs::read_to_string(output.join("user.txt")).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(output.join("snippets/local.js")).unwrap(),
            "local"
        );
        assert!(!output.join("old_glue.js").exists());
    }

    #[test]
    fn failed_publication_restores_the_previous_package() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("dist");
        let first = Package::new(&output).unwrap();
        fs::write(first.path().join("entry.js"), "old").unwrap();
        first.publish().unwrap();
        let manifest = fs::read(output.join(".boltffi-wasm-files.json")).unwrap();
        fs::create_dir(output.join("z.js")).unwrap();
        let second = Package::new(&output).unwrap();
        fs::write(second.path().join("entry.js"), "new").unwrap();
        fs::write(second.path().join("z.js"), "conflict").unwrap();
        assert!(second.publish().is_err());
        assert_eq!(fs::read_to_string(output.join("entry.js")).unwrap(), "old");
        assert_eq!(
            fs::read(output.join(".boltffi-wasm-files.json")).unwrap(),
            manifest
        );
        assert!(output.join("z.js").is_dir());
    }

    #[test]
    fn invalid_manifests_cannot_remove_files_outside_the_output() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("dist");
        fs::create_dir(&output).unwrap();
        fs::write(root.path().join("user.txt"), "keep").unwrap();
        fs::write(
            output.join(".boltffi-wasm-files.json"),
            r#"["../user.txt"]"#,
        )
        .unwrap();
        let package = Package::new(&output).unwrap();
        fs::write(package.path().join("entry.js"), "new").unwrap();
        assert!(package.publish().is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("user.txt")).unwrap(),
            "keep"
        );
        assert!(!output.join("entry.js").exists());
    }

    #[cfg(unix)]
    #[test]
    fn snippet_directory_symlinks_cannot_redirect_publication() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("dist");
        fs::create_dir(&output).unwrap();
        fs::create_dir(root.path().join("outside")).unwrap();
        fs::write(root.path().join("outside/local.js"), "keep").unwrap();
        std::os::unix::fs::symlink(root.path().join("outside"), output.join("snippets")).unwrap();
        let package = Package::new(&output).unwrap();
        fs::create_dir(package.path().join("snippets")).unwrap();
        fs::write(package.path().join("snippets/local.js"), "new").unwrap();
        assert!(package.publish().is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("outside/local.js")).unwrap(),
            "keep"
        );
    }
}
