//! Copies the Rust crate folder into the gem.
//!
//! The gem builds the crate when it installs, so the gem carries the crate
//! folder and its `Cargo.lock`. The copy keeps the folder layout, so a path
//! dependency inside the crate folder keeps its relative path.

use std::{
    fs,
    path::{Path, PathBuf},
};

use toml_edit::{DocumentMut, Item, Table, TableLike, Value};
use walkdir::{DirEntry, WalkDir};

use crate::cli::{CliError, Result};

const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

const REWRITTEN_MANIFEST_HEADER: &str = "# boltffi pack ruby copied this manifest and made it a workspace root.\n# Path dependencies outside the copied crate folder use absolute paths.\n";

/// A path dependency that the copy cannot carry.
pub(super) struct ExternalPathDependency {
    /// The manifest that declares the dependency, relative to the crate root.
    pub(super) manifest: PathBuf,
    pub(super) name: String,
    /// The absolute path that the copied manifest uses.
    pub(super) path: PathBuf,
}

/// Replaces `destination` with a copy of the crate folder at `crate_root`.
///
/// The copy skips these entries:
///
/// - hidden files and folders,
/// - each `target/` folder beside a `Cargo.toml`,
/// - the `dist/` folder at the crate root,
/// - each `skipped` folder.
///
/// A path dependency outside the copy gets an absolute path, so the
/// gem builds only on a machine that has that folder. The function returns
/// those dependencies.
///
/// The crate must have its own `Cargo.lock`, so a workspace member fails.
pub(super) fn copy(
    crate_root: &Path,
    destination: &Path,
    skipped: &[&Path],
) -> Result<Vec<ExternalPathDependency>> {
    let crate_root = canonical(crate_root)?;
    if !crate_root.join("Cargo.lock").is_file() {
        return Err(CliError::CommandFailed {
            command: format!(
                "no Cargo.lock in {}; pack ruby copies one crate folder and does not support workspace members yet",
                crate_root.display()
            ),
            status: None,
        });
    }
    let skipped: Vec<PathBuf> = skipped
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .chain([crate_root.join("dist")])
        .collect();

    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|source| CliError::WriteFailed {
            path: destination.to_path_buf(),
            source,
        })?;
    }
    let manifests = copy_files(&crate_root, destination, &skipped)?;
    let destination = canonical(destination)?;
    manifests
        .iter()
        .try_fold(Vec::new(), |mut external, manifest| {
            external.extend(rewrite_manifest(&crate_root, &destination, manifest)?);
            Ok(external)
        })
}

/// Copies the files and returns the copied manifests, relative to the root.
fn copy_files(crate_root: &Path, destination: &Path, skipped: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut manifests = Vec::new();
    let entries = WalkDir::new(crate_root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_skipped(entry, skipped));
    for entry in entries {
        let entry = entry.map_err(|error| CliError::CommandFailed {
            command: format!("walk the crate folder {}: {error}", crate_root.display()),
            status: None,
        })?;
        let canonical_entry = canonical(entry.path())?;
        if !canonical_entry.starts_with(crate_root) {
            return Err(CliError::CommandFailed {
                command: format!(
                    "symlink {} points outside the crate folder {}",
                    entry.path().display(),
                    crate_root.display()
                ),
                status: None,
            });
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(crate_root)
            .expect("the walk stays inside the crate root");
        let target = destination.join(relative);
        let parent = target.parent().expect("a copied file has a parent folder");
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
            path: parent.to_path_buf(),
            source,
        })?;
        fs::copy(entry.path(), &target).map_err(|source| CliError::CopyFailed {
            from: entry.path().to_path_buf(),
            to: target.clone(),
            source,
        })?;
        if entry.file_name() == "Cargo.toml" {
            manifests.push(relative.to_path_buf());
        }
    }
    Ok(manifests)
}

fn is_skipped(entry: &DirEntry, skipped: &[PathBuf]) -> bool {
    let name = entry.file_name().to_string_lossy();
    let crate_target = entry.file_type().is_dir()
        && name == "target"
        && entry
            .path()
            .parent()
            .is_some_and(|parent| parent.join("Cargo.toml").is_file());
    name.starts_with('.') || crate_target || skipped.iter().any(|path| entry.path() == path)
}

