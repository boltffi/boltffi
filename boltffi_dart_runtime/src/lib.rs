//! Thread-safe synchronous callback dispatch for BoltFFI's native Dart
//! target.
//!
//! Dart FFI offers two ways for native code to invoke into Dart:
//!  - `NativeCallable.isolateLocal` (and `Pointer.fromFunction`): synchronous,
//!    but aborts the process if invoked from any OS thread other than the
//!    one that owns the Dart isolate.
//!  - `NativeCallable.listener`: safe to call from any thread, but
//!    asynchronous -- the native call returns before the Dart body runs.
//!
//! BoltFFI's callback traits are `Send + Sync`, so application code can
//! legally move them to a background thread. Generated dispatch code picks
//! between the two based on [`CallableInstance::is_owner_thread`]: same
//! thread calls the existing `isolateLocal` pointer directly; a foreign
//! thread posts through `listener` and blocks on a [`Gate`] until Dart
//! resolves it via [`signal_gate_ok`]/[`signal_gate_error`]. The isolate's
//! own thread is never blocked, so there's no deadlock.
//!
//! Destruction is two-phase: [`boltffi_dart_runtime_destroy_instance`] marks
//! the instance dead and cancels outstanding calls but keeps the handle
//! queryable (so [`boltffi_dart_runtime_instance_outstanding_count`] still
//! resolves); [`boltffi_dart_runtime_forget_instance`] removes it. Closing a
//! `NativeCallable.listener` while a posted message is still unprocessed is
//! undefined behavior on Dart's side, so generated code must drain
//! outstanding calls before forgetting.
//!
//! Known limitation: the owner thread is captured once, at registration,
//! and trusted for the instance's lifetime -- there's no public Dart API to
//! verify isolate-thread ownership live. This is a contract (register a
//! callback only on the isolate that will keep owning it), not a
//! mechanically enforced guarantee.

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread::ThreadId;

/// The outcome of a dispatched call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum CallStatus {
    /// The Dart-side callback ran and completed normally.
    Ok = 0,
    /// The Dart-side callback threw.
    Error = 1,
    /// The instance was torn down before (or while) this call was pending.
    Cancelled = 2,
}

impl CallStatus {
    fn from_i64(value: i64) -> Self {
        match value {
            0 => CallStatus::Ok,
            1 => CallStatus::Error,
            _ => CallStatus::Cancelled,
        }
    }
}

enum GateOutcome {
    Pending,
    Done(CallStatus),
}

/// A single in-flight cross-thread call's synchronization point. Carries no
/// payload -- results/arguments are handled by BoltFFI's existing generated
/// marshaling code.
pub struct Gate {
    outcome: Mutex<GateOutcome>,
    cvar: Condvar,
}

impl Gate {
    fn resolve(&self, status: CallStatus) {
        let mut guard = self.outcome.lock().unwrap();
        if matches!(*guard, GateOutcome::Pending) {
            *guard = GateOutcome::Done(status);
            self.cvar.notify_all();
        }
    }

    /// Blocks the calling thread until Dart resolves this gate. Safe to call
    /// from any thread except the isolate's own.
    pub fn wait(&self) -> CallStatus {
        let mut guard = self.outcome.lock().unwrap();
        loop {
            match &*guard {
                GateOutcome::Pending => guard = self.cvar.wait(guard).unwrap(),
                GateOutcome::Done(status) => return *status,
            }
        }
    }
}

/// A [`Gate`] paired with the raw pointer that identifies it across the FFI
/// boundary. Does not expose a repeatable "give me a raw pointer" method --
/// an earlier version did, and calling it more than once per gate leaked a
/// reference per extra call. [`PendingGate::raw`] returns the same
/// already-materialized pointer every time.
pub struct PendingGate {
    gate: Arc<Gate>,
    raw: *mut c_void,
}

impl PendingGate {
    /// The raw pointer to pass as the trailing argument on the
    /// `NativeCallable.listener` call. Reclaimed exactly once by whichever
    /// of [`signal_gate_ok`]/[`signal_gate_error`] runs for it.
    pub fn raw(&self) -> *mut c_void {
        self.raw
    }

    /// Blocks the calling thread until Dart resolves this gate.
    pub fn wait(&self) -> CallStatus {
        self.gate.wait()
    }
}

