//! Copies the Rust crate folder into the gem.
//!
//! The gem builds the crate when it installs, so the gem carries the crate
//! folder and its `Cargo.lock`. The copy keeps the folder layout, so a path
//! dependency inside the crate folder keeps its relative path.
//!
//! The file list comes from `cargo package --list --allow-dirty` for the
//! crate and for each path dependency inside the crate folder. The list
//! follows `package.include`, `package.exclude`, and `.gitignore`, so the
//! gem ships the files that `cargo package` ships.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use toml_edit::{DocumentMut, Item, Table, TableLike, Value};
#[cfg(test)]
use walkdir::WalkDir;

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
/// The copy uses the `cargo package --list --allow-dirty` file list of the
/// crate and of each path dependency inside the crate folder. It skips each
/// `skipped` folder and the `dist/` folder at the crate root. It also copies
/// the nearest `rust-toolchain.toml` or `rust-toolchain` file and the
/// nearest `.cargo/config.toml` or `.cargo/config` file from the crate
/// folder and its parent folders. The install build then uses the same
/// toolchain and Cargo config as the pack build.
///
/// A symlink in the file list gets the content of its target file, as
/// `cargo package` does.
///
/// A path dependency outside the copy gets an absolute path, so the gem
/// builds only on a machine that has that folder. The copy fails when
/// `allow_external` is false. The function returns those dependencies.
///
/// The crate must have its own `Cargo.lock`, so a workspace member fails.
/// The function reads the full file list before it changes `destination`.
pub(super) fn copy(
    crate_root: &Path,
    destination: &Path,
    skipped: &[&Path],
    toolchain: Option<&str>,
    allow_external: bool,
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
    let files = package_files(&crate_root, &skipped, toolchain)?;

    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|source| CliError::WriteFailed {
            path: destination.to_path_buf(),
            source,
        })?;
    }
    let manifests = copy_files(&crate_root, destination, &files)?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    copy_build_settings(&crate_root, destination, home.as_deref())?;
    let destination = canonical(destination)?;
    let mut external: Vec<ExternalPathDependency> = Vec::new();
    for manifest in &manifests {
        external.extend(rewrite_manifest(&crate_root, &destination, manifest)?);
    }
    if !external.is_empty() && !allow_external {
        let list = external
            .iter()
            .map(|dependency| {
                format!(
                    "{}: `{}` at {}",
                    dependency.manifest.display(),
                    dependency.name,
                    dependency.path.display()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CliError::CommandFailed {
            command: format!(
                "path dependencies outside the crate folder ({list}) make the gem build only on this machine and leak its absolute paths; move the dependencies inside the crate folder or pass --allow-external-path-dependencies"
            ),
            status: None,
        });
    }
    Ok(external)
}

/// The files that the copy carries, relative to the crate root.
///
/// The list is the union of the `cargo package` lists of the crate and of
/// each path dependency inside the crate folder. `cargo package` writes
/// `.cargo_vcs_info.json` and `Cargo.toml.orig` itself, so the list drops
/// those names. The list also drops files that do not exist, such as a
/// `Cargo.lock` that `cargo package` would write for a path dependency.
fn package_files(
    crate_root: &Path,
    skipped: &[PathBuf],
    toolchain: Option<&str>,
) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    let mut packages = vec![crate_root.to_path_buf()];
    let mut seen = BTreeSet::new();
    while let Some(package) = packages.pop() {
        if !seen.insert(package.clone()) {
            continue;
        }
        let prefix = package
            .strip_prefix(crate_root)
            .expect("a package stays inside the crate root")
            .to_path_buf();
        for listed in cargo_package_list(&package, toolchain)? {
            if listed
                .file_name()
                .is_some_and(|name| name == ".cargo_vcs_info.json" || name == "Cargo.toml.orig")
            {
                continue;
            }
            let relative = prefix.join(&listed);
            if is_skipped_path(crate_root, &relative, skipped)
                || !crate_root.join(&relative).is_file()
            {
                continue;
            }
            files.insert(relative);
        }
        for dependency in path_dependency_folders(&package)? {
            if dependency.starts_with(crate_root) && dependency.join("Cargo.toml").is_file() {
                packages.push(dependency);
            }
        }
    }
    Ok(files)
}

