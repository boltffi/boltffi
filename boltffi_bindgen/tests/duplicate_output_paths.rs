use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use boltffi_backend::{FilePath, GeneratedFile, GeneratedOutput};
use boltffi_bindgen::generate::{Generation, GenerationError};

#[test]
fn write_output_rejects_duplicate_paths_before_writing_any_file() {
    let output_dir = TestDirectory::new();
    let existing_path = output_dir.path().join("existing.cs");
    fs::write(&existing_path, "original contents").expect("existing generated file");

    let output = GeneratedOutput::new(
        vec![
            GeneratedFile::new(
                FilePath::new("nested/first.cs").expect("file path"),
                "new generated file",
            ),
            GeneratedFile::new(
                FilePath::new("existing.cs").expect("file path"),
                "replacement",
            ),
            GeneratedFile::new(FilePath::new("Demo.cs").expect("file path"), "module class"),
            GeneratedFile::new(
                FilePath::new("Demo.cs").expect("file path"),
                "exported type",
            ),
        ],
        vec![],
    );

    let error = Generation::write_output(output, output_dir.path())
        .expect_err("duplicate generated paths should be rejected");
    assert!(
        matches!(
            &error,
            GenerationError::DuplicateOutputPath { path } if path == &PathBuf::from("Demo.cs")
        ),
        "unexpected generation error: {error}"
    );
    assert_eq!(
        fs::read_to_string(existing_path).expect("existing file should remain"),
        "original contents"
    );
    assert!(!output_dir.path().join("Demo.cs").exists());
    assert!(!output_dir.path().join("nested").exists());
}

#[test]
fn write_output_accepts_identical_filenames_in_different_directories() {
    let temporary_dir = TestDirectory::new();
    let output_dir = temporary_dir.path().join("generated");
    let output = GeneratedOutput::new(
        vec![
            GeneratedFile::new(FilePath::new("Demo.cs").expect("file path"), "module class"),
            GeneratedFile::new(
                FilePath::new("nested/Demo.cs").expect("file path"),
                "exported type",
            ),
        ],
        vec![],
    );

    let paths = Generation::write_output(output, &output_dir)
        .expect("distinct generated paths should be written");
    assert_eq!(
        paths,
        vec![
            output_dir.join("Demo.cs"),
            output_dir.join("nested/Demo.cs")
        ]
    );
    assert_eq!(
        fs::read_to_string(&paths[0]).expect("module file"),
        "module class"
    );
    assert_eq!(
        fs::read_to_string(&paths[1]).expect("type file"),
        "exported type"
    );
}

struct TestDirectory {
    root: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let nonce = UNIX_EPOCH.elapsed().expect("system clock").as_nanos();
        let root = std::env::temp_dir().join(format!(
            "boltffi-output-paths-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create temporary test directory");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
