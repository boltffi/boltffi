use std::{
    cell::UnsafeCell,
    future::Future,
    pin::Pin,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicPtr, AtomicU8, AtomicU64, Ordering},
    },
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmPollStatus {
    Pending = 0,
    Ready = 1,
    Cancelled = -1,
    Panicked = -2,
}

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn __boltffi_wake(handle: u32);
}

#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustFuturePoll {
    Ready = 0,
    MaybeReady = 1,
}

pub type RustFutureContinuationCallback = extern "C" fn(callback_data: u64, RustFuturePoll);

#[derive(Clone, Copy)]
struct ContinuationCallback(RustFutureContinuationCallback);

impl ContinuationCallback {
    fn from_raw_ptr(ptr: *mut ()) -> Option<Self> {
        (!ptr.is_null()).then(|| Self(unsafe { std::mem::transmute::<*mut (), RustFutureContinuationCallback>(ptr) }))
    }

    fn into_raw_ptr(self) -> *mut () {
        self.0 as *mut ()
    }

    fn invoke(self, callback_data: ContinuationData, poll_result: RustFuturePoll) {
        (self.0)(callback_data.into_raw(), poll_result)
    }
}

#[derive(Clone, Copy, Default)]
struct ContinuationData(u64);

impl ContinuationData {
    fn from_raw(value: u64) -> Self {
        Self(value)
    }

    fn into_raw(self) -> u64 {
        self.0
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SchedulerStateTag {
    Empty = 0,
    Waked = 1,
    Cancelled = 2,
    ContinuationStored = 3,
}

impl SchedulerStateTag {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Empty,
            1 => Self::Waked,
            2 => Self::Cancelled,
            3 => Self::ContinuationStored,
            _ => Self::Empty,
        }
    }

    fn into_raw(self) -> u8 {
        self as u8
    }
}

struct AtomicContinuationScheduler {
    state_tag: AtomicU8,
    stored_callback_data: AtomicU64,
    stored_callback_ptr: AtomicPtr<()>,
}

