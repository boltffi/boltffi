use std::fs;
use std::path::{Path, PathBuf};

use boltffi_bindgen::generate::{Generation, GenerationError};
use boltffi_bindgen::library_symbols::LibrarySymbolsError;
use boltffi_bindgen::target::Target;

const APP: &str = r#"
boltffi::scaffolding!();
use boltffi::*;

#[export]
pub fn norm(point: shapes::Point) -> f64 {
    (point.x * point.x + point.y * point.y).sqrt()
}

#[export]
pub fn make_widget() -> shapes::Widget {
    shapes::Widget::new()
}

#[export]
pub fn widget_n(widget: &shapes::Widget) -> u32 {
    widget.n()
}

#[export]
pub fn register(listener: Box<dyn shapes::Listener>) {
    listener.on_event(1)
}
"#;

const SHAPES: &str = r#"
boltffi::scaffolding!();
use boltffi::*;

#[data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn shapes_origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

pub struct Widget {
    pub n: u32,
}

#[export]
impl Widget {
    pub fn new() -> Widget {
        Widget { n: 1 }
    }

    pub fn n(&self) -> u32 {
        self.n
    }
}

#[export]
pub trait Listener {
    fn on_event(&self, n: u32);
}
"#;

const OTHER: &str = r#"
boltffi::scaffolding!();
use boltffi::*;

#[export]
pub fn other_hello() -> u32 {
    7
}

pub struct Gadget {
    pub n: u32,
}

