use std::fs::{File, TryLockError};

use crate::cli::{CliError, Result};

use super::{
    LIVE, LOCK, MANIFEST, PREPARE, TOMBSTONE,
    journal::{Journal, Phase},
    ownership::Manifest,
    path::{Directory, ManagedPath},
};

pub struct OutputLock {
    file: File,
}

pub struct Staging<'root> {
    root: &'root Directory,
    directory: Directory,
    journal: Journal,
}

pub struct Transaction<'root> {
    root: &'root Directory,
    directory: Directory,
    journal: Journal,
}

impl OutputLock {
    pub fn acquire(root: &Directory) -> Result<Self> {
        let path = root.path().join(LOCK);
        let file = root.open_lock(LOCK)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { file }),
            Err(TryLockError::WouldBlock) => Err(CliError::CommandFailed {
                command: format!(
                    "Java output '{}' is locked by another generator",
                    root.path().display()
                ),
                status: None,
            }),
            Err(TryLockError::Error(source)) => Err(CliError::WriteFailed { path, source }),
        }
    }
}

impl Drop for OutputLock {
    fn drop(&mut self) {
        drop(self.file.unlock());
    }
}

impl<'root> Staging<'root> {
    pub fn new(root: &'root Directory, previous: Manifest, next: Manifest) -> Result<Self> {
        let directory = root.create_child(PREPARE)?;
        ["new", "backup", "restore"]
            .into_iter()
            .try_for_each(|name| directory.create_child(name).map(drop))?;
        directory.sync()?;
        Ok(Self {
            root,
            directory,
            journal: Journal::new(previous, next),
        })
    }

    pub fn stage(&self, path: &ManagedPath, bytes: &[u8]) -> Result<()> {
        self.directory.require_child("new")?.stage(path, bytes)
    }

    pub fn stage_manifest(&self) -> Result<()> {
        self.stage(
            &ManagedPath::literal(MANIFEST),
            &self.journal.next().bytes()?,
        )
    }

    pub fn activate(self) -> Result<Transaction<'root>> {
        self.journal.write(&self.directory)?;
        self.directory.sync()?;
        let Self {
            root,
            directory,
            journal,
        } = self;
        drop(directory);
        root.rename_child(PREPARE, LIVE)?;
        let directory = root.require_child(LIVE)?;
        Ok(Transaction {
            root,
            directory,
            journal,
        })
    }
}

impl<'root> Transaction<'root> {
    pub fn recover(root: &'root Directory) -> Result<()> {
        root.remove_tree(TOMBSTONE)?;
        if let Some(directory) = root.open_child(LIVE)? {
            let journal = Journal::load(&directory)?;
            let transaction = Self {
                root,
                directory,
                journal,
            };
            match transaction.journal.phase() {
                Phase::Active => transaction.rollback()?,
                Phase::Restored | Phase::Committed => transaction.cleanup()?,
            }
        }
        root.remove_tree(PREPARE)
    }

    pub fn commit(mut self) -> Result<()> {
        match self.apply() {
            Ok(()) => match self.journal.mark(&self.directory, Phase::Committed) {
                Ok(()) => self.cleanup(),
                Err(primary) => self.abort(primary),
            },
            Err(primary) => self.abort(primary),
        }
    }

    pub fn apply(&mut self) -> Result<()> {
        self.journal
            .backup_paths()
            .collect::<Vec<_>>()
            .into_iter()
            .try_for_each(|path| self.backup(&path))?;
        self.journal
            .install_paths()
            .collect::<Vec<_>>()
            .into_iter()
            .try_for_each(|path| self.install(&path))
    }

    fn abort(self, primary: CliError) -> Result<()> {
        match self.rollback() {
            Ok(()) => Err(primary),
            Err(recovery) => Err(CliError::CommandFailed {
                command: format!(
                    "Java output transaction failed: {primary}; recovery also failed: {recovery}"
                ),
                status: None,
            }),
        }
    }

    fn backup(&self, path: &ManagedPath) -> Result<()> {
        self.root
            .move_regular_if_exists(path, &self.directory.require_child("backup")?, path)?;
        Ok(())
    }

    fn install(&self, path: &ManagedPath) -> Result<()> {
        if self.root.inspect(path)?.is_some() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "refusing to replace Java output '{}' after it changed during generation",
                    self.root.path().join(path.path()).display()
                ),
                status: None,
            });
        }
        self.directory
            .require_child("new")?
            .move_regular(path, self.root, path)
    }

    fn rollback(mut self) -> Result<()> {
        let mut failures = Vec::new();
        self.journal
            .install_paths()
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|path| {
                if let Err(error) = self.remove_installed(&path) {
                    failures.push(error.to_string());
                }
            });
        self.journal
            .backup_paths()
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|path| {
                if let Err(error) = self.restore(&path) {
                    failures.push(error.to_string());
                }
            });
        ManagedPath::deepest_parents(self.journal.next().files())
            .into_iter()
            .for_each(|path| {
                if let Err(error) = self.root.remove_empty_directory(&path) {
                    failures.push(error.to_string());
                }
            });
        if !failures.is_empty() {
            return Err(self.recovery_failure(failures));
        }
        self.journal.mark(&self.directory, Phase::Restored)?;
        self.cleanup()
    }

    fn remove_installed(&self, path: &ManagedPath) -> Result<()> {
        let staged = self.directory.require_child("new")?;
        match staged.inspect(path)? {
            Some(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
            Some(_) => Err(CliError::CommandFailed {
                command: format!(
                    "staged Java output '{}' is not a regular file",
                    staged.path().join(path.path()).display()
                ),
                status: None,
            }),
            None => self.root.remove_regular(path),
        }
    }

    fn restore(&self, path: &ManagedPath) -> Result<()> {
        let backup = self.directory.require_child("backup")?;
        let Some(metadata) = backup.inspect(path)? else {
            return Ok(());
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output backup '{}' is not a regular file",
                    backup.path().join(path.path()).display()
                ),
                status: None,
            });
        }
        let restore = self.directory.require_child("restore")?;
        restore.remove_regular(path)?;
        backup.copy_regular(path, &restore, path)?;
        self.root.remove_regular(path)?;
        restore.move_regular(path, self.root, path)
    }

    fn cleanup(self) -> Result<()> {
        let Self {
            root,
            directory,
            journal,
        } = self;
        drop(journal);
        drop(directory);
        root.rename_child(LIVE, TOMBSTONE)?;
        root.remove_tree(TOMBSTONE)
    }

    fn recovery_failure(&self, failures: Vec<String>) -> CliError {
        CliError::CommandFailed {
            command: format!(
                "Java output recovery failed: {}; recovery evidence remains at '{}'",
                failures.join("; "),
                self.directory.path().display()
            ),
            status: None,
        }
    }
}
