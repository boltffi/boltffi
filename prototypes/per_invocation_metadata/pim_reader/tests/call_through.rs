//! The acceptance bar: dlopen the artifact and call wrappers through the symbol names
//! read from their own metadata records — no name is ever derived on the reader side.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

use pim_reader::Function;

static BUILT: Once = Once::new();

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec2 {
    x: f64,
    y: f64,
}

#[repr(C)]
struct RawBuffer {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

impl RawBuffer {
    fn from_vec(bytes: Vec<u8>) -> Self {
        let mut bytes = std::mem::ManuallyDrop::new(bytes);
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            cap: bytes.capacity(),
        }
    }

    fn into_vec(self) -> Vec<u8> {
        unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) }
    }
}

#[repr(C)]
struct CallStatus {
    code: u8,
    panic_message: RawBuffer,
}

impl CallStatus {
    fn armed() -> Self {
        Self {
            code: u8::MAX,
            panic_message: RawBuffer {
                ptr: std::ptr::null_mut(),
                len: 0,
                cap: 0,
            },
        }
    }

    fn expect_success(&self) {
        assert_eq!(self.code, 0, "the wrapper reports success");
    }

    fn panic_message(self) -> String {
        assert_eq!(self.code, 1, "the wrapper reports a panic");
        String::from_utf8(self.panic_message.into_vec()).expect("the message is utf-8")
    }
}

#[test]
fn calls_a_direct_wrapper_by_value() {
    let functions = functions();
    let library = library();
    let add = symbol::<extern "C" fn(Vec2, Vec2, *mut CallStatus) -> Vec2>(
        &library,
        &functions["pim_toy::api::add_vec2"],
    );

    let mut status = CallStatus::armed();
    assert_eq!(
        add(Vec2 { x: 1.0, y: 2.0 }, Vec2 { x: 3.0, y: 4.0 }, &mut status),
        Vec2 { x: 4.0, y: 6.0 },
        "a direct record crosses by value through the recorded symbol"
    );
    status.expect_success();
}

#[test]
fn calls_an_encoded_wrapper_through_a_buffer() {
    let functions = functions();
    let library = library();
    let describe = symbol::<extern "C" fn(RawBuffer, *mut CallStatus) -> RawBuffer>(
        &library,
        &functions["pim_toy::api::describe_shape"],
    );

    let mut shape = Vec::new();
    write_f64(&mut shape, 2.0);
    write_f64(&mut shape, 3.0);
    shape.extend_from_slice(&2u64.to_le_bytes());
    write_f64(&mut shape, 0.0);
    write_f64(&mut shape, 0.0);
    write_f64(&mut shape, 1.0);
    write_f64(&mut shape, 1.0);
    shape.extend_from_slice(&7u64.to_le_bytes());

    let mut status = CallStatus::armed();
    let described = describe(RawBuffer::from_vec(shape), &mut status).into_vec();
    status.expect_success();
    assert_eq!(
        read_string(&mut described.as_slice()),
        "Shape #7 at (2, 3) with 2 points",
        "an encoded record round-trips through the wrapper"
    );
}

#[test]
fn returns_the_error_arm_through_the_result_buffer() {
    let functions = functions();
    let library = library();
    let divide = symbol::<extern "C" fn(f64, f64, *mut CallStatus) -> RawBuffer>(
        &library,
        &functions["pim_toy::api::checked_div"],
    );

    let mut status = CallStatus::armed();
    let ok = divide(9.0, 3.0, &mut status).into_vec();
    status.expect_success();
    let input = &mut ok.as_slice();
    assert_eq!(read_u8(input), 0, "the ok arm is tagged 0");
    assert_eq!(read_f64(input), 3.0, "the ok payload follows the tag");

    let mut status = CallStatus::armed();
    let err = divide(1.0, 0.0, &mut status).into_vec();
    status.expect_success();
    let input = &mut err.as_slice();
    assert_eq!(read_u8(input), 1, "the error arm is tagged 1");
    assert_eq!(read_u32(input), 1, "the error payload's code field");
    assert_eq!(
        read_string(input),
        "division by zero",
        "the error payload's message field"
    );
}

