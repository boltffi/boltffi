use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

use pim_reader::Item;

static BUILT: Once = Once::new();

#[test]
fn captures_items_a_source_scan_cannot_see() {
    let items = items(&dylib());

    for id in [
        "pim_toy::emitted::MacroEmitted",
        "pim_toy::emitted::ProcEmitted",
        "pim_toy::outdir::BuildScriptRecord",
    ] {
        assert!(
            items.contains_key(id),
            "`{id}` is emitted by a macro and must still reach the metadata section"
        );
    }
}

#[test]
fn resolves_imports_through_the_compiler() {
    let items = items(&dylib());

    assert_eq!(
        field(&items, "pim_toy::aliased::Route", "start"),
        "pim_toy::geometry::Point",
        "an aliased import resolves to the canonical id"
    );
    assert_eq!(
        field(&items, "pim_toy::globbed::Sim", "body"),
        "pim_toy::physics::Body",
        "a glob import resolves to the canonical id"
    );
    assert_eq!(
        field(&items, "pim_toy::reexport::Drawing", "shape"),
        "pim_toy::geometry::Shape",
        "a re-export resolves to the defining module, not the re-exporting one"
    );
}

#[test]
fn keeps_same_named_types_apart() {
    let items = items(&dylib());

    assert_eq!(
        field(&items, "pim_toy::geometry::Shape", "origin"),
        "pim_toy::geometry::Point",
        "geometry's Point wins in geometry"
    );
    assert_eq!(
        field(&items, "pim_toy::physics::Body", "position"),
        "pim_toy::physics::Point",
        "physics' Point wins in physics"
    );
}

#[test]
fn resolves_recursive_and_nested_types() {
    let items = items(&dylib());

    assert_eq!(
        field(&items, "pim_toy::nested::Node", "next"),
        "Option<Box<pim_toy::nested::Node>>",
        "a self-referential type does not induce a const cycle"
    );
    assert_eq!(
        field(&items, "pim_toy::nested::Index", "table"),
        "HashMap<String, Vec<pim_toy::geometry::Point>>",
        "slots nest inside generic shapes"
    );
}

#[test]
fn emits_a_slotless_record_for_primitive_only_types() {
    let records = records(&dylib());
    let plain = records
        .iter()
        .find(|record| record.module == "pim_toy::plain")
        .expect("plain module emits a record");

    assert!(
        plain.slots.is_empty(),
        "a record with no referenced types carries no slots"
    );
}

#[test]
fn resolves_a_remote_type_through_the_crate_tag() {
    let items = items(&dylib());

    assert_eq!(
        field(&items, "pim_toy::remote::Deadline", "after"),
        "std::time::Duration",
        "a `custom_type!`d foreign type gets its id from the crate-local tag impl"
    );
}

#[test]
fn resolves_a_type_from_a_dependency_crate() {
    let items = items(&dylib());

    assert_eq!(
        field(&items, "pim_toy::fromdep::Wrapper", "point"),
        "pim_dep::shapes::DepPoint",
        "a dependency's type resolves past its re-export, using this crate's tag"
    );
    assert!(
        items.contains_key("pim_dep::shapes::DepLine"),
        "a dependency's records reach the dylib even though nothing references them"
    );
}

#[test]
fn the_rlib_and_the_cdylib_agree_on_this_crate() {
    let own = |path: &Path| -> BTreeMap<String, Item> {
        items(path)
            .into_iter()
            .filter(|(id, _)| id.starts_with("pim_toy::"))
            .collect()
    };

    assert_eq!(
        own(&dylib()),
        own(&rlib()),
        "the crate's own records survive both the archive and the linked dylib"
    );
    assert!(
        !items(&rlib()).contains_key("pim_dep::shapes::DepPoint"),
        "an rlib carries only its own crate's records; only the link step aggregates"
    );
}

#[test]
fn cfg_is_evaluated_by_the_compiler_not_approximated() {
    assert!(
        !items(&dylib()).contains_key("pim_toy::gated::Extra"),
        "a feature-gated item is absent when the feature is off"
    );

    let featured = target().join("featured");
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "pim_toy", "--features", "extras"])
        .arg("--target-dir")
        .arg(&featured)
        .current_dir(workspace())
        .status()
        .expect("cargo runs");
    assert!(status.success(), "pim_toy builds with the feature on");

    let path = featured.join("debug").join(format!(
        "{}pim_toy{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let resolved = pim_reader::resolve(&pim_reader::read_artifact(&path).expect("artifact reads"))
        .expect("records resolve");
    let items = resolved
        .items
        .into_iter()
        .map(|item| (item.canonical_id.clone(), item))
        .collect::<BTreeMap<_, _>>();

    assert!(
        items.contains_key("pim_toy::gated::Extra"),
        "the same item appears once rustc says the feature is on"
    );
    assert!(
        resolved
            .functions
            .iter()
            .any(|function| function.canonical_id == "pim_toy::gated::extra_ping"),
        "a feature-gated export appears once rustc says the feature is on"
    );
}

fn items(path: &Path) -> BTreeMap<String, Item> {
    resolved(path)
        .into_iter()
        .map(|item| (item.canonical_id.clone(), item))
        .collect()
}

fn resolved(path: &Path) -> Vec<Item> {
    pim_reader::resolve(&records(path))
        .expect("records resolve")
        .items
}

fn records(path: &Path) -> Vec<pim_runtime::RawRecord> {
    build();
    pim_reader::read_artifact(path).expect("the artifact holds a metadata section")
}

fn field<'a>(items: &'a BTreeMap<String, Item>, id: &str, name: &str) -> &'a str {
    items
        .get(id)
        .unwrap_or_else(|| panic!("`{id}` is present"))
        .fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("`{id}` has a field named `{name}`"))
        .ty
        .as_str()
}

fn dylib() -> PathBuf {
    target().join(format!(
        "{}pim_toy{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
}

fn rlib() -> PathBuf {
    target().join("libpim_toy.rlib")
}

fn target() -> PathBuf {
    std::env::current_exe()
        .expect("the test binary has a path")
        .parent()
        .and_then(Path::parent)
        .expect("the test binary lives in <target>/<profile>/deps")
        .to_path_buf()
}

fn build() {
    BUILT.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "pim_toy"])
            .current_dir(workspace())
            .status()
            .expect("cargo runs");

        assert!(status.success(), "pim_toy builds");
    });
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("pim_reader sits inside the prototype workspace")
        .to_path_buf()
}