struct InstanceState {
    alive: bool,
    outstanding: Vec<Weak<Gate>>,
}

/// One callback/closure registration. Real generated code embeds a handle to
/// one of these per `NativeCallable` pair.
pub struct CallableInstance {
    owner_thread: ThreadId,
    state: Mutex<InstanceState>,
}

impl CallableInstance {
    /// Whether the calling thread is the one that registered this instance.
    /// Must be checked fresh on every call, not cached by the caller.
    pub fn is_owner_thread(&self) -> bool {
        std::thread::current().id() == self.owner_thread
    }

    /// Registers a new in-flight call, atomically with `destroy_instance`'s
    /// "mark dead, cancel everyone tracked". Returns `None` if the instance
    /// is already dead.
    pub fn create_gate(&self) -> Option<PendingGate> {
        let gate = Arc::new(Gate {
            outcome: Mutex::new(GateOutcome::Pending),
            cvar: Condvar::new(),
        });
        let mut state = self.state.lock().unwrap();
        if !state.alive {
            return None;
        }
        state.outstanding.retain(|weak| weak.strong_count() > 0);
        state.outstanding.push(Arc::downgrade(&gate));
        drop(state);

        let raw = Arc::into_raw(gate.clone()) as *mut c_void;
        Some(PendingGate { gate, raw })
    }
}

type InstanceTable = Mutex<HashMap<usize, Arc<CallableInstance>>>;
static INSTANCES: OnceLock<InstanceTable> = OnceLock::new();
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);

fn instances() -> &'static InstanceTable {
    INSTANCES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolves a handle to its instance (dead or alive), cloning the `Arc`
/// while the table's own lock is held.
pub fn get_instance(handle: usize) -> Option<Arc<CallableInstance>> {
    instances().lock().unwrap().get(&handle).cloned()
}

/// Creates a new instance, capturing the calling thread as its owner. Called
/// by Dart directly, once, when a callback/closure is registered.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_create_instance() -> usize {
    let instance = Arc::new(CallableInstance {
        owner_thread: std::thread::current().id(),
        state: Mutex::new(InstanceState {
            alive: true,
            outstanding: Vec::new(),
        }),
    });
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    instances().lock().unwrap().insert(handle, instance);
    handle
}

/// Soft teardown. Safe to call more than once, or on an unknown handle.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_destroy_instance(handle: usize) {
    let Some(instance) = get_instance(handle) else {
        return;
    };
    let mut state = instance.state.lock().unwrap();
    state.alive = false;
    for weak in state.outstanding.iter() {
        if let Some(gate) = weak.upgrade() {
            gate.resolve(CallStatus::Cancelled);
        }
    }
}

/// Hard teardown. Should only be called after
/// [`boltffi_dart_runtime_instance_outstanding_count`] reaches zero
/// following [`boltffi_dart_runtime_destroy_instance`].
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_forget_instance(handle: usize) {
    instances().lock().unwrap().remove(&handle);
}

/// How many cross-thread calls are still outstanding for this instance.
/// Generated Dart code must not close the `listener` `NativeCallable` while
/// this is nonzero.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_instance_outstanding_count(handle: usize) -> i64 {
    let Some(instance) = get_instance(handle) else {
        return 0;
    };
    let mut state = instance.state.lock().unwrap();
    state.outstanding.retain(|weak| weak.strong_count() > 0);
    state.outstanding.len() as i64
}

/// Called by Dart, from the end of a `listener` body, once the callback ran
/// successfully.
///
/// # Safety
/// `gate_ptr` must be a pointer previously returned by [`PendingGate::raw`],
/// and this function (or [`signal_gate_error`]) must be called with it
/// exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn signal_gate_ok(gate_ptr: *mut c_void) {
    let gate = unsafe { Arc::from_raw(gate_ptr as *const Gate) };
    gate.resolve(CallStatus::Ok);
}

/// Called by Dart when the callback body threw, instead of
/// [`signal_gate_ok`].
///
/// # Safety
/// Same contract as [`signal_gate_ok`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn signal_gate_error(gate_ptr: *mut c_void) {
    let gate = unsafe { Arc::from_raw(gate_ptr as *const Gate) };
    gate.resolve(CallStatus::Error);
}