impl AtomicContinuationScheduler {
    fn new() -> Self {
        Self {
            state_tag: AtomicU8::new(SchedulerStateTag::Empty.into_raw()),
            stored_callback_data: AtomicU64::new(0),
            stored_callback_ptr: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn current_state(&self) -> SchedulerStateTag {
        SchedulerStateTag::from_raw(self.state_tag.load(Ordering::Acquire))
    }

    fn try_transition(&self, from: SchedulerStateTag, to: SchedulerStateTag) -> bool {
        self.state_tag.compare_exchange(from.into_raw(), to.into_raw(), Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    fn load_stored_continuation(&self) -> (Option<ContinuationCallback>, ContinuationData) {
        let callback_ptr = self.stored_callback_ptr.load(Ordering::Acquire);
        let callback_data = ContinuationData::from_raw(self.stored_callback_data.load(Ordering::Acquire));
        (ContinuationCallback::from_raw_ptr(callback_ptr), callback_data)
    }

    fn write_continuation(&self, callback: ContinuationCallback, callback_data: ContinuationData) {
        self.stored_callback_data.store(callback_data.into_raw(), Ordering::Release);
        self.stored_callback_ptr.store(callback.into_raw_ptr(), Ordering::Release);
    }

    fn invoke_stored_continuation(&self, poll_result: RustFuturePoll) {
        let (callback, callback_data) = self.load_stored_continuation();
        if let Some(continuation_callback) = callback {
            continuation_callback.invoke(callback_data, poll_result);
        }
    }

    fn store_continuation(&self, continuation_callback: ContinuationCallback, callback_data: ContinuationData) {
        loop {
            match self.current_state() {
                SchedulerStateTag::Empty => {
                    self.write_continuation(continuation_callback, callback_data);
                    if self.try_transition(SchedulerStateTag::Empty, SchedulerStateTag::ContinuationStored) {
                        return;
                    }
                }
                SchedulerStateTag::ContinuationStored => {
                    self.invoke_stored_continuation(RustFuturePoll::Ready);
                    self.write_continuation(continuation_callback, callback_data);
                    return;
                }
                SchedulerStateTag::Waked => {
                    if self.try_transition(SchedulerStateTag::Waked, SchedulerStateTag::Empty) {
                        continuation_callback.invoke(callback_data, RustFuturePoll::MaybeReady);
                        return;
                    }
                }
                SchedulerStateTag::Cancelled => {
                    continuation_callback.invoke(callback_data, RustFuturePoll::Ready);
                    return;
                }
            }
        }
    }

    fn wake_continuation(&self) {
        loop {
            match self.current_state() {
                SchedulerStateTag::ContinuationStored => {
                    if self.try_transition(SchedulerStateTag::ContinuationStored, SchedulerStateTag::Empty) {
                        self.invoke_stored_continuation(RustFuturePoll::MaybeReady);
                        return;
                    }
                }
                SchedulerStateTag::Empty => {
                    if self.try_transition(SchedulerStateTag::Empty, SchedulerStateTag::Waked) {
                        return;
                    }
                }
                SchedulerStateTag::Waked | SchedulerStateTag::Cancelled => return,
            }
        }
    }

    fn mark_cancelled(&self) {
        loop {
            let current_state = self.current_state();
            match current_state {
                SchedulerStateTag::ContinuationStored => {
                    if self.try_transition(SchedulerStateTag::ContinuationStored, SchedulerStateTag::Cancelled) {
                        self.invoke_stored_continuation(RustFuturePoll::Ready);
                        return;
                    }
                }
                _ => {
                    if self.try_transition(current_state, SchedulerStateTag::Cancelled) {
                        return;
                    }
                }
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.current_state() == SchedulerStateTag::Cancelled
    }
}

unsafe impl Send for AtomicContinuationScheduler {}
unsafe impl Sync for AtomicContinuationScheduler {}

#[derive(Debug)]
pub enum TerminalState {
    Ready,
    Cancelled,
    Panicked(String),
}

#[allow(dead_code)]
enum CompletionState<T> {
    Running,
    Complete(T),
    Panicked(String),
    Consumed,
}

impl<T> CompletionState<T> {
    fn is_finished(&self) -> bool {
        matches!(self, Self::Complete(_) | Self::Panicked(_) | Self::Consumed)
    }

    #[cfg(target_arch = "wasm32")]
    fn is_panicked(&self) -> bool {
        matches!(self, Self::Panicked(_))
    }

    fn take_result(&mut self) -> Option<T> {
        match std::mem::replace(self, Self::Consumed) {
            Self::Complete(result) => Some(result),
            other => {
                *self = other;
                None
            }
        }
    }

    fn take_panic_message(&mut self) -> Option<String> {
        match std::mem::replace(self, Self::Consumed) {
            Self::Panicked(msg) => Some(msg),
            other => {
                *self = other;
                None
            }
        }
    }
}

/// Type-erased header containing vtable function pointers, scheduler, and
/// completion state. The handle points to this struct.
#[repr(C)]
pub struct RustFuture<T: Send + 'static> {
    poll_future: unsafe fn(*const RustFuture<T>, &Waker) -> bool,
    drop_future: unsafe fn(*const RustFuture<T>),
    arc_clone: unsafe fn(*const RustFuture<T>),
    arc_drop: unsafe fn(*const RustFuture<T>),
    continuation_scheduler: AtomicContinuationScheduler,
    completion_state: UnsafeCell<CompletionState<T>>,
}

/// Single heap allocation: vtable header followed by the concrete future.
/// `repr(C)` guarantees `header` sits at offset 0 so a pointer to this struct
/// can be safely cast to `*const RustFuture<T>`.
#[repr(C)]
struct RustFutureAlloc<T: Send + 'static, F> {
    header: RustFuture<T>,
    future: UnsafeCell<Option<F>>,
}

// SAFETY: The UnsafeCell<Option<F>> (future) is accessed exclusively:
// 1. During poll — the scheduler guarantees sequential polling
// 2. During drop_future — called after completion or from the panic handler
// 3. During Drop of RustFutureAlloc — when Arc refcount reaches 0
unsafe impl<T: Send + 'static, F: Send> Sync for RustFutureAlloc<T, F> {}

// ---------------------------------------------------------------------------
// Vtable function implementations
// ---------------------------------------------------------------------------

unsafe fn poll_impl<T: Send + 'static, F: Future<Output = T> + Send + 'static>(header_ptr: *const RustFuture<T>, waker: &Waker) -> bool {
    let alloc_ptr = header_ptr as *const RustFutureAlloc<T, F>;
    let alloc = unsafe { &*alloc_ptr };

    let future_cell = unsafe { &mut *alloc.future.get() };
    let Some(future) = future_cell.as_mut() else {
        return true;
    };

    // SAFETY: The future lives inside Arc<RustFutureAlloc> on the heap and is
    // never moved after creation. We only set it to None after completion.
    let pinned = unsafe { Pin::new_unchecked(future) };
    let mut cx = Context::from_waker(waker);

    match pinned.poll(&mut cx) {
        Poll::Pending => false,
        Poll::Ready(result) => {
            // Drop the future immediately — it's done.
            *future_cell = None;
            unsafe { *alloc.header.completion_state.get() = CompletionState::Complete(result) };
            true
        }
    }
}

unsafe fn drop_impl<T: Send + 'static, F>(header_ptr: *const RustFuture<T>) {
    let alloc_ptr = header_ptr as *const RustFutureAlloc<T, F>;
    let alloc = unsafe { &*alloc_ptr };
    let future_cell = unsafe { &mut *alloc.future.get() };
    *future_cell = None;
}

unsafe fn arc_clone_impl<T: Send + 'static, F>(header_ptr: *const RustFuture<T>) {
    unsafe { Arc::increment_strong_count(header_ptr as *const RustFutureAlloc<T, F>) };
}

unsafe fn arc_drop_impl<T: Send + 'static, F>(header_ptr: *const RustFuture<T>) {
    unsafe { Arc::decrement_strong_count(header_ptr as *const RustFutureAlloc<T, F>) };
}

// ---------------------------------------------------------------------------
// Waker vtable — delegates Arc ops through header function pointers
// ---------------------------------------------------------------------------

impl<T: Send + 'static> RustFuture<T> {
    const WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(Self::waker_clone, Self::waker_wake, Self::waker_wake_by_ref, Self::waker_drop);
    #[cfg(target_arch = "wasm32")]
    const WASM_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(Self::wasm_waker_clone, Self::wasm_waker_wake, Self::wasm_waker_wake_by_ref, Self::wasm_waker_drop);

