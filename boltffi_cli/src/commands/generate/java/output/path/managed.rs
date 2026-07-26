use std::{
    cmp::Reverse,
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use crate::cli::{CliError, Result};

use super::Directory;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManagedPath(String);

#[derive(Clone, Copy)]
pub enum SourceKind {
    Java,
    Jni,
}

pub struct ManagedRoots {
    package: ManagedPath,
    jni: ManagedPath,
}

impl ManagedPath {
    pub fn literal(path: &'static str) -> Self {
        Self(path.to_string())
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let segments = path
            .components()
            .map(|component| match component {
                Component::Normal(segment) => {
                    segment.to_str().ok_or_else(|| CliError::CommandFailed {
                        command: format!(
                            "generated Java path '{}' is not valid UTF-8",
                            path.display()
                        ),
                        status: None,
                    })
                }
                Component::Prefix(_)
                | Component::RootDir
                | Component::CurDir
                | Component::ParentDir => Err(CliError::CommandFailed {
                    command: format!(
                        "generated Java path '{}' is not a relative managed path",
                        path.display()
                    ),
                    status: None,
                }),
            })
            .collect::<Result<Vec<_>>>()?;
        if segments.is_empty()
            || segments.iter().any(|segment| {
                segment.is_empty() || segment.contains('/') || segment.contains('\\')
            })
        {
            return Err(CliError::CommandFailed {
                command: format!(
                    "generated Java path '{}' is not a relative managed path",
                    path.display()
                ),
                status: None,
            });
        }
        Ok(Self(segments.join("/")))
    }

    pub fn from_manifest(path: String) -> Result<Self> {
        if path.contains('\\') {
            return Err(CliError::CommandFailed {
                command: format!("Java generated-file manifest contains invalid path '{path}'"),
                status: None,
            });
        }
        Self::from_path(Path::new(&path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn path(&self) -> PathBuf {
        self.0.split('/').collect()
    }

    pub fn depth(&self) -> usize {
        self.0.split('/').count()
    }

    pub fn belongs_to(&self, root: &Self) -> bool {
        self.0
            .strip_prefix(&root.0)
            .is_some_and(|suffix| suffix.starts_with('/'))
    }

    pub fn is_manifest_source(&self) -> bool {
        if self.depth() < 2 || self.is_reserved() {
            return false;
        }
        match self
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some("java") => true,
            Some("c" | "h") => self.0.starts_with("jni/"),
            _ => false,
        }
    }

    pub fn parents(&self) -> Vec<Self> {
        let segments = self.0.split('/').collect::<Vec<_>>();
        (1..segments.len())
            .map(|length| Self(segments[..length].join("/")))
            .collect()
    }

    pub fn child(&self, segment: &str) -> Result<Self> {
        Self::from_path(&self.path().join(segment))
    }

    pub fn deepest_parents<'path>(paths: impl IntoIterator<Item = &'path Self>) -> Vec<Self> {
        let mut directories = paths
            .into_iter()
            .flat_map(Self::parents)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        directories.sort_by_key(|path| Reverse(path.depth()));
        directories
    }

    pub fn segments(&self) -> Vec<&str> {
        self.0.split('/').collect()
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().expect("managed path is nonempty")
    }

    fn is_reserved(&self) -> bool {
        matches!(
            self.0.split('/').next(),
            Some(".boltffi-java-prepare" | ".boltffi-java-transaction" | ".boltffi-java-cleanup")
        )
    }
}

impl SourceKind {
    pub fn accepts(self, path: &ManagedPath) -> bool {
        matches!(
            (
                self,
                path.path()
                    .extension()
                    .and_then(|extension| extension.to_str()),
            ),
            (Self::Java, Some("java")) | (Self::Jni, Some("c" | "h"))
        )
    }
}

impl ManagedRoots {
    pub fn new(package: &str) -> Result<Self> {
        Ok(Self {
            package: ManagedPath::from_path(Path::new(&package.replace('.', "/")))?,
            jni: ManagedPath::literal("jni"),
        })
    }

    pub fn accepts_output(&self, path: &ManagedPath) -> bool {
        (path.belongs_to(&self.package) && SourceKind::Java.accepts(path))
            || (path.belongs_to(&self.jni) && SourceKind::Jni.accepts(path))
    }

    pub fn sources(&self, directory: &Directory) -> Result<BTreeSet<ManagedPath>> {
        let mut sources = BTreeSet::new();
        [
            (&self.package, SourceKind::Java),
            (&self.jni, SourceKind::Jni),
        ]
        .into_iter()
        .try_for_each(|(path, kind)| directory.collect_sources(path, kind, &mut sources))?;
        Ok(sources)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ManagedPath> {
        [&self.package, &self.jni].into_iter()
    }
}
