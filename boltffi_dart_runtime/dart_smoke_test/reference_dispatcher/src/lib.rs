//! A minimal stand-in for what generated dispatch code does per synchronous
//! callback method: given a `CallableInstance` handle and two function
//! pointers (the `isolateLocal` fast path, and a `listener`-based one),
//! dispatch through whichever is safe for the calling thread.
//!
//! Hardcoded to one trivial shape (`i64 -> i64`) -- this is a usage
//! reference and smoke test, not a generalized dispatcher. Real codegen
//! (see `boltffi_backend`'s `target::dart::render::shim`) writes the result
//! into a method-shaped out-parameter rather than this smoke test's
//! keyed-by-gate-address map.

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::c_longlong;
use std::sync::Mutex;

use boltffi_dart_runtime::get_instance;

type FastPathFn = extern "C" fn(i64, *mut i64) -> i64;
type ListenerFn = extern "C" fn(i64, *mut c_void);

// A single slot is enough for a smoke test that only ever registers one.
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
pub extern "C" fn reference_set_fast_path(f: FastPathFn) {
    *FAST_PATH.lock().unwrap() = Some(f);
}

#[unsafe(no_mangle)]
pub extern "C" fn reference_set_listener(f: ListenerFn) {
    *LISTENER.lock().unwrap() = Some(f);
}

/// Called BY DART, from the listener body, to stash the computed result
/// before signaling the gate.
#[unsafe(no_mangle)]
pub extern "C" fn reference_write_result(gate_ptr: *mut c_void, value: c_longlong) {
    results().as_mut().unwrap().insert(gate_ptr as usize, value);
}

/// The actual reference dispatcher, mirroring exactly what codegen should
/// generate per synchronous method: check `is_owner_thread`, take the fast
/// or gated path accordingly.
#[unsafe(no_mangle)]
pub extern "C" fn reference_dispatch_call(
    handle: usize,
    value: c_longlong,
    out_status: *mut i64,
) -> c_longlong {
    let Some(instance) = get_instance(handle) else {
        unsafe { *out_status = 2 /* Cancelled */ };
        return 0;
    };

    if instance.is_owner_thread() {
        let f = FAST_PATH.lock().unwrap().expect("fast path not registered");
        return f(value, out_status);
    }

    let Some(gate) = instance.create_gate() else {
        unsafe { *out_status = 2 /* Cancelled */ };
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

/// Test-only: spawns a foreign OS thread that calls `reference_dispatch_call`,
/// reporting `(request_id, status, result)` back via `done_fn` -- expected
/// to be a `NativeCallable.listener` pointer, so the caller never blocks its
/// own isolate thread waiting on itself.
#[unsafe(no_mangle)]
pub extern "C" fn reference_start_cross_thread_call(
    handle: usize,
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