/// Prepares one copied manifest for the gem build.
///
/// Each path dependency outside the copy gets an absolute path. A relative
/// path stays when it reaches the copied dependency from the copied
/// manifest. The root manifest also gets a `[workspace]` table so cargo
/// builds the copy as its own workspace.
fn rewrite_manifest(
    crate_root: &Path,
    destination: &Path,
    manifest: &Path,
) -> Result<Vec<ExternalPathDependency>> {
    let copied_manifest = destination.join(manifest);
    let original_folder = crate_root
        .join(manifest)
        .parent()
        .expect("a manifest has a parent folder")
        .to_path_buf();
    let copied_folder = copied_manifest
        .parent()
        .expect("a manifest has a parent folder")
        .to_path_buf();
    let text = fs::read_to_string(&copied_manifest).map_err(|source| CliError::ReadFailed {
        path: copied_manifest.clone(),
        source,
    })?;
    let mut document: DocumentMut = text.parse().map_err(|error| CliError::CommandFailed {
        command: format!("parse {}: {error}", copied_manifest.display()),
        status: None,
    })?;
    if manifest == Path::new("Cargo.toml") && !document.contains_key("workspace") {
        document.insert("workspace", Item::Table(Table::new()));
    }

    let mut external = Vec::new();
    for dependencies in dependency_tables(document.as_table_mut()) {
        for (name, dependency) in dependencies.iter_mut() {
            let Some(path) = dependency
                .as_table_like_mut()
                .and_then(|dependency| dependency.get_mut("path"))
                .and_then(Item::as_value_mut)
            else {
                continue;
            };
            let Some(relative) = path.as_str() else {
                continue;
            };
            let original = canonical(&original_folder.join(relative))?;
            let copied = original
                .strip_prefix(crate_root)
                .ok()
                .map(|inside| destination.join(inside))
                .filter(|copied| copied.join("Cargo.toml").is_file());
            if copied.is_some() && copied_folder.join(relative).canonicalize().ok() == copied {
                continue;
            }
            let absolute = original.to_str().ok_or_else(|| CliError::CommandFailed {
                command: format!("path dependency {} is not UTF-8", original.display()),
                status: None,
            })?;
            let decor = path.decor().clone();
            *path = Value::from(absolute);
            *path.decor_mut() = decor;
            external.push(ExternalPathDependency {
                manifest: manifest.to_path_buf(),
                name: name.get().to_owned(),
                path: original,
            });
        }
    }

    let rewritten = document.to_string();
    if rewritten != text {
        fs::write(
            &copied_manifest,
            format!("{REWRITTEN_MANIFEST_HEADER}{rewritten}"),
        )
        .map_err(|source| CliError::WriteFailed {
            path: copied_manifest,
            source,
        })?;
    }
    Ok(external)
}