// Generated Dart-target dispatch shims are plain Rust, `include!()`'d into
// the `boltffi` facade crate and linked against this crate as an ordinary
// dependency -- they call the functions below as normal Rust function
// calls, not through FFI. These stay `extern "C"` because a few
// (`boltffi_dart_runtime_create_instance`, `signal_gate_ok`,
// `signal_gate_error`) are called by Dart directly and need a stable ABI.

/// Whether the calling thread owns `handle`'s isolate. Returns `false` for
/// an unknown/destroyed handle.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_is_owner_thread(handle: usize) -> bool {
    get_instance(handle).is_some_and(|instance| instance.is_owner_thread())
}

/// Registers a new in-flight call and returns the raw gate pointer, or null
/// if the instance is already dead.
///
/// Reserves a *second*, independent strong reference for
/// [`boltffi_dart_runtime_wait_gate`] to reclaim, on top of the one
/// [`PendingGate::raw`] already materializes for
/// `signal_gate_ok`/`signal_gate_error`. Both references happen to share
/// the same pointer value (an `Arc`'s data pointer is stable across
/// clones), so it's safe to call `Arc::from_raw` on that pointer twice
/// overall, once from each side, in any order -- each independently owns
/// one of the two units this function pins here. Without this, the two C
/// calls (`create_gate` then, later, `wait_gate`) would share only one
/// strong reference between the moment `create_gate` returns and whenever
/// `wait_gate` runs; if Dart resolves the gate first, `signal_gate_ok`/
/// `signal_gate_error` would free it before `wait_gate` ever touches it.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_create_gate(handle: usize) -> *mut c_void {
    get_instance(handle)
        .and_then(|instance| instance.create_gate())
        .map(|gate| {
            let raw = gate.raw();
            std::mem::forget(gate);
            raw
        })
        .unwrap_or(std::ptr::null_mut())
}

/// Blocks until `gate` is resolved, returning the status as a raw `i64` (0 =
/// Ok, 1 = Error, 2 = Cancelled). Reclaims the strong reference
/// `boltffi_dart_runtime_create_gate` reserved for this call -- see its
/// docs -- so this never races `signal_gate_ok`/`signal_gate_error`'s own,
/// separate reference regardless of which resolves first.
///
/// # Safety
/// `gate` must be a non-null pointer previously returned by
/// `boltffi_dart_runtime_create_gate` and not yet passed to
/// `boltffi_dart_runtime_wait_gate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boltffi_dart_runtime_wait_gate(gate: *mut c_void) -> i64 {
    let gate = unsafe { Arc::from_raw(gate as *const Gate) };
    gate.wait() as i64
}

/// One shim's opaque hooks allocation (the fast-path and listener function
/// pointers for one callback registration), reference-counted so a
/// concurrent [`boltffi_dart_runtime_release_hooks`] can never free it out
/// from under a dispatch that already holds the pointer. `free_fn` is how
/// the owning shim reclaims the allocation once the last reference drops.
///
/// Also carries the [`CallableInstance`] this registration belongs to, so
/// dispatch resolves both in one lookup instead of two.
pub struct HooksEntry {
    ptr: *mut c_void,
    free_fn: unsafe extern "C" fn(*mut c_void),
    instance: Arc<CallableInstance>,
}

// SAFETY: the pointee is written once, by the shim's `_register` function,
// and never mutated afterwards -- concurrent shared reads from multiple
// threads via cloned `Arc`s are sound. `free_fn` is a stateless C function
// pointer.
unsafe impl Send for HooksEntry {}
unsafe impl Sync for HooksEntry {}

impl HooksEntry {
    /// The raw hooks pointer, valid for as long as this `Arc` is held.
    pub fn ptr(&self) -> *mut c_void {
        self.ptr
    }

    /// The [`CallableInstance`] this hooks registration belongs to.
    pub fn instance(&self) -> &Arc<CallableInstance> {
        &self.instance
    }
}

impl Drop for HooksEntry {
    fn drop(&mut self) {
        unsafe { (self.free_fn)(self.ptr) }
    }
}

type HooksTable = Mutex<HashMap<usize, Arc<HooksEntry>>>;
static HOOKS: OnceLock<HooksTable> = OnceLock::new();