    unsafe fn waker_clone(data: *const ()) -> RawWaker {
        let header = data as *const RustFuture<T>;
        unsafe { ((*header).arc_clone)(header) };
        RawWaker::new(data, &Self::WAKER_VTABLE)
    }

    unsafe fn waker_wake(data: *const ()) {
        let header = data as *const RustFuture<T>;
        unsafe { (*header).continuation_scheduler.wake_continuation() };
        // Consume the waker's Arc ref.
        unsafe { ((*header).arc_drop)(header) };
    }

    unsafe fn waker_wake_by_ref(data: *const ()) {
        let header = data as *const RustFuture<T>;
        unsafe { (*header).continuation_scheduler.wake_continuation() };
    }

    unsafe fn waker_drop(data: *const ()) {
        let header = data as *const RustFuture<T>;
        unsafe { ((*header).arc_drop)(header) };
    }

    #[cfg(target_arch = "wasm32")]
    unsafe fn wasm_waker_clone(data: *const ()) -> RawWaker {
        let header = data as *const RustFuture<T>;
        unsafe { ((*header).arc_clone)(header) };
        RawWaker::new(data, &Self::WASM_WAKER_VTABLE)
    }

    #[cfg(target_arch = "wasm32")]
    unsafe fn wasm_waker_wake(data: *const ()) {
        let handle = data as u32;
        unsafe { __boltffi_wake(handle) };
        let header = data as *const RustFuture<T>;
        unsafe { ((*header).arc_drop)(header) };
    }

    #[cfg(target_arch = "wasm32")]
    unsafe fn wasm_waker_wake_by_ref(data: *const ()) {
        let handle = data as u32;
        unsafe { __boltffi_wake(handle) };
    }

