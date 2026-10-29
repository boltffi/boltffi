use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use boltffi_bindgen::generate::Generation;
use boltffi_bindgen::target::Target;
use boltffi_binding::RecordStore;

const UNITS: &str = r#"
boltffi::scaffolding!();

pub mod si {
    pub mod f64 {
        #[derive(Clone, Copy, Debug)]
        pub struct Length(pub f64);

        #[derive(Clone, Copy, Debug)]
        pub struct Feet(pub f64);
    }
}

boltffi::custom_type!(
    pub FeetValue,
    remote = si::f64::Feet,
    repr = f64,
    into_ffi = |feet: &si::f64::Feet| feet.0,
    try_from_ffi = |feet: f64| Ok::<_, boltffi::CustomTypeConversionError>(si::f64::Feet(feet)),
);
"#;

const DECLARATIONS: &str = r#"
boltffi::scaffolding!();

pub mod units_ffi {
    boltffi::custom_type!(
        pub LengthMeters,
        remote = units::si::f64::Length,
        repr = f64,
        into_ffi = |length: &units::si::f64::Length| length.0,
        try_from_ffi = |meters: f64| Ok::<_, boltffi::CustomTypeConversionError>(units::si::f64::Length(meters)),
    );
}
"#;

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn write(label: &str, distance: &str, point_fields: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "boltffi-any-spelling-{label}-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale workspace");
        }
        let boltffi = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("boltffi")
            .to_string_lossy()
            .replace('\\', "/");
        let workspace = Self { root };
        workspace.file(
            "Cargo.toml",
            &format!(
                "[workspace]\nmembers = [\"app\", \"units\"]\nresolver = \"2\"\n\n[workspace.dependencies]\nboltffi = {{ path = \"{boltffi}\" }}\n"
            ),
        );
        workspace.file(
            "units/Cargo.toml",
            "[package]\nname = \"units\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nboltffi.workspace = true\n",
        );
        workspace.file("units/src/lib.rs", UNITS);
        workspace.file(
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\nboltffi.workspace = true\nunits = { path = \"../units\" }\n",
        );
        workspace.rewrite(distance, point_fields);
        workspace
    }

    fn rewrite(&self, distance: &str, point_fields: &str) {
        self.file(
            "app/src/lib.rs",
            &format!(
                r#"{DECLARATIONS}
pub mod geo {{
    #[boltffi::data]
    #[derive(Clone, Copy)]
    pub struct Point {{ {point_fields} }}
}}

pub mod route {{
    use units::si::f64::Length;

    type Distance = {distance};
    type Coordinate = crate::geo::Point;

    #[boltffi::export]
    pub fn stride(distance: Distance, start: Coordinate) -> units::si::f64::Length {{
        Length(distance.0 * 2.0 + start.x)
    }}

    #[boltffi::export]
    pub fn total(lengths: Vec<units::si::f64::Length>) -> Option<Length> {{
        lengths.into_iter().reduce(|total, leg| Length(total.0 + leg.0))
    }}
}}
"#
            ),
        );
    }

    fn file(&self, path: &str, contents: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().expect("fixture file parent")).expect("create directory");
        fs::write(path, contents).expect("write fixture file");
    }

    fn target(&self) -> PathBuf {
        self.root.join("target")
    }

    fn generate(&self) -> String {
        Generation::new(self.root.join("app/Cargo.toml"))
            .cargo_environment([("CARGO_TARGET_DIR", self.target())])
            .render(Target::C)
            .expect("generation resolves every spelling")
            .files()
            .iter()
            .map(|file| file.contents().to_owned())
            .collect()
    }

    fn build(&self) -> Output {
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .arg("build")
            .arg("--quiet")
            .arg("--lib")
            .arg("--manifest-path")
            .arg(self.root.join("app/Cargo.toml"))
            .env("CARGO_TARGET_DIR", self.target())
            .env_remove("BOLTFFI_SOURCE_RECORDS")
            .output()
            .expect("cargo runs")
    }

    fn library(&self) -> Vec<u8> {
        let name = format!(
            "{}app{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        );
        fs::read(self.target().join("debug").join(name)).expect("read the built library")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn exports(library: &[u8], symbol: &str) -> bool {
    library
        .windows(symbol.len())
        .any(|window| window == symbol.as_bytes())
}

#[test]
fn an_alias_reaches_no_lane_before_bindings_are_generated() {
    let workspace = Workspace::write("before", "Length", "pub x: f64, pub y: f64");

    let build = workspace.build();

    assert!(!build.status.success(), "lanes cannot follow an alias");
    assert!(
        String::from_utf8_lossy(&build.stderr).contains("Distance"),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

#[test]
fn generated_bindings_let_every_spelling_cross() {
    let workspace = Workspace::write("generated", "Length", "pub x: f64, pub y: f64");

    let header = workspace.generate();
    let build = workspace.build();

    assert!(
        workspace.root.join(RecordStore::RELATIVE_PATH).is_file(),
        "generation keeps the records for later builds"
    );
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let library = workspace.library();
    [
        "boltffi_function_3app6stride",
        "boltffi_function_3app5total",
    ]
    .into_iter()
    .for_each(|symbol| {
        assert!(header.contains(symbol), "`{symbol}` is declared");
        assert!(exports(&library, symbol), "`{symbol}` is exported");
    });
}

#[test]
fn an_alias_retargeted_to_a_custom_from_another_crate_fails_until_bindings_are_generated_again() {
    let workspace = Workspace::write("retargeted", "Length", "pub x: f64, pub y: f64");
    workspace.generate();

    workspace.rewrite("units::si::f64::Feet", "pub x: f64, pub y: f64");
    let stale = workspace.build();
    workspace.generate();
    let regenerated = workspace.build();

    assert!(
        !stale.status.success(),
        "the stored plan names the old type"
    );
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("names a different or changed declaration"),
        "{}",
        String::from_utf8_lossy(&stale.stderr)
    );
    assert!(
        regenerated.status.success(),
        "{}",
        String::from_utf8_lossy(&regenerated.stderr)
    );
}

#[test]
fn a_changed_record_fails_the_functions_expanded_from_its_old_shape() {
    let workspace = Workspace::write("changed-record", "Length", "pub x: f64, pub y: f64");
    workspace.generate();

    workspace.rewrite("Length", "pub x: f64, pub y: f64, pub label: String");
    let stale = workspace.build();

    assert!(
        !stale.status.success(),
        "the stored plan read the old record"
    );
    assert!(
        String::from_utf8_lossy(&stale.stderr)
            .contains("`Coordinate` names a different or changed declaration"),
        "{}",
        String::from_utf8_lossy(&stale.stderr)
    );
}
