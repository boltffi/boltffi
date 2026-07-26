use std::collections::BTreeSet;

use boltffi_backend::GeneratedOutput;
use serde::{Deserialize, Serialize};

use crate::cli::{CliError, Result};

use super::{
    MANIFEST,
    path::{Directory, ManagedPath, ManagedRoots},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    files: BTreeSet<ManagedPath>,
}

#[derive(Deserialize, Serialize)]
struct ManifestDocument {
    version: u8,
    files: Vec<String>,
}

impl Manifest {
    const VERSION: u8 = 1;

    pub fn empty() -> Self {
        Self {
            files: BTreeSet::new(),
        }
    }

    pub fn from_output(output: &GeneratedOutput, roots: &ManagedRoots) -> Result<Self> {
        output
            .files()
            .iter()
            .try_fold(Self::empty(), |mut manifest, file| {
                let path = ManagedPath::from_path(file.path().as_path())?;
                if path.as_str() == MANIFEST || !roots.accepts_output(&path) {
                    return Err(CliError::CommandFailed {
                        command: format!(
                            "Java backend emitted unmanaged output path '{}'",
                            path.as_str()
                        ),
                        status: None,
                    });
                }
                if !manifest.files.insert(path.clone()) {
                    return Err(CliError::CommandFailed {
                        command: format!(
                            "Java backend emitted duplicate output path '{}'",
                            path.as_str()
                        ),
                        status: None,
                    });
                }
                Ok(manifest)
            })
    }

    pub fn load_owned(directory: &Directory, roots: &ManagedRoots) -> Result<Self> {
        let sources = roots.sources(directory)?;
        let Some(bytes) = directory.read_regular(MANIFEST)? else {
            return match sources.is_empty() {
                true => Ok(Self::empty()),
                false => Err(Self::unowned_error(directory, &sources)),
            };
        };
        let manifest = Self::from_bytes(directory, &bytes)?;
        let unclaimed = sources
            .difference(&manifest.files)
            .cloned()
            .collect::<BTreeSet<_>>();
        match unclaimed.is_empty() {
            true => Ok(manifest),
            false => Err(Self::unowned_error(directory, &unclaimed)),
        }
    }

    pub fn from_paths(paths: Vec<String>, document: &str) -> Result<Self> {
        paths
            .into_iter()
            .map(ManagedPath::from_manifest)
            .try_fold(BTreeSet::new(), |mut files, path| {
                let path = path?;
                if !files.insert(path.clone()) {
                    return Err(CliError::CommandFailed {
                        command: format!(
                            "Java {document} contains duplicate path '{}'",
                            path.as_str()
                        ),
                        status: None,
                    });
                }
                Ok(files)
            })
            .and_then(|files| {
                if files.iter().all(ManagedPath::is_manifest_source) {
                    Ok(Self { files })
                } else {
                    Err(CliError::CommandFailed {
                        command: format!("Java {document} claims an unmanaged path"),
                        status: None,
                    })
                }
            })
    }

    pub fn bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(&ManifestDocument {
            version: Self::VERSION,
            files: self
                .files
                .iter()
                .map(|path| path.as_str().to_string())
                .collect(),
        })
        .map_err(|error| CliError::CommandFailed {
            command: format!("write Java generated-file manifest: {error}"),
            status: None,
        })
    }

    pub fn validate_additions(&self, directory: &Directory, next: &Self) -> Result<()> {
        next.files
            .difference(&self.files)
            .try_for_each(|path| match directory.inspect(path)? {
                None => Ok(()),
                Some(_) => Err(CliError::CommandFailed {
                    command: format!(
                        "refusing to replace unowned Java output '{}'",
                        directory.path().join(path.path()).display()
                    ),
                    status: None,
                }),
            })
    }

    pub fn files(&self) -> impl Iterator<Item = &ManagedPath> {
        self.files.iter()
    }

    fn from_bytes(directory: &Directory, bytes: &[u8]) -> Result<Self> {
        let path = directory.path().join(MANIFEST);
        let document = serde_json::from_slice::<ManifestDocument>(bytes).map_err(|error| {
            CliError::CommandFailed {
                command: format!(
                    "read Java generated-file manifest '{}': {error}",
                    path.display()
                ),
                status: None,
            }
        })?;
        if document.version != Self::VERSION {
            return Err(CliError::CommandFailed {
                command: format!(
                    "Java generated-file manifest '{}' has unsupported version {}",
                    path.display(),
                    document.version
                ),
                status: None,
            });
        }
        Self::from_paths(
            document.files,
            &format!("generated-file manifest '{}'", path.display()),
        )
    }

    fn unowned_error(directory: &Directory, paths: &BTreeSet<ManagedPath>) -> CliError {
        CliError::CommandFailed {
            command: format!(
                "Java output contains source files without manifest ownership: {}",
                paths
                    .iter()
                    .map(|path| directory.path().join(path.path()).display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            status: None,
        }
    }
}
