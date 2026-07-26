mod journal;
mod ownership;
mod path;
mod transaction;

use std::path::{Path, PathBuf};

use boltffi_backend::GeneratedOutput;

use crate::cli::Result;

use self::{
    ownership::Manifest,
    path::{Directory, ManagedPath, ManagedRoots},
    transaction::{OutputLock, Staging, Transaction},
};

const MANIFEST: &str = ".boltffi-java-manifest.json";
const LOCK: &str = ".boltffi-java.lock";
const PREPARE: &str = ".boltffi-java-prepare";
const LIVE: &str = ".boltffi-java-transaction";
const TOMBSTONE: &str = ".boltffi-java-cleanup";

pub struct Output {
    root: PathBuf,
    roots: ManagedRoots,
}

impl Output {
    pub fn new(root: &Path, package: &str) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            roots: ManagedRoots::new(package)?,
        })
    }

    pub fn write(&self, output: GeneratedOutput) -> Result<()> {
        let next = Manifest::from_output(&output, &self.roots)?;
        let root = Directory::open_root(&self.root)?;
        let output_lock = OutputLock::acquire(&root)?;
        Transaction::recover(&root)?;
        let previous = Manifest::load_owned(&root, &self.roots)?;
        previous.validate_additions(&root, &next)?;
        let staging = Staging::new(&root, previous.clone(), next)?;
        output.files().iter().try_for_each(|file| {
            staging.stage(
                &ManagedPath::from_path(file.path().as_path())?,
                file.contents().as_bytes(),
            )
        })?;
        staging.stage_manifest()?;
        staging.activate()?.commit()?;
        let result = ManagedPath::deepest_parents(previous.files().chain(self.roots.iter()))
            .into_iter()
            .try_for_each(|path| root.remove_empty_directory(&path));
        drop(output_lock);
        result
    }
}

#[cfg(test)]
mod tests;
