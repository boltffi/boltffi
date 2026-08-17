//! Stand-in for generated dual-path dispatch: `fromFunction` on the owner
//! thread, `NativeCallable.listener` + gate off-thread.

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::c_longlong;
use std::sync::Mutex;

use boltffi_dart_runtime::{header_from_handle, HooksHeader};

type FastPathFn = extern "C" fn(i64, *mut i64) -> i64;
type ListenerFn = extern "C" fn(i64, *mut c_void);

#[repr(C)]
struct DemoHooks {
    header: HooksHeader,
}

static FAST_PATH: Mutex<Option<FastPathFn>> = Mutex::new(None);
static LISTENER: Mutex<Option<ListenerFn>> = Mutex::new(None);
static RESULTS: Mutex<Option<HashMap<usize, i64>>> = Mutex::new(None);

fn results() -> std::sync::MutexGuard<'static, Option<HashMap<usize, i64>>> {
    let mut guard = RESULTS.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_register() -> u64 {
    Box::into_raw(Box::new(DemoHooks {
        header: HooksHeader::new(),
    })) as u64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reference_release(handle: u64) {
    if handle == 0 {
        return;
    }
    let hooks = unsafe { &*(handle as *const DemoHooks) };
    hooks.header.shutdown();
    drop(unsafe { Box::from_raw(handle as *mut DemoHooks) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reference_outstanding(handle: u64) -> i64 {
    match unsafe { header_from_handle(handle) } {
        Some(header) => header.outstanding() as i64,
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_set_fast_path(f: FastPathFn) {
    *FAST_PATH.lock().unwrap() = Some(f);
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_set_listener(f: ListenerFn) {
    *LISTENER.lock().unwrap() = Some(f);
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_write_result(gate_ptr: *mut c_void, value: c_longlong) {
    results().as_mut().unwrap().insert(gate_ptr as usize, value);
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_dispatch_call(
    handle: u64,
    value: c_longlong,
    out_status: *mut i64,
) -> c_longlong {
    let Some(header) = (unsafe { header_from_handle(handle) }) else {
        unsafe { *out_status = 2 };
        return 0;
    };

    if header.is_owner() {
        let f = FAST_PATH.lock().unwrap().expect("fast path not registered");
        return f(value, out_status);
    }

    let Some(gate) = header.create_gate() else {
        unsafe { *out_status = 2 };
        return 0;
    };
    let gate_raw = gate.raw();
    let gate_key = gate_raw as usize;

    let listener = LISTENER.lock().unwrap().expect("listener not registered");
    listener(value, gate_raw);

    let status = gate.wait();
    let result = results().as_mut().unwrap().remove(&gate_key).unwrap_or(0);

    unsafe { *out_status = status as i64 };
    result
}

type TestDoneFn = extern "C" fn(i64, i64, i64);

#[unsafe(no_mangle)]
pub extern "C" fn reference_start_cross_thread_call(
    handle: u64,
    value: c_longlong,
    request_id: c_longlong,
    done_fn: TestDoneFn,
) {
    std::thread::spawn(move || {
        let mut status = 0i64;
        let result = reference_dispatch_call(handle, value, &mut status);
        done_fn(request_id, status, result);
    });
}