#[test]
fn calls_wrappers_a_source_scan_cannot_see() {
    let functions = functions();
    let library = library();

    let double = symbol::<extern "C" fn(f64, *mut CallStatus) -> f64>(
        &library,
        &functions["pim_toy::api::double_it"],
    );
    let mut status = CallStatus::armed();
    assert_eq!(
        double(21.0, &mut status),
        42.0,
        "a macro_rules-emitted export is callable"
    );
    status.expect_success();

    let sum = symbol::<extern "C" fn(RawBuffer, *mut CallStatus) -> f64>(
        &library,
        &functions["pim_toy::api::build_script_sum"],
    );
    let mut values = Vec::new();
    values.extend_from_slice(&2u64.to_le_bytes());
    write_f64(&mut values, 1.5);
    write_f64(&mut values, 2.5);
    let mut status = CallStatus::armed();
    assert_eq!(
        sum(RawBuffer::from_vec(values), &mut status),
        4.0,
        "an `include!(OUT_DIR)` export is callable"
    );
    status.expect_success();
}

#[test]
fn distinguishes_direct_from_encoded_in_the_records() {
    let resolved = resolved();
    let direct = |id: &str| {
        resolved
            .items
            .iter()
            .find(|item| item.canonical_id == id)
            .unwrap_or_else(|| panic!("`{id}` is present"))
            .direct
    };

    assert!(
        direct("pim_toy::api::Vec2"),
        "repr(C) + primitives is direct"
    );
    assert!(
        !direct("pim_toy::api::MathError"),
        "a String field forces the encoded codec"
    );
    assert!(
        !direct("pim_toy::geometry::Point"),
        "primitive fields without repr(C) stay encoded"
    );
}

#[test]
fn a_gated_export_is_absent_without_the_feature() {
    assert!(
        !functions().contains_key("pim_toy::gated::extra_ping"),
        "rustc never ran the macro, so no record and no symbol exist"
    );
}

#[test]
fn a_panicking_export_reports_through_the_status() {
    let functions = functions();
    let library = library();
    let assert_positive = symbol::<extern "C" fn(f64, *mut CallStatus) -> f64>(
        &library,
        &functions["pim_toy::api::assert_positive"],
    );

    let mut status = CallStatus::armed();
    assert_eq!(assert_positive(2.0, &mut status), 2.0, "the happy path");
    status.expect_success();

    let mut status = CallStatus::armed();
    assert_positive(-1.0, &mut status);
    assert!(
        status.panic_message().contains("value must be positive"),
        "the panic crosses as a status code and message instead of aborting"
    );
}

#[test]
fn a_malformed_buffer_reports_through_the_status() {
    let functions = functions();
    let library = library();
    let describe = symbol::<extern "C" fn(RawBuffer, *mut CallStatus) -> RawBuffer>(
        &library,
        &functions["pim_toy::api::describe_shape"],
    );

    let mut status = CallStatus::armed();
    describe(RawBuffer::from_vec(vec![1, 2, 3]), &mut status);
    assert!(
        !status.panic_message().is_empty(),
        "a truncated buffer fails the lift inside the wrapper, not the process"
    );
}

#[test]
fn frees_a_returned_buffer_through_the_scaffolding_export() {
    let resolved = resolved();
    let library = library();
    let scaffolding = resolved
        .scaffolding
        .iter()
        .find(|entry| entry.module == "pim_toy")
        .expect("the crate's `scaffolding!` leaves a record");
    let free = unsafe {
        library.get::<extern "C" fn(RawBuffer)>(scaffolding.free_symbol.as_bytes())
    }
    .expect("the free export is exported");

    let functions = resolved
        .functions
        .iter()
        .map(|function| (function.canonical_id.clone(), function.clone()))
        .collect::<BTreeMap<_, _>>();
    let divide = symbol::<extern "C" fn(f64, f64, *mut CallStatus) -> RawBuffer>(
        &library,
        &functions["pim_toy::api::checked_div"],
    );

    let mut status = CallStatus::armed();
    free(divide(9.0, 3.0, &mut status));
    status.expect_success();
}

