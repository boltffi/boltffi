use std::{
    collections::BTreeSet,
    fs::{self, File as StdFile},
    io::{self, ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, ambient_authority};
use cap_std::fs::{Dir, File as CapFile, Metadata, OpenOptions};

use crate::cli::{CliError, Result};

use super::managed::{ManagedPath, SourceKind};

pub struct Directory {
    path: PathBuf,
    handle: Dir,
}

impl Directory {
    pub fn open_root(path: &Path) -> Result<Self> {
        fs::create_dir_all(path).map_err(|source| CliError::CreateDirectoryFailed {
            path: path.to_path_buf(),
            source,
        })?;
        let canonical = fs::canonicalize(path).map_err(|source| CliError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })?;
        let handle = Dir::open_ambient_dir(&canonical, ambient_authority()).map_err(|source| {
            CliError::ReadFailed {
                path: canonical.clone(),
                source,
            }
        })?;
        Ok(Self {
            path: canonical,
            handle,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sync(&self) -> Result<()> {
        #[cfg(windows)]
        {
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let directory = self
                .handle
                .open(".")
                .map_err(|source| CliError::WriteFailed {
                    path: self.path.clone(),
                    source,
                })?;
            directory
                .sync_all()
                .map_err(|source| CliError::WriteFailed {
                    path: self.path.clone(),
                    source,
                })
        }
    }
}

impl Directory {
    pub fn create_child(&self, name: &str) -> Result<Self> {
        if self.entry_metadata(name)?.is_some() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output state path '{}' already exists",
                    self.path.join(name).display()
                ),
                status: None,
            });
        }
        self.handle
            .create_dir(name)
            .map_err(|source| CliError::CreateDirectoryFailed {
                path: self.path.join(name),
                source,
            })?;
        self.sync()?;
        self.require_child(name)
    }

    pub fn require_child(&self, name: &str) -> Result<Self> {
        self.open_child(name)?.ok_or_else(|| CliError::ReadFailed {
            path: self.path.join(name),
            source: io::Error::new(ErrorKind::NotFound, "directory does not exist"),
        })
    }

    pub fn open_child(&self, name: &str) -> Result<Option<Self>> {
        let Some(metadata) = self.entry_metadata(name)? else {
            return Ok(None);
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output path '{}' is not a regular directory",
                    self.path.join(name).display()
                ),
                status: None,
            });
        }
        self.handle
            .open_dir_nofollow(name)
            .map(|handle| {
                Some(Self {
                    path: self.path.join(name),
                    handle,
                })
            })
            .map_err(|source| CliError::ReadFailed {
                path: self.path.join(name),
                source,
            })
    }

    pub fn remove_tree(&self, name: &str) -> Result<()> {
        let Some(directory) = self.open_child(name)? else {
            return Ok(());
        };
        directory
            .handle
            .remove_open_dir_all()
            .map_err(|source| CliError::WriteFailed {
                path: directory.path,
                source,
            })?;
        self.sync()
    }

    pub fn rename_child(&self, source: &str, destination: &str) -> Result<()> {
        if self.entry_metadata(destination)?.is_some() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output state path '{}' already exists",
                    self.path.join(destination).display()
                ),
                status: None,
            });
        }
        self.handle
            .rename(source, &self.handle, destination)
            .map_err(|rename_error| CliError::WriteFailed {
                path: self.path.join(destination),
                source: rename_error,
            })?;
        self.sync()
    }
}

impl Directory {
    pub fn read_regular(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let Some(mut file) = self.open_regular(name)? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| CliError::ReadFailed {
                path: self.path.join(name),
                source,
            })?;
        Ok(Some(bytes))
    }

    pub fn regular_exists(&self, name: &str) -> Result<bool> {
        self.open_regular(name).map(|file| file.is_some())
    }

    pub fn open_lock(&self, name: &str) -> Result<StdFile> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        options.follow(FollowSymlinks::No);
        let file =
            self.handle
                .open_with(name, &options)
                .map_err(|source| CliError::WriteFailed {
                    path: self.path.join(name),
                    source,
                })?;
        let metadata = file.metadata().map_err(|source| CliError::ReadFailed {
            path: self.path.join(name),
            source,
        })?;
        if !metadata.is_file() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output lock '{}' is not a regular file",
                    self.path.join(name).display()
                ),
                status: None,
            });
        }
        self.sync()?;
        Ok(file.into_std())
    }

    pub fn write_new(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        options.follow(FollowSymlinks::No);
        let mut file =
            self.handle
                .open_with(name, &options)
                .map_err(|source| CliError::WriteFailed {
                    path: self.path.join(name),
                    source,
                })?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| CliError::WriteFailed {
                path: self.path.join(name),
                source,
            })?;
        self.sync()
    }
}

