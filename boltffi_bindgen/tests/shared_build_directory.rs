use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use boltffi_bindgen::generate::Generation;
use boltffi_bindgen::target::Target;

const DECLARED_NAMES: &str = r#"
boltffi::scaffolding!();

pub mod geo {
    #[boltffi::data]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }
}

#[boltffi::export]
pub fn echo(point: geo::Point) -> geo::Point {
    point
}
"#;

const ALIASES: &str = r#"
boltffi::scaffolding!();

pub mod geo {
    #[boltffi::data]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }
}

type Coordinate = geo::Point;

#[boltffi::export]
pub fn echo(point: Coordinate) -> Coordinate {
    point
}
"#;

struct Project {
    root: PathBuf,
    build_directory: PathBuf,
}

impl Project {
    fn write(name: &str, source: &str, build_directory: &Path) -> Self {
        let root = std::env::temp_dir().join(format!(
            "boltffi-shared-build-directory-{name}-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale project");
        }
        let boltffi = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("boltffi")
            .to_string_lossy()
            .replace('\\', "/");
        fs::create_dir_all(root.join("src")).expect("create project");
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\nboltffi = {{ path = \"{boltffi}\" }}\n\n[workspace]\n"
            ),
        )
        .expect("write manifest");
        fs::write(root.join("src/lib.rs"), source).expect("write source");
        Self {
            root,
            build_directory: build_directory.to_path_buf(),
        }
    }

    fn generate(&self) {
        Generation::new(self.root.join("Cargo.toml"))
            .cargo_environment([
                ("CARGO_TARGET_DIR", &self.build_directory),
                ("CARGO_BUILD_BUILD_DIR", &self.build_directory),
            ])
            .render(Target::C)
            .expect("generation succeeds");
    }

    fn build(&self) -> Output {
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "--quiet", "--lib", "--manifest-path"])
            .arg(self.root.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", &self.build_directory)
            .env("CARGO_BUILD_BUILD_DIR", &self.build_directory)
            .env_remove("BOLTFFI_SOURCE_RECORDS")
            .output()
            .expect("cargo runs")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn shared_build_directory(label: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "boltffi-shared-build-directory-{label}-{}",
        std::process::id()
    ));
    if directory.exists() {
        fs::remove_dir_all(&directory).expect("remove stale build directory");
    }
    directory
}

fn assert_builds(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_project_ignores_the_records_another_project_generated() {
    let build_directory = shared_build_directory("ignores");
    let first = Project::write("first", DECLARED_NAMES, &build_directory);
    let second = Project::write("second", DECLARED_NAMES, &build_directory);

    first.generate();
    let built = second.build();

    assert_builds(&built);
    let _ = fs::remove_dir_all(&build_directory);
}

#[test]
fn projects_sharing_a_build_directory_keep_their_own_records() {
    let build_directory = shared_build_directory("keeps");
    let first = Project::write("north", ALIASES, &build_directory);
    let second = Project::write("south", ALIASES, &build_directory);

    first.generate();
    second.generate();
    let first_built = first.build();
    let second_built = second.build();

    assert_builds(&first_built);
    assert_builds(&second_built);
    let _ = fs::remove_dir_all(&build_directory);
}