fn hooks_table() -> &'static HooksTable {
    HOOKS.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    /// Per-thread last-hit cache for [`boltffi_dart_runtime_get_hooks_ref`].
    /// Holds a `Weak`, not a strong `Arc`: a thread that calls a callback
    /// once and never again would otherwise pin this cache slot's strong
    /// reference alive indefinitely (until that thread happens to look up
    /// something else, or exits) even after
    /// [`boltffi_dart_runtime_release_hooks`] drops the table's own
    /// reference -- a real leak on long-lived worker threads. A `Weak`
    /// only resolves to `Some` while some other strong reference (the
    /// table's own, or one an in-flight dispatch elsewhere is holding)
    /// keeps the entry alive, and correctly (and cheaply) fails after
    /// release, including if the numeric handle gets reused for a
    /// different registration afterward -- a `Weak` is tied to the
    /// specific allocation it was created from, never to the handle
    /// number, so no separate epoch/staleness tracking is needed either.
    static HOOKS_CACHE: Cell<Option<(usize, Weak<HooksEntry>)>> = const { Cell::new(None) };
}

/// Associates an opaque, shim-owned "hooks" pointer with the same `handle`
/// BoltFFI's callback vtable dispatch already carries as its first C ABI
/// parameter. `instance_handle` must name a live [`CallableInstance`] --
/// generated code always creates one immediately before this.
///
/// # Safety
/// `hooks` must be a pointer `free_hooks` can validly free exactly once,
/// and must not be freed any other way -- this crate calls `free_hooks`
/// itself, once, only after every reference has been dropped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn boltffi_dart_runtime_register_hooks(
    handle: usize,
    instance_handle: usize,
    hooks: *mut c_void,
    free_hooks: unsafe extern "C" fn(*mut c_void),
) {
    let instance = get_instance(instance_handle).unwrap_or_else(|| {
        panic!(
            "boltffi_dart_runtime_register_hooks: instance_handle {instance_handle} does not \
             name a live CallableInstance -- generated code must call \
             boltffi_dart_runtime_create_instance immediately before registering hooks for it"
        )
    });
    let entry = Arc::new(HooksEntry {
        ptr: hooks,
        free_fn: free_hooks,
        instance,
    });
    hooks_table().lock().unwrap().insert(handle, entry);
}

/// Resolves `handle` to its registered hooks, cloning the `Arc` while the
/// table's own lock is held so a concurrent `release_hooks` can't free it
/// out from under the caller. Checks this thread's local cache first; a
/// miss (or a cached entry whose `Weak` no longer upgrades) falls back to
/// the table and refreshes the cache.
///
/// Rust-only API: called by generated Rust shim code, never by Dart.
pub fn boltffi_dart_runtime_get_hooks_ref(handle: usize) -> Option<Arc<HooksEntry>> {
    HOOKS_CACHE.with(|cache| {
        if let Some((cached_handle, weak)) = cache.take()
            && cached_handle == handle
            && let Some(entry) = weak.upgrade()
        {
            cache.set(Some((cached_handle, Arc::downgrade(&entry))));
            return Some(entry);
        }
        // Either a miss, a different handle, or released and fully dropped
        // since it was cached (`weak` then drops here, permanently dead) --
        // fall through to a real lookup.
        let entry = hooks_table().lock().unwrap().get(&handle).cloned()?;
        cache.set(Some((handle, Arc::downgrade(&entry))));
        Some(entry)
    })
}

/// Removes the hooks registered for `handle`. Doesn't necessarily free the
/// pointee immediately -- it stays alive until every outstanding reference
/// is dropped. Safe to call on a handle with nothing registered.
#[unsafe(no_mangle)]
pub extern "C" fn boltffi_dart_runtime_release_hooks(handle: usize) {
    hooks_table().lock().unwrap().remove(&handle);
}

