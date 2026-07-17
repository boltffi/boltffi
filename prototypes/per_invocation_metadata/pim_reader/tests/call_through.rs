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