impl Directory {
    pub fn inspect(&self, path: &ManagedPath) -> Result<Option<Metadata>> {
        let Some(parent) = self.open_parent(path)? else {
            return Ok(None);
        };
        parent.entry_metadata(path.file_name())
    }

    pub fn stage(&self, path: &ManagedPath, bytes: &[u8]) -> Result<()> {
        let parent = self.ensure_parent(path)?;
        parent.write_new(path.file_name(), bytes)
    }

    pub fn move_regular_if_exists(
        &self,
        source: &ManagedPath,
        destination: &Directory,
        destination_path: &ManagedPath,
    ) -> Result<bool> {
        let Some(source_parent) = self.open_parent(source)? else {
            return Ok(false);
        };
        let Some(metadata) = source_parent.entry_metadata(source.file_name())? else {
            return Ok(false);
        };
        source_parent.require_regular(source, &metadata)?;
        let destination_parent = destination.ensure_parent(destination_path)?;
        if destination_parent
            .entry_metadata(destination_path.file_name())?
            .is_some()
        {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output destination '{}' already exists",
                    destination.path.join(destination_path.path()).display()
                ),
                status: None,
            });
        }
        source_parent
            .handle
            .rename(
                source.file_name(),
                &destination_parent.handle,
                destination_path.file_name(),
            )
            .map_err(|rename_error| CliError::WriteFailed {
                path: destination.path.join(destination_path.path()),
                source: rename_error,
            })?;
        source_parent.sync()?;
        destination_parent.sync()?;
        Ok(true)
    }

    pub fn move_regular(
        &self,
        source: &ManagedPath,
        destination: &Directory,
        destination_path: &ManagedPath,
    ) -> Result<()> {
        match self.move_regular_if_exists(source, destination, destination_path)? {
            true => Ok(()),
            false => Err(CliError::CommandFailed {
                command: format!(
                    "Java output source '{}' is missing",
                    self.path.join(source.path()).display()
                ),
                status: None,
            }),
        }
    }

    pub fn copy_regular(
        &self,
        source: &ManagedPath,
        destination: &Directory,
        destination_path: &ManagedPath,
    ) -> Result<()> {
        let source_parent = self
            .open_parent(source)?
            .ok_or_else(|| CliError::ReadFailed {
                path: self.path.join(source.path()),
                source: io::Error::new(ErrorKind::NotFound, "source parent does not exist"),
            })?;
        let mut source_file = source_parent
            .open_regular(source.file_name())?
            .ok_or_else(|| CliError::ReadFailed {
                path: self.path.join(source.path()),
                source: io::Error::new(ErrorKind::NotFound, "source file does not exist"),
            })?;
        let destination_parent = destination.ensure_parent(destination_path)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        options.follow(FollowSymlinks::No);
        let mut destination_file = destination_parent
            .handle
            .open_with(destination_path.file_name(), &options)
            .map_err(|source| CliError::WriteFailed {
                path: destination.path.join(destination_path.path()),
                source,
            })?;
        io::copy(&mut source_file, &mut destination_file)
            .and_then(|_| destination_file.sync_all())
            .map_err(|copy_error| CliError::CopyFailed {
                from: self.path.join(source.path()),
                to: destination.path.join(destination_path.path()),
                source: copy_error,
            })?;
        destination_parent.sync()
    }

    pub fn remove_regular(&self, path: &ManagedPath) -> Result<()> {
        let Some(parent) = self.open_parent(path)? else {
            return Ok(());
        };
        let Some(metadata) = parent.entry_metadata(path.file_name())? else {
            return Ok(());
        };
        parent.require_regular(path, &metadata)?;
        parent
            .handle
            .remove_file(path.file_name())
            .map_err(|source| CliError::WriteFailed {
                path: self.path.join(path.path()),
                source,
            })?;
        parent.sync()
    }

    pub fn remove_empty_directory(&self, path: &ManagedPath) -> Result<()> {
        let Some(parent) = self.open_parent(path)? else {
            return Ok(());
        };
        let Some(metadata) = parent.entry_metadata(path.file_name())? else {
            return Ok(());
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output directory '{}' is not a regular directory",
                    self.path.join(path.path()).display()
                ),
                status: None,
            });
        }
        match parent.handle.remove_dir(path.file_name()) {
            Ok(()) => parent.sync(),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Ok(())
            }
            Err(source) => Err(CliError::WriteFailed {
                path: self.path.join(path.path()),
                source,
            }),
        }
    }
}

impl Directory {
    pub fn collect_sources(
        &self,
        root: &ManagedPath,
        kind: SourceKind,
        sources: &mut BTreeSet<ManagedPath>,
    ) -> Result<()> {
        let Some(directory) = self.open_directory(root)? else {
            return Ok(());
        };
        directory.collect_source_entries(root, kind, sources)
    }

