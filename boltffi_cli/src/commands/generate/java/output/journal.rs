use std::io::{Error, ErrorKind};

use serde::{Deserialize, Serialize};

use crate::cli::{CliError, Result};

use super::{
    MANIFEST,
    ownership::Manifest,
    path::{Directory, ManagedPath},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Active,
    Restored,
    Committed,
}

#[derive(Deserialize, Serialize)]
struct JournalDocument {
    version: u8,
    previous: Vec<String>,
    next: Vec<String>,
}

pub struct Journal {
    phase: Phase,
    previous: Manifest,
    next: Manifest,
}

impl Journal {
    pub const FILE: &'static str = "journal.json";
    const VERSION: u8 = 1;
    const RESTORED: &'static str = "restored";
    const COMMITTED: &'static str = "committed";

    pub fn new(previous: Manifest, next: Manifest) -> Self {
        Self {
            phase: Phase::Active,
            previous,
            next,
        }
    }

    pub fn load(directory: &Directory) -> Result<Self> {
        let path = directory.path().join(Self::FILE);
        let bytes = directory
            .read_regular(Self::FILE)?
            .ok_or_else(|| CliError::ReadFailed {
                path: path.clone(),
                source: Error::new(ErrorKind::NotFound, "recovery journal does not exist"),
            })?;
        let document = serde_json::from_slice::<JournalDocument>(&bytes).map_err(|error| {
            CliError::CommandFailed {
                command: format!(
                    "read Java output recovery journal '{}': {error}",
                    path.display()
                ),
                status: None,
            }
        })?;
        if document.version != Self::VERSION {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java output recovery journal '{}' has unsupported version {}",
                    path.display(),
                    document.version
                ),
                status: None,
            });
        }
        Ok(Self {
            phase: Self::read_phase(directory)?,
            previous: Manifest::from_paths(
                document.previous,
                &format!("output recovery journal '{}'", path.display()),
            )?,
            next: Manifest::from_paths(
                document.next,
                &format!("output recovery journal '{}'", path.display()),
            )?,
        })
    }

    pub fn write(&self, directory: &Directory) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&JournalDocument {
            version: Self::VERSION,
            previous: self
                .previous
                .files()
                .map(|path| path.as_str().to_string())
                .collect(),
            next: self
                .next
                .files()
                .map(|path| path.as_str().to_string())
                .collect(),
        })
        .map_err(|error| CliError::CommandFailed {
            command: format!("write Java output recovery journal: {error}"),
            status: None,
        })?;
        directory.write_new(Self::FILE, &bytes)
    }

    pub fn mark(&mut self, directory: &Directory, phase: Phase) -> Result<()> {
        let marker = match phase {
            Phase::Active => {
                return Err(CliError::CommandFailed {
                    command: "cannot mark a Java output transaction active".to_owned(),
                    status: None,
                });
            }
            Phase::Restored => Self::RESTORED,
            Phase::Committed => Self::COMMITTED,
        };
        directory.write_new(marker, &[])?;
        self.phase = phase;
        Ok(())
    }

    pub const fn phase(&self) -> Phase {
        self.phase
    }

    pub const fn next(&self) -> &Manifest {
        &self.next
    }

    pub fn install_paths(&self) -> impl Iterator<Item = ManagedPath> + '_ {
        self.next
            .files()
            .cloned()
            .chain([ManagedPath::literal(MANIFEST)])
    }

    pub fn backup_paths(&self) -> impl Iterator<Item = ManagedPath> + '_ {
        self.previous
            .files()
            .cloned()
            .chain([ManagedPath::literal(MANIFEST)])
    }

    fn read_phase(directory: &Directory) -> Result<Phase> {
        match (
            directory.regular_exists(Self::RESTORED)?,
            directory.regular_exists(Self::COMMITTED)?,
        ) {
            (false, false) => Ok(Phase::Active),
            (true, false) => Ok(Phase::Restored),
            (false, true) => Ok(Phase::Committed),
            (true, true) => Err(CliError::CommandFailed {
                command: format!(
                    "Java output recovery directory '{}' contains conflicting phase markers",
                    directory.path().display()
                ),
                status: None,
            }),
        }
    }
}