/// Reads back a raw status value received from Dart as a [`CallStatus`].
pub fn call_status_from_raw(value: i64) -> CallStatus {
    CallStatus::from_i64(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI64;

    fn fresh_instance() -> (usize, Arc<CallableInstance>) {
        let handle = boltffi_dart_runtime_create_instance();
        (handle, get_instance(handle).unwrap())
    }

    #[test]
    fn same_thread_is_owner() {
        let (_handle, instance) = fresh_instance();
        assert!(instance.is_owner_thread());
    }

    #[test]
    fn foreign_thread_is_not_owner() {
        let (_handle, instance) = fresh_instance();
        let result = std::thread::spawn(move || instance.is_owner_thread())
            .join()
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn cross_thread_gate_round_trip() {
        let (_handle, instance) = fresh_instance();
        let (tx, rx) = std::sync::mpsc::channel();

        let caller = std::thread::spawn(move || {
            let gate = instance.create_gate().expect("instance is alive");
            tx.send(gate.raw() as usize).unwrap();
            gate.wait()
        });

        let gate_addr = rx.recv().unwrap();
        unsafe { signal_gate_ok(gate_addr as *mut c_void) };

        assert_eq!(caller.join().unwrap(), CallStatus::Ok);
    }

    #[test]
    fn throwing_callback_resolves_as_error_not_a_hang() {
        let (_handle, instance) = fresh_instance();
        let (tx, rx) = std::sync::mpsc::channel();

        let caller = std::thread::spawn(move || {
            let gate = instance.create_gate().expect("instance is alive");
            tx.send(gate.raw() as usize).unwrap();
            gate.wait()
        });

        let gate_addr = rx.recv().unwrap();
        unsafe { signal_gate_error(gate_addr as *mut c_void) };

        assert_eq!(caller.join().unwrap(), CallStatus::Error);
    }

    #[test]
    fn destroy_wakes_outstanding_waiters_instead_of_hanging_them() {
        let (handle, instance) = fresh_instance();
        let (tx, rx) = std::sync::mpsc::channel();

        let waiter = std::thread::spawn(move || {
            let gate = instance.create_gate().expect("instance is alive");
            tx.send(()).unwrap();
            gate.wait()
        });
        rx.recv().unwrap();
        boltffi_dart_runtime_destroy_instance(handle);

        assert_eq!(
            waiter.join().unwrap(),
            CallStatus::Cancelled,
            "a call outstanding at the moment of destroy must resolve Cancelled, not hang"
        );
    }

    #[test]
    fn calls_after_destroy_never_register_and_return_cancelled_immediately() {
        let (handle, instance) = fresh_instance();
        boltffi_dart_runtime_destroy_instance(handle);
        assert!(
            instance.create_gate().is_none(),
            "an instance marked dead must refuse to register new gates"
        );
    }

    #[test]
    fn outstanding_count_survives_soft_destroy_so_drain_can_observe_it() {
        let (handle, instance) = fresh_instance();
        let _gate = instance.create_gate().expect("instance is alive");
        boltffi_dart_runtime_destroy_instance(handle);
        assert_eq!(
            boltffi_dart_runtime_instance_outstanding_count(handle),
            1,
            "soft destroy must not remove the handle from the table"
        );
    }

    #[test]
    fn forget_after_destroy_is_the_real_removal() {
        let (handle, _instance) = fresh_instance();
        boltffi_dart_runtime_destroy_instance(handle);
        boltffi_dart_runtime_forget_instance(handle);
        assert!(get_instance(handle).is_none());
        assert_eq!(boltffi_dart_runtime_instance_outstanding_count(handle), 0);
        boltffi_dart_runtime_destroy_instance(handle);
        boltffi_dart_runtime_forget_instance(handle);
    }

    #[test]
    fn double_forget_from_two_threads_at_once_does_not_double_free() {
        let (handle, _instance) = fresh_instance();
        let a = std::thread::spawn(move || boltffi_dart_runtime_forget_instance(handle));
        let b = std::thread::spawn(move || boltffi_dart_runtime_forget_instance(handle));
        a.join().unwrap();
        b.join().unwrap();
        assert!(get_instance(handle).is_none());
    }

    #[test]
    fn barrier_proves_registration_is_atomic_with_destroy() {
        const N: usize = 16;
        let (handle, instance) = fresh_instance();
        let barrier = Arc::new(std::sync::Barrier::new(N + 1));
        let statuses: Arc<Mutex<Vec<CallStatus>>> = Arc::new(Mutex::new(Vec::new()));

        let workers: Vec<_> = (0..N)
            .map(|_| {
                let instance = instance.clone();
                let barrier = barrier.clone();
                let statuses = statuses.clone();
                std::thread::spawn(move || {
                    let gate = instance.create_gate();
                    barrier.wait();
                    let status = match gate {
                        Some(gate) => gate.wait(),
                        None => CallStatus::Cancelled,
                    };
                    statuses.lock().unwrap().push(status);
                })
            })
            .collect();

        std::thread::spawn(move || {
            barrier.wait();
            boltffi_dart_runtime_destroy_instance(handle);
        });

        for worker in workers {
            worker.join().unwrap();
        }

        let statuses = statuses.lock().unwrap();
        assert_eq!(statuses.len(), N);
        assert!(
            statuses.iter().all(|s| *s == CallStatus::Cancelled),
            "every call provably registered before destroy must resolve Cancelled: {statuses:?}"
        );
    }

    #[test]
    fn stress_many_concurrent_calls_racing_a_destroy_never_hang() {
        const N: i64 = 500;
        let (handle, instance) = fresh_instance();
        let ok_count = Arc::new(AtomicI64::new(0));
        let cancelled_count = Arc::new(AtomicI64::new(0));

        let workers: Vec<_> = (0..N)
            .map(|_| {
                let instance = instance.clone();
                let ok_count = ok_count.clone();
                let cancelled_count = cancelled_count.clone();
                std::thread::spawn(move || match instance.create_gate() {
                    Some(gate) => {
                        let gate_addr = gate.raw() as usize;
                        std::thread::spawn(move || unsafe {
                            signal_gate_ok(gate_addr as *mut c_void)
                        });
                        match gate.wait() {
                            CallStatus::Ok => ok_count.fetch_add(1, Ordering::SeqCst),
                            CallStatus::Cancelled => cancelled_count.fetch_add(1, Ordering::SeqCst),
                            CallStatus::Error => unreachable!("this test never signals error"),
                        };
                    }
                    None => {
                        cancelled_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        std::thread::spawn(move || boltffi_dart_runtime_destroy_instance(handle));

        for worker in workers {
            worker.join().unwrap();
        }

        let total = ok_count.load(Ordering::SeqCst) + cancelled_count.load(Ordering::SeqCst);
        assert_eq!(
            total, N,
            "every call must resolve one way or the other -- none hang"
        );
    }

    #[test]
    fn two_instances_are_independent() {
        let (handle_a, instance_a) = fresh_instance();
        let (_handle_b, instance_b) = fresh_instance();
        assert!(instance_a.is_owner_thread());
        assert!(instance_b.is_owner_thread());
        boltffi_dart_runtime_destroy_instance(handle_a);
        assert!(instance_a.create_gate().is_none());
        assert!(instance_b.create_gate().is_some());
    }

    #[test]
    fn c_surface_owner_thread_round_trip() {
        let handle = boltffi_dart_runtime_create_instance();
        assert!(boltffi_dart_runtime_is_owner_thread(handle));
        let foreign = std::thread::spawn(move || boltffi_dart_runtime_is_owner_thread(handle))
            .join()
            .unwrap();
        assert!(!foreign);
        assert!(!boltffi_dart_runtime_is_owner_thread(handle + 999_999));
    }

    #[test]
    fn c_surface_gate_round_trip_via_extern_fns_only() {
        let handle = boltffi_dart_runtime_create_instance();
        let (tx, rx) = std::sync::mpsc::channel();

        let caller = std::thread::spawn(move || {
            let gate = boltffi_dart_runtime_create_gate(handle);
            assert!(!gate.is_null(), "instance is alive, gate must be created");
            let gate_addr = gate as usize;
            tx.send(gate_addr).unwrap();
            unsafe { boltffi_dart_runtime_wait_gate(gate) }
        });

        let gate_addr = rx.recv().unwrap();
        unsafe { signal_gate_ok(gate_addr as *mut c_void) };

        assert_eq!(caller.join().unwrap(), CallStatus::Ok as i64);
    }

    /// Regression test for the race Codex flagged on the C-callable
    /// surface: `signal_gate_ok`/`signal_gate_error` reclaiming the gate
    /// *before* the caller even reaches `wait_gate` (deterministically
    /// forced here, not left to scheduling luck) must not free memory
    /// `wait_gate` then touches.
    #[test]
    fn c_surface_wait_gate_is_safe_even_if_signaled_before_it_starts() {
        let handle = boltffi_dart_runtime_create_instance();
        let gate = boltffi_dart_runtime_create_gate(handle);
        assert!(!gate.is_null(), "instance is alive, gate must be created");
        let gate_addr = gate as usize;

        // Resolve it right now, deterministically before any wait_gate
        // call exists anywhere -- exactly the window `create_gate`'s
        // reserved second reference exists to survive.
        unsafe { signal_gate_ok(gate_addr as *mut c_void) };

        assert_eq!(
            unsafe { boltffi_dart_runtime_wait_gate(gate_addr as *mut c_void) },
            CallStatus::Ok as i64
        );
    }

    #[test]
    fn c_surface_create_gate_on_dead_instance_returns_null() {
        let handle = boltffi_dart_runtime_create_instance();
        boltffi_dart_runtime_destroy_instance(handle);
        assert!(boltffi_dart_runtime_create_gate(handle).is_null());
    }

    unsafe extern "C" fn free_u64_hooks(ptr: *mut c_void) {
        drop(unsafe { Box::from_raw(ptr as *mut u64) });
    }

    #[test]
    fn c_surface_hooks_registry_round_trip() {
        let handle = boltffi_dart_runtime_create_instance();
        assert!(boltffi_dart_runtime_get_hooks_ref(handle).is_none());

        let hooks_storage: Box<u64> = Box::new(0x1234);
        let hooks_ptr = Box::into_raw(hooks_storage) as *mut c_void;
        unsafe { boltffi_dart_runtime_register_hooks(handle, handle, hooks_ptr, free_u64_hooks) };

        let entry = boltffi_dart_runtime_get_hooks_ref(handle).expect("hooks registered");
        assert_eq!(entry.ptr(), hooks_ptr);
        drop(entry);

        boltffi_dart_runtime_release_hooks(handle);
        assert!(boltffi_dart_runtime_get_hooks_ref(handle).is_none());
    }

    #[test]
    fn c_surface_release_hooks_on_unregistered_handle_is_a_no_op() {
        boltffi_dart_runtime_release_hooks(999_999_999);
    }

    /// Regression test: an earlier version stored a bare `*mut c_void`, so a
    /// dispatch that had already looked up the pointer had no way to stop
    /// `release_hooks` from freeing it out from under it on another thread.
    #[test]
    fn hooks_stay_alive_while_a_reference_is_held_across_a_concurrent_release() {
        let handle = boltffi_dart_runtime_create_instance();
        let hooks_storage: Box<u64> = Box::new(0xdead_beef);
        let hooks_ptr = Box::into_raw(hooks_storage) as *mut c_void;
        unsafe { boltffi_dart_runtime_register_hooks(handle, handle, hooks_ptr, free_u64_hooks) };

        let held = boltffi_dart_runtime_get_hooks_ref(handle).expect("hooks registered");

        boltffi_dart_runtime_release_hooks(handle);
        assert_eq!(unsafe { *(held.ptr() as *const u64) }, 0xdead_beef);
        // A thread with no prior cache entry for this handle -- genuinely
        // independent of `held` -- must not find it once released, even
        // though `held` (representing some other, already in-flight
        // dispatch) is still keeping the allocation alive.
        let found_elsewhere =
            std::thread::spawn(move || boltffi_dart_runtime_get_hooks_ref(handle).is_none())
                .join()
                .unwrap();
        assert!(found_elsewhere);

        drop(held); // only now does `free_u64_hooks` actually run
    }

    #[test]
    fn hooks_entry_instance_matches_the_registered_instance_handle() {
        let hooks_handle = 0xF00D;
        let instance_handle = boltffi_dart_runtime_create_instance();
        let hooks_storage: Box<u64> = Box::new(0);
        let hooks_ptr = Box::into_raw(hooks_storage) as *mut c_void;
        unsafe {
            boltffi_dart_runtime_register_hooks(
                hooks_handle,
                instance_handle,
                hooks_ptr,
                free_u64_hooks,
            )
        };

        let entry = boltffi_dart_runtime_get_hooks_ref(hooks_handle).expect("hooks registered");
        assert!(entry.instance().is_owner_thread());

        let foreign_says_owner = std::thread::spawn({
            let instance = entry.instance().clone();
            move || instance.is_owner_thread()
        })
        .join()
        .unwrap();
        assert!(
            !foreign_says_owner,
            "a background thread is never the owner"
        );

        drop(entry);
        boltffi_dart_runtime_release_hooks(hooks_handle);
    }

    // No test for "register_hooks panics on an unknown instance_handle": the
    // panic aborts the process (extern "C", no unwinding), which would take
    // the whole test binary down with it.

    /// Regression test: the thread-local cache in `get_hooks_ref` must never
    /// serve a stale entry after a numeric handle is released and reused for
    /// a different registration.
    #[test]
    fn cache_does_not_serve_a_stale_entry_after_handle_reuse() {
        let hooks_handle = 0xABCD;

        let instance_a = boltffi_dart_runtime_create_instance();
        let hooks_a: Box<u64> = Box::new(111);
        let hooks_a_ptr = Box::into_raw(hooks_a) as *mut c_void;
        unsafe {
            boltffi_dart_runtime_register_hooks(
                hooks_handle,
                instance_a,
                hooks_a_ptr,
                free_u64_hooks,
            )
        };

        let first = boltffi_dart_runtime_get_hooks_ref(hooks_handle).expect("registered");
        assert_eq!(unsafe { *(first.ptr() as *const u64) }, 111);
        drop(first);

        boltffi_dart_runtime_release_hooks(hooks_handle);

        let instance_b = boltffi_dart_runtime_create_instance();
        let hooks_b: Box<u64> = Box::new(222);
        let hooks_b_ptr = Box::into_raw(hooks_b) as *mut c_void;
        unsafe {
            boltffi_dart_runtime_register_hooks(
                hooks_handle,
                instance_b,
                hooks_b_ptr,
                free_u64_hooks,
            )
        };

        let second = boltffi_dart_runtime_get_hooks_ref(hooks_handle).expect("registered");
        assert_eq!(
            unsafe { *(second.ptr() as *const u64) },
            222,
            "a cached entry from before the release must never be served for the reused handle"
        );

        drop(second);
        boltffi_dart_runtime_release_hooks(hooks_handle);
    }

    #[test]
    fn cache_hits_are_transparent_across_repeated_lookups() {
        let hooks_handle = 0x5A5A;
        let instance_handle = boltffi_dart_runtime_create_instance();
        let hooks: Box<u64> = Box::new(0x99);
        let hooks_ptr = Box::into_raw(hooks) as *mut c_void;
        unsafe {
            boltffi_dart_runtime_register_hooks(
                hooks_handle,
                instance_handle,
                hooks_ptr,
                free_u64_hooks,
            )
        };

        for _ in 0..1000 {
            let entry = boltffi_dart_runtime_get_hooks_ref(hooks_handle).expect("registered");
            assert_eq!(entry.ptr(), hooks_ptr);
        }

        boltffi_dart_runtime_release_hooks(hooks_handle);
        assert!(boltffi_dart_runtime_get_hooks_ref(hooks_handle).is_none());
    }

    /// Regression test: a thread that resolves a handle once (populating
    /// its `HOOKS_CACHE` entry) and never looks anything up again must not
    /// keep the hooks allocation pinned alive forever via that cached
    /// reference -- it must actually free once `release_hooks` drops the
    /// table's own (last) reference, the same as if this thread had never
    /// cached anything at all.
    #[test]
    fn cache_does_not_keep_a_handle_alive_after_the_last_real_reference_drops() {
        static FREED: AtomicI64 = AtomicI64::new(0);
        unsafe extern "C" fn free_and_flag(ptr: *mut c_void) {
            drop(unsafe { Box::from_raw(ptr as *mut u64) });
            FREED.fetch_add(1, Ordering::SeqCst);
        }

        let hooks_handle = 0x6060;
        let instance_handle = boltffi_dart_runtime_create_instance();
        let hooks: Box<u64> = Box::new(0);
        let hooks_ptr = Box::into_raw(hooks) as *mut c_void;
        unsafe {
            boltffi_dart_runtime_register_hooks(
                hooks_handle,
                instance_handle,
                hooks_ptr,
                free_and_flag,
            )
        };

        // Warm this thread's cache, then drop the only other reference we
        // hold directly -- from here on, the table's own entry and this
        // thread's cached `Weak` are the only things that know about it.
        drop(boltffi_dart_runtime_get_hooks_ref(hooks_handle).expect("registered"));

        assert_eq!(FREED.load(Ordering::SeqCst), 0, "not released yet");
        boltffi_dart_runtime_release_hooks(hooks_handle);
        assert_eq!(
            FREED.load(Ordering::SeqCst),
            1,
            "the table drop was the last strong reference -- a Weak-holding \
             cache entry on this thread must not have kept it alive"
        );
    }
}