#[test]
fn drives_a_class_through_its_recorded_handle_protocol() {
    let resolved = resolved();
    let library = library();
    let class = resolved
        .classes
        .iter()
        .find(|class| class.canonical_id == "pim_toy::counter::Counter")
        .expect("the impl block leaves a class record");

    let new = class_symbol::<extern "C" fn(RawBuffer, u64, *mut CallStatus) -> u64>(
        &library,
        &class.constructors,
        "new",
    );
    let add = class_symbol::<extern "C" fn(u64, u64, *mut CallStatus) -> u64>(
        &library,
        &class.methods,
        "add",
    );
    let label = class_symbol::<extern "C" fn(u64, *mut CallStatus) -> RawBuffer>(
        &library,
        &class.methods,
        "label",
    );
    let free = unsafe { library.get::<extern "C" fn(u64)>(class.free_symbol.as_bytes()) }
        .expect("the free export is exported");

    let mut status = CallStatus::armed();
    let mut name = Vec::new();
    write_string(&mut name, "clicks");
    let handle = new(RawBuffer::from_vec(name), 10, &mut status);
    status.expect_success();
    assert_ne!(handle, 0, "the constructor yields a live handle");

    let mut status = CallStatus::armed();
    assert_eq!(add(handle, 5, &mut status), 15, "state lives behind the handle");
    status.expect_success();
    let mut status = CallStatus::armed();
    assert_eq!(add(handle, 2, &mut status), 17, "and persists across calls");
    status.expect_success();

    let mut status = CallStatus::armed();
    let described = label(handle, &mut status).into_vec();
    status.expect_success();
    assert_eq!(
        read_string(&mut described.as_slice()),
        "clicks",
        "an encoded return crosses out of a method"
    );

    let mut status = CallStatus::armed();
    add(0, 1, &mut status);
    assert!(
        status.panic_message().contains("null class handle"),
        "a null handle is a status error, not a crash"
    );

    free(handle);
}

static CAPTURED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static FREED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[repr(C)]
struct LoggerVTable {
    free: extern "C" fn(u64),
    log: extern "C" fn(u64, RawBuffer) -> bool,
    level: extern "C" fn(u64) -> u32,
}

#[repr(C)]
struct CallbackHandle {
    data: u64,
    vtable: *const std::ffi::c_void,
}

extern "C" fn logger_free(_data: u64) {
    FREED.store(true, std::sync::atomic::Ordering::SeqCst);
}

extern "C" fn logger_log(_data: u64, line: RawBuffer) -> bool {
    let bytes = line.into_vec();
    let line = read_string(&mut bytes.as_slice());
    let keep = !line.contains("noise");
    CAPTURED.lock().expect("no test panicked here").push(line);
    keep
}

extern "C" fn logger_level(_data: u64) -> u32 {
    3
}

#[test]
fn dispatches_a_callback_through_the_recorded_vtable_order() {
    let resolved = resolved();
    let library = library();
    let callback = resolved
        .callbacks
        .iter()
        .find(|callback| callback.canonical_id == "pim_toy::logging::Logger")
        .expect("the trait leaves a callback record");
    assert_eq!(
        callback
            .methods
            .iter()
            .map(|method| method.name.as_str())
            .collect::<Vec<_>>(),
        ["log", "level"],
        "the record pins the vtable order after the free slot"
    );

    let drain = resolved
        .functions
        .iter()
        .find(|function| function.canonical_id == "pim_toy::logging::drain_logs")
        .expect("the export taking the callback is recorded");
    assert_eq!(
        drain.params[0].ty, "Box<pim_toy::logging::Logger>",
        "the callback parameter resolves to the trait's canonical id"
    );
    let drain = symbol::<extern "C" fn(CallbackHandle, RawBuffer, *mut CallStatus) -> u32>(
        &library,
        drain,
    );

    static VTABLE: LoggerVTable = LoggerVTable {
        free: logger_free,
        log: logger_log,
        level: logger_level,
    };
    let mut lines = Vec::new();
    lines.extend_from_slice(&3u64.to_le_bytes());
    write_string(&mut lines, "ready");
    write_string(&mut lines, "noise: skip me");
    write_string(&mut lines, "done");

    let mut status = CallStatus::armed();
    let kept = drain(
        CallbackHandle {
            data: 7,
            vtable: &raw const VTABLE as *const std::ffi::c_void,
        },
        RawBuffer::from_vec(lines),
        &mut status,
    );
    status.expect_success();

    assert_eq!(kept, 2, "the callback's returns steer the Rust side");
    assert_eq!(
        CAPTURED.lock().expect("no test panicked here").as_slice(),
        [
            "[3] ready".to_owned(),
            "[3] noise: skip me".to_owned(),
            "[3] done".to_owned()
        ],
        "every dispatch reached the foreign vtable with its encoded argument"
    );
    assert!(
        FREED.load(std::sync::atomic::Ordering::SeqCst),
        "dropping the Box<dyn Logger> reaches the vtable's free slot"
    );
}

