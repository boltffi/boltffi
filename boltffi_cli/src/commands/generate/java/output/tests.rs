use std::{fs, path::Path};

#[cfg(unix)]
use std::os::unix::fs::symlink;

use boltffi_backend::{FilePath, GeneratedFile, GeneratedOutput};

use super::{
    LIVE, LOCK, MANIFEST, Output, PREPARE, TOMBSTONE,
    journal::Journal,
    ownership::Manifest,
    path::{Directory, ManagedPath},
    transaction::{OutputLock, Staging, Transaction},
};

fn generated(files: &[(&str, &str)]) -> GeneratedOutput {
    GeneratedOutput::new(
        files
            .iter()
            .map(|(path, contents)| GeneratedFile::new(FilePath::new(path).unwrap(), *contents))
            .collect(),
        Vec::new(),
    )
}

fn output(root: &Path) -> Output {
    Output::new(root, "com.example").unwrap()
}

fn stage<'root>(
    directory: &'root Directory,
    output: &Output,
    next_output: &GeneratedOutput,
) -> Staging<'root> {
    let previous = Manifest::load_owned(directory, &output.roots).unwrap();
    let next = Manifest::from_output(next_output, &output.roots).unwrap();
    let staging = Staging::new(directory, previous, next).unwrap();
    next_output.files().iter().for_each(|file| {
        staging
            .stage(
                &ManagedPath::from_path(file.path().as_path()).unwrap(),
                file.contents().as_bytes(),
            )
            .unwrap();
    });
    staging.stage_manifest().unwrap();
    staging
}

#[test]
fn rejects_duplicate_generated_paths_before_touching_the_output() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let result = output(&root).write(generated(&[
        ("com/example/Demo.java", "first"),
        ("com/example/Demo.java", "second"),
    ]));

    assert!(matches!(
        result,
        Err(crate::cli::CliError::CommandFailed { command, status: None })
            if command.contains("duplicate output path 'com/example/Demo.java'")
    ));
    assert!(!root.exists());
}

#[test]
fn rejects_duplicate_paths_in_the_persisted_manifest() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(MANIFEST),
        r#"{"version":1,"files":["com/example/Demo.java","com/example/Demo.java"]}"#,
    )
    .unwrap();

    let result = output(&root).write(generated(&[("com/example/Demo.java", "generated")]));

    assert!(matches!(
        result,
        Err(crate::cli::CliError::CommandFailed { command, status: None })
            if command.contains("contains duplicate path 'com/example/Demo.java'")
    ));
    assert!(!root.join("com/example/Demo.java").exists());
}

#[test]
fn refuses_unmanifested_source_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let handwritten = root.join("com/example/Demo.java");
    fs::create_dir_all(handwritten.parent().unwrap()).unwrap();
    fs::write(&handwritten, "handwritten").unwrap();

    let result = output(&root).write(generated(&[("com/example/Demo.java", "generated")]));

    assert!(matches!(
        result,
        Err(crate::cli::CliError::CommandFailed { command, status: None })
            if command.contains("without manifest ownership")
                && command.contains("Demo.java")
    ));
    assert_eq!(fs::read_to_string(handwritten).unwrap(), "handwritten");
    assert!(!root.join(MANIFEST).exists());
}

#[test]
fn refuses_unclaimed_sources_after_manifest_creation() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "owned")]))
        .unwrap();
    let handwritten = root.join("com/example/Handwritten.java");
    fs::write(&handwritten, "handwritten").unwrap();

    let result = output.write(generated(&[("com/example/Demo.java", "replacement")]));

    assert!(matches!(
        result,
        Err(crate::cli::CliError::CommandFailed { command, status: None })
            if command.contains("without manifest ownership")
                && command.contains("Handwritten.java")
    ));
    assert_eq!(fs::read_to_string(handwritten).unwrap(), "handwritten");
    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "owned"
    );
}

#[test]
fn removes_stale_owned_sources_and_preserves_unowned_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[
            ("com/example/Demo.java", "old"),
            ("com/example/Stale.java", "stale"),
            ("jni/demo.h", "old header"),
            ("jni/stale.c", "stale native"),
        ]))
        .unwrap();
    fs::write(root.join("com/example/README.md"), "notes").unwrap();
    fs::create_dir_all(root.join("native")).unwrap();
    fs::write(root.join("native/library.so"), "native").unwrap();

    output
        .write(generated(&[
            ("com/example/Demo.java", "new"),
            ("jni/demo.h", "new header"),
        ]))
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "new"
    );
    assert_eq!(
        fs::read_to_string(root.join("jni/demo.h")).unwrap(),
        "new header"
    );
    assert!(!root.join("com/example/Stale.java").exists());
    assert!(!root.join("jni/stale.c").exists());
    assert_eq!(
        fs::read_to_string(root.join("com/example/README.md")).unwrap(),
        "notes"
    );
    assert_eq!(
        fs::read_to_string(root.join("native/library.so")).unwrap(),
        "native"
    );
}

#[cfg(unix)]
#[test]
fn refuses_a_symlinked_managed_parent_without_touching_external_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "owned")]))
        .unwrap();
    let external = temporary.path().join("external/example");
    fs::create_dir_all(&external).unwrap();
    let external_source = external.join("Demo.java");
    fs::write(&external_source, "external").unwrap();
    fs::rename(root.join("com"), root.join("owned-com")).unwrap();
    symlink(temporary.path().join("external"), root.join("com")).unwrap();

    let result = output.write(generated(&[("com/example/Demo.java", "replacement")]));

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(external_source).unwrap(), "external");
    assert!(root.join(MANIFEST).is_file());
}

