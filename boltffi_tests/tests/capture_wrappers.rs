//! Calls per-invocation wrappers through symbols read from their own source records.
//!
//! A fixture cdylib compiles with `BOLTFFI_CAPTURE_WRAPPERS` set, its records are
//! aggregated, and every call goes through the record-carried symbol — the acceptance
//! shape RFC #665 prototyped.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use boltffi_bindgen::metadata::BindingMetadataBuild;
use boltffi_core::safety::PANIC_STATUS;
use boltffi_core::types::FfiBuf;
use boltffi_core::{FfiStatus, RustFutureContinuationCallback, RustFutureHandle, RustFuturePoll};

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
pub fn until(deadline: std::time::Duration) -> u64 {
    deadline.as_millis() as u64
}

#[export]
pub fn boom(trigger: bool) -> f64 {
    if trigger {
        panic!("wrapper caught this");
    }
    1.0
}

#[export]
pub async fn shout_later(value: String, suffix: String) -> String {
    Delay::new(std::time::Duration::from_millis(10)).await;
    format!("{}{suffix}", value.to_uppercase())
}

#[export]
pub async fn double(value: f64) -> f64 {
    value * 2.0
}

struct Delay {
    woken: std::sync::Arc<std::sync::atomic::AtomicBool>,
    started: bool,
    delay: std::time::Duration,
}

impl Delay {
    fn new(delay: std::time::Duration) -> Self {
        Self {
            woken: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            started: false,
            delay,
        }
    }
}

impl std::future::Future for Delay {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.woken.load(std::sync::atomic::Ordering::Acquire) {
            return std::task::Poll::Ready(());
        }
        if !self.started {
            self.started = true;
            let woken = self.woken.clone();
            let waker = cx.waker().clone();
            let delay = self.delay;
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                woken.store(true, std::sync::atomic::Ordering::Release);
                waker.wake();
            });
        }
        std::task::Poll::Pending
    }
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