/// The tables that can declare path dependencies: the dependency tables at
/// the top level and under `[target.<cfg>]`, `[workspace.dependencies]`, and
/// each `[patch.<source>]`.
fn dependency_tables(manifest: &mut dyn TableLike) -> Vec<&mut dyn TableLike> {
    manifest
        .iter_mut()
        .flat_map(|(key, item)| -> Vec<&mut dyn TableLike> {
            let Some(table) = item.as_table_like_mut() else {
                return Vec::new();
            };
            match key.get() {
                key if DEPENDENCY_TABLES.contains(&key) => vec![table],
                "target" => table
                    .iter_mut()
                    .filter_map(|(_, platform)| platform.as_table_like_mut())
                    .flat_map(|platform| {
                        platform
                            .iter_mut()
                            .filter(|(key, _)| DEPENDENCY_TABLES.contains(&key.get()))
                            .filter_map(|(_, item)| item.as_table_like_mut())
                    })
                    .collect(),
                "workspace" => table
                    .get_mut("dependencies")
                    .and_then(Item::as_table_like_mut)
                    .into_iter()
                    .collect(),
                "patch" => table
                    .iter_mut()
                    .filter_map(|(_, source)| source.as_table_like_mut())
                    .collect(),
                _ => Vec::new(),
            }
        })
        .collect()
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|source| CliError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn files(root: &Path) -> Vec<String> {
        let mut files: Vec<String> = WalkDir::new(root)
            .into_iter()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        files.sort();
        files
    }

    fn dependency_path(manifest: &Path, table: &str, name: &str) -> String {
        let manifest: toml::Table = toml::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
        table
            .split('.')
            .fold(&manifest, |table, key| table[key].as_table().unwrap())[name]["path"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn copies_the_crate_folder_and_gives_outside_path_dependencies_absolute_paths() {
        let root = tempfile::tempdir().unwrap();
        let crate_root = root.path().join("demo");
        let outside = root.path().join("outside");
        write(
            &crate_root.join("Cargo.toml"),
            r#"
[package]
name = "demo"

[dependencies]
inner = { path = "inner" }
# The demo needs the x feature.
outside = { path = "../outside", features = ["x"] }

[dev-dependencies.outside]
path = "../outside"

[target.'cfg(unix)'.build-dependencies]
outside = { path = "../outside" }

[workspace.dependencies]
shared = { path = "../outside" }
"#,
        );
        write(&crate_root.join("Cargo.lock"), "version = 4\n");
        write(&crate_root.join("src/lib.rs"), "");
        write(&crate_root.join("src/target/mod.rs"), "");
        write(
            &crate_root.join("inner/Cargo.toml"),
            "[package]\nname = \"inner\"\n\n[dependencies]\noutside = { path = \"../../outside\" }\n",
        );
        write(&crate_root.join("inner/src/lib.rs"), "");
        write(&crate_root.join("target/release/libdemo.a"), "");
        write(&crate_root.join("inner/target/debug/junk"), "");
        write(&crate_root.join("dist/apple/Demo.xcframework"), "");
        write(&crate_root.join("ruby/demo.gemspec"), "");
        write(&crate_root.join(".git/config"), "");
        write(
            &outside.join("Cargo.toml"),
            "[package]\nname = \"outside\"\n",
        );
        let destination = crate_root.join("ruby/ext/demo/crate");
        write(&destination.join("stale.rs"), "");

        let external = copy(&crate_root, &destination, &[&crate_root.join("ruby")]).unwrap();

        assert_eq!(
            files(&destination),
            [
                "Cargo.lock",
                "Cargo.toml",
                "inner/Cargo.toml",
                "inner/src/lib.rs",
                "src/lib.rs",
                "src/target/mod.rs",
            ]
        );
        let outside = outside.canonicalize().unwrap();
        let root_manifest = destination.join("Cargo.toml");
        assert_eq!(
            dependency_path(&root_manifest, "dependencies", "inner"),
            "inner"
        );
        assert_eq!(
            PathBuf::from(dependency_path(&root_manifest, "dependencies", "outside")),
            outside
        );
        assert_eq!(
            PathBuf::from(dependency_path(
                &root_manifest,
                "target.cfg(unix).build-dependencies",
                "outside"
            )),
            outside
        );
        assert_eq!(
            PathBuf::from(dependency_path(
                &root_manifest,
                "dev-dependencies",
                "outside"
            )),
            outside
        );
        assert_eq!(
            PathBuf::from(dependency_path(
                &root_manifest,
                "workspace.dependencies",
                "shared"
            )),
            outside
        );
        let root_text = fs::read_to_string(&root_manifest).unwrap();
        assert!(
            root_text.starts_with(REWRITTEN_MANIFEST_HEADER),
            "{root_text}"
        );
        assert!(
            root_text.contains(&format!(
                "# The demo needs the x feature.\noutside = {{ path = \"{}\", features = [\"x\"] }}\n",
                outside.display()
            )),
            "{root_text}"
        );
        assert_eq!(
            PathBuf::from(dependency_path(
                &destination.join("inner/Cargo.toml"),
                "dependencies",
                "outside"
            )),
            outside
        );
        let mut reported: Vec<_> = external
            .iter()
            .map(|dependency| {
                assert_eq!(dependency.path, outside);
                (
                    dependency.manifest.to_string_lossy().replace('\\', "/"),
                    dependency.name.as_str(),
                )
            })
            .collect();
        reported.sort();
        assert_eq!(
            reported,
            [
                ("Cargo.toml".to_owned(), "outside"),
                ("Cargo.toml".to_owned(), "outside"),
                ("Cargo.toml".to_owned(), "outside"),
                ("Cargo.toml".to_owned(), "shared"),
                ("inner/Cargo.toml".to_owned(), "outside"),
            ]
        );
    }

    #[test]
    fn a_gem_output_inside_the_crate_folder_is_not_copied_into_itself() {
        let root = tempfile::tempdir().unwrap();
        let crate_root = root.path();
        write(
            &crate_root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        );
        write(&crate_root.join("Cargo.lock"), "");
        write(&crate_root.join("src/lib.rs"), "");
        write(&crate_root.join("ext/demo/extconf.rb"), "");
        let extension = crate_root.join("ext/demo");

        copy(
            crate_root,
            &extension.join("crate"),
            &[crate_root, &extension],
        )
        .unwrap();

        assert_eq!(
            files(&extension.join("crate")),
            ["Cargo.lock", "Cargo.toml", "src/lib.rs"]
        );
    }

    #[test]
    fn a_crate_without_its_own_lockfile_cannot_be_copied() {
        let root = tempfile::tempdir().unwrap();
        write(
            &root.path().join("member/Cargo.toml"),
            "[package]\nname = \"member\"\n",
        );

        let error = copy(
            &root.path().join("member"),
            &root.path().join("gem/crate"),
            &[],
        )
        .err()
        .expect("a crate without Cargo.lock fails");

        assert!(error.to_string().contains("Cargo.lock"), "{error}");
        assert!(!root.path().join("gem/crate").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_outside_the_crate_folder_cannot_be_copied() {
        let root = tempfile::tempdir().unwrap();
        let crate_root = root.path().join("demo");
        let secret = root.path().join("secret.txt");
        write(&secret, "secret");
        write(
            &crate_root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        );
        write(&crate_root.join("Cargo.lock"), "version = 4\n");
        write(&crate_root.join("src/lib.rs"), "");
        std::os::unix::fs::symlink(&secret, crate_root.join("secret.txt")).unwrap();

        let error = copy(&crate_root, &root.path().join("gem/crate"), &[])
            .err()
            .expect("an external symlink fails");

        assert!(
            error
                .to_string()
                .contains("points outside the crate folder"),
            "{error}"
        );
    }
}