#[test]
fn pops_a_stream_to_exhaustion_through_its_recorded_protocol() {
    let resolved = resolved();
    let library = library();
    let stream = resolved
        .streams
        .iter()
        .find(|stream| stream.canonical_id == "pim_toy::streaming::countdown")
        .expect("the stream export leaves a stream record");
    assert_eq!(stream.item, "u32", "the record names the item type");

    let subscribe = unsafe {
        library.get::<extern "C" fn(u32, *mut CallStatus) -> u64>(stream.subscribe_symbol.as_bytes())
    }
    .expect("the subscribe export is exported");
    let pop = unsafe {
        library.get::<extern "C" fn(u64, *mut CallStatus) -> RawBuffer>(stream.pop_symbol.as_bytes())
    }
    .expect("the pop export is exported");
    let free = unsafe { library.get::<extern "C" fn(u64)>(stream.free_symbol.as_bytes()) }
        .expect("the free export is exported");

    let mut status = CallStatus::armed();
    let handle = subscribe(3, &mut status);
    status.expect_success();
    assert_ne!(handle, 0, "subscribing yields a live handle");

    let mut seen = Vec::new();
    loop {
        let mut status = CallStatus::armed();
        let popped = pop(handle, &mut status).into_vec();
        status.expect_success();
        let input = &mut popped.as_slice();
        match read_u8(input) {
            0 => break,
            _ => seen.push(read_u32(input)),
        }
    }
    assert_eq!(seen, [3, 2, 1], "items pop in order until exhaustion");

    free(handle);
}

fn class_symbol<'a, T>(
    library: &'a libloading::Library,
    methods: &[pim_reader::Method],
    name: &str,
) -> libloading::Symbol<'a, T> {
    let method = methods
        .iter()
        .find(|method| method.name == name)
        .unwrap_or_else(|| panic!("`{name}` is recorded"));
    unsafe { library.get(method.symbol.as_bytes()) }
        .unwrap_or_else(|_| panic!("`{}` is exported", method.symbol))
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn functions() -> BTreeMap<String, Function> {
    resolved()
        .functions
        .into_iter()
        .map(|function| (function.canonical_id.clone(), function))
        .collect()
}

fn resolved() -> pim_reader::Resolved {
    build();
    let records = pim_reader::read_artifact(&dylib()).expect("the artifact holds records");
    pim_reader::resolve(&records).expect("records resolve")
}

fn library() -> libloading::Library {
    build();
    unsafe { libloading::Library::new(dylib()) }.expect("the dylib loads")
}

fn symbol<'a, T>(
    library: &'a libloading::Library,
    function: &Function,
) -> libloading::Symbol<'a, T> {
    unsafe { library.get(function.symbol.as_bytes()) }
        .unwrap_or_else(|_| panic!("`{}` is exported", function.symbol))
}

fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u8(input: &mut &[u8]) -> u8 {
    let (byte, rest) = input.split_first().expect("a byte remains");
    *input = rest;
    *byte
}

fn read_u32(input: &mut &[u8]) -> u32 {
    let (bytes, rest) = input.split_at(4);
    *input = rest;
    u32::from_le_bytes(bytes.try_into().expect("four bytes"))
}

fn read_u64(input: &mut &[u8]) -> u64 {
    let (bytes, rest) = input.split_at(8);
    *input = rest;
    u64::from_le_bytes(bytes.try_into().expect("eight bytes"))
}

fn read_f64(input: &mut &[u8]) -> f64 {
    f64::from_bits(read_u64(input))
}

fn read_string(input: &mut &[u8]) -> String {
    let len = read_u64(input) as usize;
    let (bytes, rest) = input.split_at(len);
    *input = rest;
    String::from_utf8(bytes.to_vec()).expect("encoded string is utf-8")
}

fn dylib() -> PathBuf {
    target().join(format!(
        "{}pim_toy{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
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