    fn collect_source_entries(
        &self,
        relative: &ManagedPath,
        kind: SourceKind,
        sources: &mut BTreeSet<ManagedPath>,
    ) -> Result<()> {
        self.handle
            .entries()
            .map_err(|source| CliError::ReadFailed {
                path: self.path.clone(),
                source,
            })?
            .map(|entry| {
                entry.map_err(|source| CliError::ReadFailed {
                    path: self.path.clone(),
                    source,
                })
            })
            .try_for_each(|entry| {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_str().ok_or_else(|| CliError::CommandFailed {
                    command: format!(
                        "Java managed output contains a non-UTF-8 path under '{}'",
                        self.path.display()
                    ),
                    status: None,
                })?;
                let child = relative.child(name)?;
                let file_type = entry.file_type().map_err(|source| CliError::ReadFailed {
                    path: self.path.join(name),
                    source,
                })?;
                if file_type.is_symlink() {
                    return match kind.accepts(&child) {
                        true => Err(CliError::CommandFailed {
                            command: format!(
                                "Java managed source '{}' is a symbolic link",
                                self.path.join(name).display()
                            ),
                            status: None,
                        }),
                        false => Ok(()),
                    };
                }
                if file_type.is_dir() {
                    return self
                        .require_child(name)?
                        .collect_source_entries(&child, kind, sources);
                }
                if kind.accepts(&child) {
                    if !file_type.is_file() {
                        return Err(CliError::CommandFailed {
                            command: format!(
                                "Java managed source '{}' is not a regular file",
                                self.path.join(name).display()
                            ),
                            status: None,
                        });
                    }
                    sources.insert(child);
                }
                Ok(())
            })
    }
}

impl Directory {
    fn open_directory(&self, path: &ManagedPath) -> Result<Option<Self>> {
        path.segments()
            .into_iter()
            .try_fold(Some(self.duplicate()?), |directory, segment| {
                directory
                    .map(|directory| directory.open_child(segment))
                    .transpose()
                    .map(Option::flatten)
            })
    }

    fn open_parent(&self, path: &ManagedPath) -> Result<Option<Self>> {
        let segments = path.segments();
        segments[..segments.len() - 1].iter().try_fold(
            Some(self.duplicate()?),
            |directory, segment| {
                directory
                    .map(|directory| directory.open_child(segment))
                    .transpose()
                    .map(Option::flatten)
            },
        )
    }

    fn ensure_parent(&self, path: &ManagedPath) -> Result<Self> {
        let segments = path.segments();
        segments[..segments.len() - 1]
            .iter()
            .try_fold(self.duplicate()?, |directory, segment| {
                directory.ensure_child(segment)
            })
    }

    fn ensure_child(&self, name: &str) -> Result<Self> {
        match self.open_child(name)? {
            Some(directory) => Ok(directory),
            None => {
                self.handle
                    .create_dir(name)
                    .map_err(|source| CliError::CreateDirectoryFailed {
                        path: self.path.join(name),
                        source,
                    })?;
                self.sync()?;
                self.require_child(name)
            }
        }
    }

    fn duplicate(&self) -> Result<Self> {
        self.handle
            .try_clone()
            .map(|handle| Self {
                path: self.path.clone(),
                handle,
            })
            .map_err(|source| CliError::ReadFailed {
                path: self.path.clone(),
                source,
            })
    }

    fn entry_metadata(&self, name: &str) -> Result<Option<Metadata>> {
        match self.handle.symlink_metadata(name) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(source) => Err(CliError::ReadFailed {
                path: self.path.join(name),
                source,
            }),
        }
    }

    fn open_regular(&self, name: &str) -> Result<Option<CapFile>> {
        let Some(metadata) = self.entry_metadata(name)? else {
            return Ok(None);
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output path '{}' is not a regular file",
                    self.path.join(name).display()
                ),
                status: None,
            });
        }
        let mut options = OpenOptions::new();
        options.read(true);
        options.follow(FollowSymlinks::No);
        self.handle
            .open_with(name, &options)
            .map(Some)
            .map_err(|source| CliError::ReadFailed {
                path: self.path.join(name),
                source,
            })
    }

    fn require_regular(&self, path: &ManagedPath, metadata: &Metadata) -> Result<()> {
        match metadata.is_file() && !metadata.file_type().is_symlink() {
            true => Ok(()),
            false => Err(CliError::CommandFailed {
                command: format!(
                    "managed Java output '{}' is not a regular file",
                    self.path.join(path.file_name()).display()
                ),
                status: None,
            }),
        }
    }
}