#[cfg(unix)]
#[test]
fn refuses_a_symlinked_lock_without_touching_its_target() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let external = temporary.path().join("external.lock");
    fs::create_dir_all(&root).unwrap();
    fs::write(&external, "external").unwrap();
    symlink(&external, root.join(LOCK)).unwrap();

    let result = output(&root).write(generated(&[("com/example/Demo.java", "generated")]));

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(external).unwrap(), "external");
    assert!(!root.join("com/example/Demo.java").exists());
}

#[test]
fn output_lock_prevents_a_second_writer_from_entering_recovery() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let directory = Directory::open_root(&root).unwrap();
    let output_lock = OutputLock::acquire(&directory).unwrap();

    let result = output(&root).write(generated(&[("com/example/Demo.java", "generated")]));

    assert!(matches!(
        result,
        Err(crate::cli::CliError::CommandFailed { command, status: None })
            if command.contains("locked by another generator")
    ));
    drop(output_lock);
}

#[test]
fn rollback_removes_new_directories_after_restoring_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "previous")]))
        .unwrap();
    let directory = Directory::open_root(&root).unwrap();
    let next = generated(&[("com/example/replacement/Next.java", "interrupted")]);
    let mut transaction = stage(&directory, &output, &next).activate().unwrap();
    transaction.apply().unwrap();
    drop(transaction);

    Transaction::recover(&directory).unwrap();

    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "previous"
    );
    assert!(!root.join("com/example/replacement").exists());
    assert!(!root.join(LIVE).exists());
}

#[test]
fn removes_an_orphaned_prepare_state_before_writing() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    let directory = Directory::open_root(&root).unwrap();
    let next = generated(&[("com/example/Interrupted.java", "interrupted")]);
    let staging = stage(&directory, &output, &next);
    drop(staging);
    assert!(root.join(PREPARE).is_dir());

    output
        .write(generated(&[("com/example/Demo.java", "final")]))
        .unwrap();

    assert!(!root.join(PREPARE).exists());
    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "final"
    );
}

#[test]
fn recovers_an_interrupted_active_transaction() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "previous")]))
        .unwrap();
    let directory = Directory::open_root(&root).unwrap();
    let interrupted = generated(&[("com/example/Demo.java", "interrupted")]);
    let mut transaction = stage(&directory, &output, &interrupted).activate().unwrap();
    transaction.apply().unwrap();
    drop(transaction);

    output
        .write(generated(&[("com/example/Demo.java", "final")]))
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "final"
    );
    assert!(!root.join(LIVE).exists());
}

#[test]
fn cleans_an_interrupted_restored_transaction() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "previous")]))
        .unwrap();
    let directory = Directory::open_root(&root).unwrap();
    let interrupted = generated(&[("com/example/Demo.java", "interrupted")]);
    let transaction = stage(&directory, &output, &interrupted).activate().unwrap();
    drop(transaction);
    directory
        .require_child(LIVE)
        .unwrap()
        .write_new("restored", &[])
        .unwrap();

    output
        .write(generated(&[("com/example/Demo.java", "final")]))
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "final"
    );
    assert!(!root.join(LIVE).exists());
}

#[test]
fn cleans_an_interrupted_committed_transaction() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "previous")]))
        .unwrap();
    let directory = Directory::open_root(&root).unwrap();
    let interrupted = generated(&[("com/example/Demo.java", "interrupted")]);
    let mut transaction = stage(&directory, &output, &interrupted).activate().unwrap();
    transaction.apply().unwrap();
    drop(transaction);
    directory
        .require_child(LIVE)
        .unwrap()
        .write_new("committed", &[])
        .unwrap();

    output
        .write(generated(&[("com/example/Demo.java", "final")]))
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "final"
    );
    assert!(!root.join(LIVE).exists());
}

#[test]
fn removes_an_interrupted_cleanup_tombstone() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let output = output(&root);
    output
        .write(generated(&[("com/example/Demo.java", "previous")]))
        .unwrap();
    let directory = Directory::open_root(&root).unwrap();
    let interrupted = generated(&[("com/example/Demo.java", "interrupted")]);
    let mut transaction = stage(&directory, &output, &interrupted).activate().unwrap();
    transaction.apply().unwrap();
    drop(transaction);
    directory
        .require_child(LIVE)
        .unwrap()
        .write_new("committed", &[])
        .unwrap();
    directory.rename_child(LIVE, TOMBSTONE).unwrap();

    output
        .write(generated(&[("com/example/Demo.java", "final")]))
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.join("com/example/Demo.java")).unwrap(),
        "final"
    );
    assert!(!root.join(TOMBSTONE).exists());
}

#[test]
fn preserves_corrupted_live_recovery_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("java");
    let directory = Directory::open_root(&root).unwrap();
    let live = directory.create_child(LIVE).unwrap();
    ["new", "backup", "restore"]
        .into_iter()
        .for_each(|name| drop(live.create_child(name).unwrap()));
    live.write_new(Journal::FILE, b"not json").unwrap();

    let result = output(&root).write(generated(&[("com/example/Demo.java", "final")]));

    assert!(result.is_err());
    assert!(root.join(LIVE).join(Journal::FILE).is_file());
    assert!(!root.join("com/example/Demo.java").exists());
}
