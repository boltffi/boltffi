//! Calls per-invocation wrappers through symbols read from their own source records.
//!
//! A fixture cdylib compiles with `BOLTFFI_CAPTURE_WRAPPERS` set, its records are
//! aggregated, and every call goes through the record-carried symbol — the acceptance
//! shape RFC #665 prototyped.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use boltffi_bindgen::metadata::BindingMetadataBuild;
use boltffi_core::FfiStatus;
use boltffi_core::safety::PANIC_STATUS;
use boltffi_core::types::FfiBuf;

const FIXTURE_SOURCE: &str = r#"
use boltffi::*;

scaffolding!();

#[data]
#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn add(a: f64, b: f64) -> f64 {
    a + b
}

#[export]
pub fn shift(point: Point, by: f64) -> Point {
    Point {
        x: point.x + by,
        y: point.y + by,
    }
}

#[export]
pub fn shout(value: String) -> String {
    value.to_uppercase()
}

#[export]
pub fn boom(trigger: bool) -> f64 {
    if trigger {
        panic!("wrapper caught this");
    }
    1.0
}
"#;

struct FixtureCrate {
    root: PathBuf,
    manifest: PathBuf,
}

impl FixtureCrate {
    fn write() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "boltffi-capture-wrappers-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let source_dir = root.join("src");
        fs::create_dir_all(&source_dir).expect("create fixture source dir");
        let manifest = root.join("Cargo.toml");
        let boltffi = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("boltffi");
        fs::write(
            &manifest,
            format!(
                "[package]\nname = \"capture_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\npath = \"src/lib.rs\"\n\n[dependencies]\nboltffi = {{ path = \"{}\" }}\n",
                boltffi.display()
            ),
        )
        .expect("write fixture manifest");
        fs::write(source_dir.join("lib.rs"), FIXTURE_SOURCE).expect("write fixture source");
        Self { root, manifest }
    }
}

impl Drop for FixtureCrate {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn wire_string(value: &str) -> FfiBuf {
    FfiBuf::wire_encode(&value.to_owned())
}

fn unwire_string(buf: FfiBuf) -> String {
    boltffi_core::wire::decode(unsafe { buf.as_byte_slice() }).expect("string decodes")
}

#[test]
fn wrappers_are_called_through_their_record_carried_symbols() {
    let fixture = FixtureCrate::write();

    let source = BindingMetadataBuild::new(&fixture.manifest)
        .cargo_environment([("BOLTFFI_CAPTURE_WRAPPERS", "1")])
        .read_source()
        .expect("fixture source metadata reads");
    let contract =
        boltffi_binding::aggregate_records(&source.source_records, source.package.clone())
            .expect("fixture records aggregate");

    let symbol = |name: &str| {
        contract
            .functions
            .iter()
            .find(|function| function.id.as_str() == format!("capture_fixture::{name}"))
            .unwrap_or_else(|| panic!("function `{name}` is captured"))
            .native_symbol
            .clone()
            .unwrap_or_else(|| panic!("function `{name}` carries its wrapper symbol"))
    };

    let library_path = source
        .artifacts
        .iter()
        .find(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("dylib" | "so" | "dll")
            ) && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("capture_fixture"))
        })
        .expect("the fixture produced a loadable library")
        .clone();
    let library = unsafe { libloading::Library::new(&library_path) }.expect("fixture loads");

    let add: libloading::Symbol<'_, unsafe extern "C" fn(f64, f64, *mut FfiStatus) -> f64> =
        unsafe { library.get(symbol("add").as_bytes()) }.expect("add symbol resolves");
    let mut status = FfiStatus::OK;
    let sum = unsafe { add(2.0, 40.0, &mut status) };
    assert_eq!(sum, 42.0, "direct values cross by value");
    assert_eq!(status, FfiStatus::OK);

    let shift: libloading::Symbol<'_, unsafe extern "C" fn(FfiBuf, f64, *mut FfiStatus) -> FfiBuf> =
        unsafe { library.get(symbol("shift").as_bytes()) }.expect("shift symbol resolves");
    let point = FfiBuf::from_vec([1.5f64.to_le_bytes(), 2.5f64.to_le_bytes()].concat());
    let mut status = FfiStatus::OK;
    let shifted = unsafe { shift(point, 10.0, &mut status) };
    assert_eq!(status, FfiStatus::OK);
    let bytes = unsafe { shifted.as_byte_slice() };
    assert_eq!(
        bytes.len(),
        16,
        "a blittable point crosses as raw field bytes"
    );
    let x = f64::from_le_bytes(bytes[..8].try_into().expect("f64 width"));
    let y = f64::from_le_bytes(bytes[8..].try_into().expect("f64 width"));
    assert_eq!(
        (x, y),
        (11.5, 12.5),
        "records cross as wire-encoded buffers"
    );

    let shout: libloading::Symbol<'_, unsafe extern "C" fn(FfiBuf, *mut FfiStatus) -> FfiBuf> =
        unsafe { library.get(symbol("shout").as_bytes()) }.expect("shout symbol resolves");
    let mut status = FfiStatus::OK;
    let shouted = unsafe { shout(wire_string("hey"), &mut status) };
    assert_eq!(status, FfiStatus::OK);
    assert_eq!(unwire_string(shouted), "HEY", "strings round-trip");

    let boom: libloading::Symbol<'_, unsafe extern "C" fn(bool, *mut FfiStatus) -> f64> =
        unsafe { library.get(symbol("boom").as_bytes()) }.expect("boom symbol resolves");
    let mut status = FfiStatus::OK;
    let poisoned = unsafe { boom(true, &mut status) };
    assert_eq!(
        status, PANIC_STATUS,
        "a caught panic writes the panic status through the out-parameter"
    );
    assert_eq!(
        poisoned, 0.0,
        "the poisoned return is the type's zero value"
    );

    let mut status = FfiStatus::OK;
    let calm = unsafe { boom(false, &mut status) };
    assert_eq!(
        status,
        FfiStatus::OK,
        "the wrapper resets the status on entry"
    );
    assert_eq!(calm, 1.0);
}