    #[cfg(target_arch = "wasm32")]
    unsafe fn wasm_waker_drop(data: *const ()) {
        let header = data as *const RustFuture<T>;
        unsafe { ((*header).arc_drop)(header) };
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_waker_from_header<T: Send + 'static>(header: *const RustFuture<T>) -> Waker {
    // Clone the Arc for the waker's ownership.
    unsafe { ((*header).arc_clone)(header) };
    let raw_waker = RawWaker::new(header as *const (), &RustFuture::<T>::WAKER_VTABLE);
    unsafe { Waker::from_raw(raw_waker) }
}

#[cfg(target_arch = "wasm32")]
fn create_wasm_waker_from_header<T: Send + 'static>(header: *const RustFuture<T>) -> Waker {
    unsafe { ((*header).arc_clone)(header) };
    let raw_waker = RawWaker::new(header as *const (), &RustFuture::<T>::WASM_WAKER_VTABLE);
    unsafe { Waker::from_raw(raw_waker) }
}

#[cfg(target_arch = "wasm32")]
fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "unknown panic".to_string()
}

// ---------------------------------------------------------------------------
// Public handle type and free functions
// ---------------------------------------------------------------------------

pub type RustFutureHandle = *const core::ffi::c_void;

pub fn rust_future_new<F, T>(future: F) -> RustFutureHandle
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let alloc = Arc::new(RustFutureAlloc {
        header: RustFuture {
            poll_future: poll_impl::<T, F>,
            drop_future: drop_impl::<T, F>,
            arc_clone: arc_clone_impl::<T, F>,
            arc_drop: arc_drop_impl::<T, F>,
            continuation_scheduler: AtomicContinuationScheduler::new(),
            completion_state: UnsafeCell::new(CompletionState::Running),
        },
        future: UnsafeCell::new(Some(future)),
    });
    Arc::into_raw(alloc) as RustFutureHandle
}

pub unsafe fn rust_future_poll<T: Send + 'static>(handle: RustFutureHandle, continuation_callback: RustFutureContinuationCallback, callback_data: u64) {
    let header = handle as *const RustFuture<T>;

    let is_cancelled = unsafe { (*header).continuation_scheduler.is_cancelled() };

    let is_ready = is_cancelled || {
        let waker = create_waker_from_header(header);
        unsafe { ((*header).poll_future)(header, &waker) }
    };

    if is_ready {
        continuation_callback(callback_data, RustFuturePoll::Ready);
    } else {
        unsafe {
            (*header)
                .continuation_scheduler
                .store_continuation(ContinuationCallback(continuation_callback), ContinuationData::from_raw(callback_data));
        }
    }
}

pub unsafe fn rust_future_complete<T: Send + 'static>(handle: RustFutureHandle) -> Option<T> {
    let header = handle as *const RustFuture<T>;
    unsafe { (*(*header).completion_state.get()).take_result() }
}

pub unsafe fn rust_future_cancel<T: Send + 'static>(handle: RustFutureHandle) {
    let header = handle as *const RustFuture<T>;
    unsafe { (*header).continuation_scheduler.mark_cancelled() };
}

pub unsafe fn rust_future_free<T: Send + 'static>(handle: RustFutureHandle) {
    let header = handle as *const RustFuture<T>;
    unsafe {
        (*header).continuation_scheduler.mark_cancelled();
        ((*header).arc_drop)(header);
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rust_future_poll_sync<T: Send + 'static>(handle: RustFutureHandle) -> i32 {
    let header = handle as *const RustFuture<T>;

    if unsafe { (*header).continuation_scheduler.is_cancelled() } {
        return WasmPollStatus::Cancelled as i32;
    }

    let waker = create_wasm_waker_from_header(header);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { ((*header).poll_future)(header, &waker) }));

    match result {
        Ok(true) => {
            let state = unsafe { &*(*header).completion_state.get() };
            if state.is_panicked() {
                WasmPollStatus::Panicked as i32
            } else {
                WasmPollStatus::Ready as i32
            }
        }
        Ok(false) => WasmPollStatus::Pending as i32,
        Err(panic_payload) => {
            let message = panic_payload_to_string(panic_payload);
            // Drop the future to prevent further access after panic.
            unsafe { ((*header).drop_future)(header) };
            unsafe { *(*header).completion_state.get() = CompletionState::Panicked(message) };
            WasmPollStatus::Panicked as i32
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rust_future_panic_message<T: Send + 'static>(handle: RustFutureHandle) -> Option<String> {
    let header = handle as *const RustFuture<T>;
    unsafe { (*(*header).completion_state.get()).take_panic_message() }
}