extern "C" fn send_poll_signal(callback_data: u64, poll: RustFuturePoll) {
    let sender = unsafe { &*(callback_data as *const mpsc::Sender<RustFuturePoll>) };
    sender.send(poll).expect("the poll signal delivers");
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

    let mut status = FfiStatus::OK;
    let refused = unsafe { shout(FfiBuf::from_vec(vec![0xFF, 0xFF, 0xFF]), &mut status) };
    assert_eq!(
        status,
        FfiStatus::INVALID_ARG,
        "malformed bytes report the invalid-argument status"
    );
    assert!(
        unsafe { refused.as_byte_slice() }.is_empty(),
        "the refused return is the empty buffer"
    );

    let until: libloading::Symbol<'_, unsafe extern "C" fn(FfiBuf, *mut FfiStatus) -> u64> =
        unsafe { library.get(symbol("until").as_bytes()) }.expect("until symbol resolves");
    let mut status = FfiStatus::OK;
    let millis = unsafe {
        until(
            FfiBuf::wire_encode(&Duration::from_millis(1500)),
            &mut status,
        )
    };
    assert_eq!(status, FfiStatus::OK);
    assert_eq!(millis, 1500, "builtins cross as encoded buffers");

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

    let drive =
        |poll: unsafe extern "C" fn(RustFutureHandle, u64, RustFutureContinuationCallback),
         handle: RustFutureHandle| {
            let (sender, receiver) = mpsc::channel::<RustFuturePoll>();
            let sender = Box::new(sender);
            loop {
                let callback_data = &*sender as *const mpsc::Sender<RustFuturePoll> as u64;
                unsafe { poll(handle, callback_data, send_poll_signal) };
                match receiver
                    .recv_timeout(Duration::from_secs(5))
                    .expect("the continuation callback fires")
                {
                    RustFuturePoll::Ready => break,
                    RustFuturePoll::MaybeReady => continue,
                }
            }
        };

    let shout_later = symbol("shout_later");
    let start: libloading::Symbol<'_, unsafe extern "C" fn(FfiBuf, FfiBuf) -> RustFutureHandle> =
        unsafe { library.get(shout_later.as_bytes()) }.expect("shout_later symbol resolves");
    let poll: libloading::Symbol<
        '_,
        unsafe extern "C" fn(RustFutureHandle, u64, RustFutureContinuationCallback),
    > = unsafe { library.get(format!("{shout_later}_poll").as_bytes()) }
        .expect("shout_later poll companion resolves");
    let complete: libloading::Symbol<
        '_,
        unsafe extern "C" fn(RustFutureHandle, *mut FfiStatus) -> FfiBuf,
    > = unsafe { library.get(format!("{shout_later}_complete").as_bytes()) }
        .expect("shout_later complete companion resolves");
    let cancel: libloading::Symbol<'_, unsafe extern "C" fn(RustFutureHandle)> =
        unsafe { library.get(format!("{shout_later}_cancel").as_bytes()) }
            .expect("shout_later cancel companion resolves");
    let free: libloading::Symbol<'_, unsafe extern "C" fn(RustFutureHandle)> =
        unsafe { library.get(format!("{shout_later}_free").as_bytes()) }
            .expect("shout_later free companion resolves");
    unsafe {
        library.get::<unsafe extern "C" fn(RustFutureHandle) -> FfiBuf>(
            format!("{shout_later}_panic_message").as_bytes(),
        )
    }
    .expect("shout_later panic message companion resolves");

    let handle = unsafe { start(wire_string("later"), wire_string("!")) };
    drive(*poll, handle);
    let mut status = FfiStatus::OK;
    let shouted = unsafe { complete(handle, &mut status) };
    assert_eq!(status, FfiStatus::OK);
    assert_eq!(
        unwire_string(shouted),
        "LATER!",
        "async values cross at the completion call"
    );
    unsafe { free(handle) };

    let handle = unsafe {
        start(
            wire_string("later"),
            FfiBuf::from_vec(vec![0xFF, 0xFF, 0xFF]),
        )
    };
    drive(*poll, handle);
    let mut status = FfiStatus::OK;
    let refused = unsafe { complete(handle, &mut status) };
    assert_eq!(
        status,
        FfiStatus::INVALID_ARG,
        "malformed argument bytes surface at the completion call"
    );
    assert!(
        unsafe { refused.as_byte_slice() }.is_empty(),
        "the refused completion is the empty buffer"
    );
    unsafe { free(handle) };

    let handle = unsafe { start(wire_string("dropped"), wire_string("?")) };
    unsafe { cancel(handle) };
    drive(*poll, handle);
    let mut status = FfiStatus::OK;
    let cancelled = unsafe { complete(handle, &mut status) };
    assert_eq!(
        status,
        FfiStatus::CANCELLED,
        "a cancelled future reports through the completion status"
    );
    assert!(
        unsafe { cancelled.as_byte_slice() }.is_empty(),
        "the cancelled completion is the empty buffer"
    );
    unsafe { free(handle) };

    let double = symbol("double");
    let start_double: libloading::Symbol<'_, unsafe extern "C" fn(f64) -> RustFutureHandle> =
        unsafe { library.get(double.as_bytes()) }.expect("double symbol resolves");
    let poll_double: libloading::Symbol<
        '_,
        unsafe extern "C" fn(RustFutureHandle, u64, RustFutureContinuationCallback),
    > = unsafe { library.get(format!("{double}_poll").as_bytes()) }
        .expect("double poll companion resolves");
    let complete_double: libloading::Symbol<
        '_,
        unsafe extern "C" fn(RustFutureHandle, *mut FfiStatus) -> f64,
    > = unsafe { library.get(format!("{double}_complete").as_bytes()) }
        .expect("double complete companion resolves");
    let free_double: libloading::Symbol<'_, unsafe extern "C" fn(RustFutureHandle)> =
        unsafe { library.get(format!("{double}_free").as_bytes()) }
            .expect("double free companion resolves");

    let handle = unsafe { start_double(21.0) };
    drive(*poll_double, handle);
    let mut status = FfiStatus::OK;
    let doubled = unsafe { complete_double(handle, &mut status) };
    assert_eq!(status, FfiStatus::OK);
    assert_eq!(doubled, 42.0, "direct async values cross by value");
    unsafe { free_double(handle) };
}