#[export]
impl Gadget {
    pub fn new() -> Gadget {
        Gadget { n: 1 }
    }
}
"#;

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn write(label: &str, dependencies: &[&str], app_extra: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("boltffi-dependency-{label}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale dependency workspace");
        }
        let boltffi = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("boltffi")
            .to_string_lossy()
            .replace('\\', "/");
        let file = |path: &str, contents: &str| {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("fixture file parent"))
                .expect("create fixture directory");
            fs::write(path, contents).expect("write fixture file");
        };
        file(
            "Cargo.toml",
            &format!(
                "[workspace]\nmembers = [\"app\", \"shapes\", \"other\"]\nresolver = \"2\"\n\n[workspace.dependencies]\nboltffi = {{ path = \"{boltffi}\" }}\n"
            ),
        );
        let dependencies = dependencies
            .iter()
            .map(|name| format!("{name} = {{ path = \"../{name}\" }}\n"))
            .collect::<String>();
        file(
            "app/Cargo.toml",
            &format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\nboltffi.workspace = true\n{dependencies}"
            ),
        );
        file("app/src/lib.rs", &format!("{APP}{app_extra}"));
        ["shapes", "other"].into_iter().for_each(|name| {
            file(
                &format!("{name}/Cargo.toml"),
                &format!(
                    "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nboltffi.workspace = true\n"
                ),
            );
        });
        file("shapes/src/lib.rs", SHAPES);
        file("other/src/lib.rs", OTHER);
        Self { root }
    }

    fn target(&self) -> PathBuf {
        self.root.join("target")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn header(workspace: &Workspace) -> Result<String, GenerationError> {
    Generation::new(workspace.root.join("app/Cargo.toml"))
        .cargo_environment([("CARGO_TARGET_DIR", workspace.target())])
        .render(Target::C)
        .map(|output| {
            output
                .files()
                .iter()
                .map(|file| file.contents().to_owned())
                .collect()
        })
}

fn typescript(workspace: &Workspace, triple: Option<&str>) -> Result<String, GenerationError> {
    let generation = Generation::new(workspace.root.join("app/Cargo.toml"))
        .cargo_environment([("CARGO_TARGET_DIR", workspace.target())]);
    triple
        .into_iter()
        .fold(generation, |generation, triple| generation.triple(triple))
        .render(Target::TypeScript)
        .map(|output| {
            output
                .files()
                .iter()
                .map(|file| file.contents().to_owned())
                .collect()
        })
}

#[test]
fn bindings_carry_the_exports_of_linked_dependencies() {
    let workspace = Workspace::write("linked", &["shapes"], "");

    let header = header(&workspace).expect("linked dependency workspace generates C bindings");

    [
        "boltffi_function_app_norm",
        "boltffi_function_app_make_widget",
        "boltffi_function_app_widget_n",
        "boltffi_function_app_register",
        "boltffi_function_shapes_shapes_origin",
        "boltffi_init_class_shapes_widget_new",
        "boltffi_method_class_shapes_widget_n",
        "boltffi_register_callback_shapes_listener",
    ]
    .into_iter()
    .for_each(|symbol| assert!(header.contains(symbol), "{symbol} is linked and exported"));
}

#[test]
fn a_dependency_the_library_never_links_is_refused_by_name() {
    let workspace = Workspace::write("unlinked", &["shapes", "other"], "");

    let error = header(&workspace).expect_err("the unused crate's exports are not in the library");

    assert!(
        matches!(
            &error,
            GenerationError::LibrarySymbols(LibrarySymbolsError::Missing { missing, .. })
                if missing == &[
                    "boltffi_function_other_other_hello".to_owned(),
                    "boltffi_init_class_other_gadget_new".to_owned(),
                    "boltffi_release_class_other_gadget".to_owned(),
                ]
        ) && error.to_string().contains("`use <crate> as _;`"),
        "{error}"
    );
}

#[test]
fn naming_a_dependency_links_it_so_its_exports_bind() {
    let workspace = Workspace::write("used", &["shapes", "other"], "use other as _;\n");

    let header = header(&workspace).expect("a used dependency is linked and generates");

    [
        "boltffi_function_shapes_shapes_origin",
        "boltffi_function_other_other_hello",
        "boltffi_init_class_other_gadget_new",
    ]
    .into_iter()
    .for_each(|symbol| assert!(header.contains(symbol), "{symbol} is linked and exported"));
}

#[test]
fn a_root_type_named_like_a_dependency_type_is_refused_with_both_crates() {
    let workspace = Workspace::write(
        "collision",
        &["shapes"],
        "#[data]\n#[derive(Clone, Copy)]\npub struct Point {\n    pub z: f64,\n}\n\n#[export]\npub fn lift(point: Point) -> f64 {\n    point.z\n}\n",
    );

    let error = header(&workspace).expect_err("two records bind as Point");

    let message = error.to_string();
    assert!(
        matches!(&error, GenerationError::Lower(_))
            && message.contains("`app::Point`")
            && message.contains("`shapes::Point`")
            && message.contains("both bind as `Point`"),
        "{message}"
    );
}

fn refused_as_unlinked(error: &GenerationError, extension: &str) -> bool {
    matches!(
        error,
        GenerationError::LibrarySymbols(LibrarySymbolsError::Missing { path, missing })
            if path.extension().is_some_and(|found| found == extension)
                && missing == &[
                    "boltffi_function_other_other_hello".to_owned(),
                    "boltffi_init_class_other_gadget_new".to_owned(),
                    "boltffi_release_class_other_gadget".to_owned(),
                ]
    ) && error.to_string().contains("`use <crate> as _;`")
}

#[test]
fn typescript_bindings_refuse_a_dependency_the_host_library_never_links() {
    let linked = Workspace::write("typescript-linked", &["shapes"], "");
    let unlinked = Workspace::write("typescript-unlinked", &["shapes", "other"], "");

    typescript(&linked, None).expect("wasm-only exports are not required of the host library");
    let error = typescript(&unlinked, None).expect_err("the unused crate's exports are not linked");

    assert!(
        refused_as_unlinked(&error, std::env::consts::DLL_EXTENSION),
        "{error}"
    );
}

#[test]
fn typescript_bindings_refuse_a_dependency_the_wasm_module_never_links() {
    let linked = Workspace::write("wasm-linked", &["shapes"], "");
    let unlinked = Workspace::write("wasm-unlinked", &["shapes", "other"], "");

    typescript(&linked, Some("wasm32-unknown-unknown"))
        .expect("the wasm module exports every linked symbol");
    let error = typescript(&unlinked, Some("wasm32-unknown-unknown"))
        .expect_err("the unused crate's exports are not in the wasm module");

    assert!(refused_as_unlinked(&error, "wasm"), "{error}");
}