/// The file list of one package, relative to the package folder.
///
/// The rustup toolchain selector, such as `+nightly`, goes before
/// `package`, as for every other cargo call of the pack.
fn cargo_package_list(package: &Path, toolchain: Option<&str>) -> Result<Vec<PathBuf>> {
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(toolchain)
        .args(["package", "--list", "--allow-dirty", "--manifest-path"])
        .arg(package.join("Cargo.toml"))
        .output()
        .map_err(|source| CliError::ReadFailed {
            path: package.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: format!(
                "cargo package --list for {}: {}",
                package.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            status: output.status.code(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .collect())
}

/// The folders of the path dependencies that one manifest declares.
fn path_dependency_folders(package: &Path) -> Result<Vec<PathBuf>> {
    let (_, mut document) = read_manifest(&package.join("Cargo.toml"))?;
    let mut folders = Vec::new();
    for dependencies in dependency_tables(document.as_table_mut()) {
        for (_, dependency) in dependencies.iter() {
            if let Some(path) = dependency
                .as_table_like()
                .and_then(|table| table.get("path"))
                .and_then(Item::as_value)
                .and_then(Value::as_str)
            {
                folders.push(package.join(path));
            }
        }
    }
    folders
        .iter()
        .map(|folder| {
            folder
                .canonicalize()
                .map_err(|source| CliError::ReadFailed {
                    path: folder.clone(),
                    source,
                })
        })
        .collect()
}

fn is_skipped_path(crate_root: &Path, relative: &Path, skipped: &[PathBuf]) -> bool {
    let absolute = crate_root.join(relative);
    skipped.iter().any(|path| {
        path.as_path() != crate_root && (absolute == *path || absolute.starts_with(path))
    })
}

/// Copies the files and returns the copied manifests, relative to the root.
fn copy_files(
    crate_root: &Path,
    destination: &Path,
    files: &BTreeSet<PathBuf>,
) -> Result<Vec<PathBuf>> {
    let mut manifests = Vec::new();
    for relative in files {
        let source = crate_root.join(relative);
        let target = destination.join(relative);
        let parent = target.parent().expect("a copied file has a parent folder");
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
            path: parent.to_path_buf(),
            source,
        })?;
        fs::copy(&source, &target).map_err(|error| CliError::CopyFailed {
            from: source.clone(),
            to: target.clone(),
            source: error,
        })?;
        if relative
            .file_name()
            .is_some_and(|name| name == "Cargo.toml")
        {
            manifests.push(relative.clone());
        }
    }
    Ok(manifests)
}

/// Copies the toolchain file and the Cargo config that the pack build uses.
///
/// Each search starts at the crate folder and goes to its parent folders.
/// The first file wins. A file that the copy already carries stays. The
/// search stops after the repo root, the first parent folder with a `.git`
/// entry. A search with no repo root stops before the home folder. Settings
/// above the repo root and settings in the home folder belong to the user
/// or to a shared parent folder, so the gem must not ship them.
fn copy_build_settings(crate_root: &Path, destination: &Path, home: Option<&Path>) -> Result<()> {
    for names in [
        ["rust-toolchain.toml", "rust-toolchain"],
        [".cargo/config.toml", ".cargo/config"],
    ] {
        if names.iter().any(|name| destination.join(name).exists()) {
            continue;
        }
        let Some((source, name)) = nearest_setting(crate_root, &names, home) else {
            continue;
        };
        let target = destination.join(name);
        let parent = target.parent().expect("a setting file has a parent folder");
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
            path: parent.to_path_buf(),
            source,
        })?;
        fs::copy(&source, &target).map_err(|error| CliError::CopyFailed {
            from: source.clone(),
            to: target.clone(),
            source: error,
        })?;
    }
    Ok(())
}

/// The nearest setting file with one of `names`, and the name it matched.
fn nearest_setting<'name>(
    crate_root: &Path,
    names: &[&'name str],
    home: Option<&Path>,
) -> Option<(PathBuf, &'name str)> {
    for ancestor in crate_root.ancestors() {
        if home == Some(ancestor) {
            return None;
        }
        let found = names
            .iter()
            .map(|name| (ancestor.join(name), *name))
            .find(|(source, _)| source.is_file());
        if found.is_some() || ancestor.join(".git").exists() {
            return found;
        }
    }
    None
}

/// Reads and parses one manifest.
fn read_manifest(manifest: &Path) -> Result<(String, DocumentMut)> {
    let text = fs::read_to_string(manifest).map_err(|source| CliError::ReadFailed {
        path: manifest.to_path_buf(),
        source,
    })?;
    let document = text.parse().map_err(|error| CliError::CommandFailed {
        command: format!("parse {}: {error}", manifest.display()),
        status: None,
    })?;
    Ok((text, document))
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
    let (text, mut document) = read_manifest(&copied_manifest)?;
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
            "[package]\nname = \"outside\"\n\n[features]\nx = []\n",
        );
        write(&outside.join("src/lib.rs"), "");
        let destination = crate_root.join("ruby/ext/demo/crate");
        write(&destination.join("stale.rs"), "");

        let external = copy(
            &crate_root,
            &destination,
            &[&crate_root.join("ruby")],
            None,
            true,
        )
        .unwrap();

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
            None,
            false,
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
            None,
            false,
        )
        .err()
        .expect("a crate without Cargo.lock fails");

        assert!(error.to_string().contains("Cargo.lock"), "{error}");
        assert!(!root.path().join("gem/crate").exists());
    }

    #[test]
    fn an_outside_path_dependency_fails_without_the_opt_in() {
        let root = tempfile::tempdir().unwrap();
        let crate_root = root.path().join("demo");
        write(
            &crate_root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n\n[dependencies]\noutside = { path = \"../outside\" }\n",
        );
        write(&crate_root.join("Cargo.lock"), "version = 4\n");
        write(&crate_root.join("src/lib.rs"), "");
        write(
            &root.path().join("outside/Cargo.toml"),
            "[package]\nname = \"outside\"\n",
        );
        write(&root.path().join("outside/src/lib.rs"), "");

        let error = copy(
            &crate_root,
            &root.path().join("gem/crate"),
            &[],
            None,
            false,
        )
        .err()
        .expect("an outside path dependency fails");

        assert!(
            error
                .to_string()
                .contains("--allow-external-path-dependencies"),
            "{error}"
        );
    }

    #[test]
    fn the_copy_uses_the_package_list_and_copies_build_settings() {
        let root = tempfile::tempdir().unwrap();
        let crate_root = root.path().join("demo");
        write(
            &crate_root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nexclude = [\"local.txt\"]\n",
        );
        write(&crate_root.join("Cargo.lock"), "version = 4\n");
        write(&crate_root.join("src/lib.rs"), "");
        write(&crate_root.join("local.txt"), "local");
        write(
            &crate_root.join(".cargo/config.toml"),
            "[build]\nrustflags = [\"--cfg\", \"demo\"]\n",
        );
        write(
            &root.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.97.1\"\n",
        );

        copy(
            &crate_root,
            &root.path().join("gem/crate"),
            &[],
            None,
            false,
        )
        .unwrap();

        assert_eq!(
            files(&root.path().join("gem/crate")),
            [
                ".cargo/config.toml",
                "Cargo.lock",
                "Cargo.toml",
                "rust-toolchain.toml",
                "src/lib.rs"
            ]
        );
    }

    #[test]
    fn build_settings_stop_at_the_repo_root() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let crate_root = repo.join("crates/demo");
        fs::create_dir_all(crate_root.join("src")).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        write(
            &root.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.97.1\"\n",
        );
        write(
            &repo.join(".cargo/config.toml"),
            "[build]\nrustflags = [\"--cfg\", \"demo\"]\n",
        );
        let destination = root.path().join("gem/crate");

        copy_build_settings(&crate_root, &destination, None).unwrap();

        assert!(destination.join(".cargo/config.toml").is_file());
        assert!(!destination.join("rust-toolchain.toml").exists());
    }

    #[test]
    fn build_settings_stop_before_the_home_folder() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let repo = home.join("repo");
        let crate_root = repo.join("crates/demo");
        fs::create_dir_all(crate_root.join("src")).unwrap();
        write(
            &home.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.97.1\"\n",
        );
        write(
            &home.join(".cargo/config.toml"),
            "[registries.example]\nindex = \"sparse+https://example.invalid/\"\n",
        );
        write(
            &repo.join(".cargo/config.toml"),
            "[build]\nrustflags = [\"--cfg\", \"demo\"]\n",
        );
        let destination = root.path().join("gem/crate");

        copy_build_settings(&crate_root, &destination, Some(home)).unwrap();

        assert!(destination.join(".cargo/config.toml").is_file());
        assert_eq!(
            fs::read_to_string(destination.join(".cargo/config.toml")).unwrap(),
            "[build]\nrustflags = [\"--cfg\", \"demo\"]\n"
        );
        assert!(!destination.join("rust-toolchain.toml").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_gets_the_content_of_its_target_file() {
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

        copy(
            &crate_root,
            &root.path().join("gem/crate"),
            &[],
            None,
            false,
        )
        .unwrap();

        let copied = root.path().join("gem/crate/secret.txt");
        assert_eq!(fs::read_to_string(&copied).unwrap(), "secret");
        assert!(!copied.is_symlink());
    }
}
