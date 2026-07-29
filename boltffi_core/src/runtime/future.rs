use std::future::Future;
use std::pin::Pin;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::AtomicU32;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use super::continuation::{ContinuationScheduler, ContinuationSignalPolicy};
use crate::status::FfiStatus;

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

struct RustFutureContinuationPolicy;

impl ContinuationSignalPolicy for RustFutureContinuationPolicy {
    type Signal = RustFuturePoll;

    fn displaced() -> Self::Signal {
        RustFuturePoll::Ready
    }

    fn wake() -> Self::Signal {
        RustFuturePoll::MaybeReady
    }

    fn cancelled() -> Self::Signal {
        RustFuturePoll::Ready
    }
}

struct FutureWakeTarget {
    continuation_scheduler: ContinuationScheduler<RustFutureContinuationPolicy>,
    #[cfg(target_arch = "wasm32")]
    wasm_handle: AtomicU32,
}

impl FutureWakeTarget {
    fn new() -> Self {
        Self {
            continuation_scheduler: ContinuationScheduler::new(),
            #[cfg(target_arch = "wasm32")]
            wasm_handle: AtomicU32::new(0),
        }
    }

    fn store_continuation(
        &self,
        continuation_callback: RustFutureContinuationCallback,
        callback_data: u64,
    ) {
        self.continuation_scheduler
            .store_continuation(continuation_callback, callback_data);
    }

    fn wake_continuation(&self) {
        self.continuation_scheduler.wake();
    }

    fn mark_cancelled(&self) {
        self.continuation_scheduler.cancel();
    }

    fn is_cancelled(&self) -> bool {
        self.continuation_scheduler.is_cancelled()
    }

    #[cfg(target_arch = "wasm32")]
    fn initialize_wasm_handle<T: Send + 'static>(&self, future: &Arc<RustFuture<T>>) {
        self.wasm_handle
            .store(Arc::as_ptr(future) as usize as u32, Ordering::Release);
    }

    #[cfg(target_arch = "wasm32")]
    fn wasm_handle(&self) -> u32 {
        self.wasm_handle.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub enum TerminalState {
    Ready,
    Cancelled,
    Panicked(String),
}

#[allow(dead_code)]
enum FutureExecutionState<T> {
    Running(Pin<Box<dyn Future<Output = T> + Send + 'static>>),
    Complete(T),
    Failed(FfiStatus),
    Panicked(String),
    Consumed,
}

impl<T> FutureExecutionState<T> {
    fn is_finished(&self) -> bool {
        matches!(
            self,
            Self::Complete(_) | Self::Failed(_) | Self::Panicked(_) | Self::Consumed
        )
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

    fn take_status(&mut self) -> Option<FfiStatus> {
        match std::mem::replace(self, Self::Consumed) {
            Self::Failed(status) => Some(status),
            other => {
                *self = other;
                None
            }
        }
    }
}

pub struct RustFuture<T: Send + 'static> {
    future_execution_state: Mutex<FutureExecutionState<T>>,
    wake_target: Arc<FutureWakeTarget>,
}

impl<T: Send + 'static> RustFuture<T> {
    pub fn new<F>(future: F) -> Arc<Self>
    where
        F: Future<Output = T> + Send + 'static,
    {
        Self::from_execution_state(FutureExecutionState::Running(Box::pin(future)))
    }

    fn from_execution_state(future_execution_state: FutureExecutionState<T>) -> Arc<Self> {
        let wake_target = Arc::new(FutureWakeTarget::new());
        let rust_future = Arc::new(Self {
            future_execution_state: Mutex::new(future_execution_state),
            wake_target: Arc::clone(&wake_target),
        });
        #[cfg(target_arch = "wasm32")]
        wake_target.initialize_wasm_handle(&rust_future);
        rust_future
    }

    fn from_status(status: FfiStatus) -> Arc<Self> {
        Self::from_execution_state(FutureExecutionState::Failed(status))
    }

    fn poll_future_once(&self, waker: &Waker) -> bool {
        let mut execution_state_guard = self.future_execution_state.lock().unwrap();

        if execution_state_guard.is_finished() {
            return true;
        }

        let FutureExecutionState::Running(pinned_future) = &mut *execution_state_guard else {
            return true;
        };

        let mut poll_context = Context::from_waker(waker);
        match pinned_future.as_mut().poll(&mut poll_context) {
            Poll::Pending => false,
            Poll::Ready(result) => {
                *execution_state_guard = FutureExecutionState::Complete(result);
                true
            }
        }
    }

    pub fn poll(
        self: &Arc<Self>,
        continuation_callback: RustFutureContinuationCallback,
        callback_data: u64,
    ) {
        let is_cancelled = self.wake_target.is_cancelled();

        let is_ready = is_cancelled || {
            let waker = self.clone().create_waker();
            self.poll_future_once(&waker)
        };

        if is_ready {
            continuation_callback(callback_data, RustFuturePoll::Ready);
        } else {
            self.wake_target
                .store_continuation(continuation_callback, callback_data);
        }
    }

    pub fn complete(&self) -> Result<T, FfiStatus> {
        let mut execution_state = self.future_execution_state.lock().unwrap();
        if let Some(result) = execution_state.take_result() {
            return Ok(result);
        }
        if let Some(status) = execution_state.take_status() {
            return Err(status);
        }
        if matches!(&*execution_state, FutureExecutionState::Panicked(_)) {
            return Err(FfiStatus::INTERNAL_ERROR);
        }
        drop(execution_state);
        if self.wake_target.is_cancelled() {
            Err(FfiStatus::CANCELLED)
        } else {
            Err(FfiStatus::INTERNAL_ERROR)
        }
    }

    pub fn panic_message(&self) -> Option<String> {
        self.future_execution_state
            .lock()
            .unwrap()
            .take_panic_message()
    }

    pub fn cancel(&self) {
        self.wake_target.mark_cancelled();
    }

    pub fn free(self: Arc<Self>) {
        self.wake_target.mark_cancelled();
    }

    #[cfg(target_arch = "wasm32")]
    pub fn poll_sync(self: &Arc<Self>) -> WasmPollStatus {
        if self.wake_target.is_cancelled() {
            return WasmPollStatus::Cancelled;
        }

        let waker = self.clone().create_wasm_waker();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.poll_future_once(&waker)
        }));

        match result {
            Ok(true) => {
                let state = self.future_execution_state.lock().unwrap();
                if state.is_panicked() {
                    WasmPollStatus::Panicked
                } else {
                    WasmPollStatus::Ready
                }
            }
            Ok(false) => WasmPollStatus::Pending,
            Err(panic_payload) => {
                let message = panic_payload_to_string(panic_payload);
                *self.future_execution_state.lock().unwrap() =
                    FutureExecutionState::Panicked(message);
                WasmPollStatus::Panicked
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn create_wasm_waker(self: Arc<Self>) -> Waker {
        let wake_target = Arc::clone(&self.wake_target);
        let handle = Arc::into_raw(wake_target) as *const ();
        let raw_waker = RawWaker::new(handle as *const (), &WASM_WAKER_VTABLE);
        unsafe { Waker::from_raw(raw_waker) }
    }

    fn create_waker(self: Arc<Self>) -> Waker {
        let wake_target = Arc::clone(&self.wake_target);
        let raw_waker = RawWaker::new(
            Arc::into_raw(wake_target) as *const (),
            &RUST_FUTURE_WAKER_VTABLE,
        );
        unsafe { Waker::from_raw(raw_waker) }
    }
}

const RUST_FUTURE_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    wake_target_clone,
    wake_target_wake,
    wake_target_wake_by_ref,
    wake_target_drop,
);

#[cfg(target_arch = "wasm32")]
const WASM_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    wasm_wake_target_clone,
    wasm_wake_target_wake,
    wasm_wake_target_wake_by_ref,
    wasm_wake_target_drop,
);

fn wake_target_clone(waker_data_ptr: *const ()) -> RawWaker {
    unsafe { Arc::increment_strong_count(waker_data_ptr as *const FutureWakeTarget) };
    RawWaker::new(waker_data_ptr, &RUST_FUTURE_WAKER_VTABLE)
}

fn wake_target_wake(waker_data_ptr: *const ()) {
    let wake_target = unsafe { Arc::from_raw(waker_data_ptr as *const FutureWakeTarget) };
    wake_target.wake_continuation();
}

fn wake_target_wake_by_ref(waker_data_ptr: *const ()) {
    let wake_target = unsafe { &*(waker_data_ptr as *const FutureWakeTarget) };
    wake_target.wake_continuation();
}

fn wake_target_drop(waker_data_ptr: *const ()) {
    drop(unsafe { Arc::from_raw(waker_data_ptr as *const FutureWakeTarget) });
}

#[cfg(target_arch = "wasm32")]
fn wasm_wake_target_clone(data: *const ()) -> RawWaker {
    unsafe { Arc::increment_strong_count(data as *const FutureWakeTarget) };
    RawWaker::new(data, &WASM_WAKER_VTABLE)
}

#[cfg(target_arch = "wasm32")]
fn wasm_wake_target_wake(data: *const ()) {
    let wake_target = unsafe { Arc::from_raw(data as *const FutureWakeTarget) };
    unsafe { __boltffi_wake(wake_target.wasm_handle()) };
}

#[cfg(target_arch = "wasm32")]
fn wasm_wake_target_wake_by_ref(data: *const ()) {
    let wake_target = unsafe { &*(data as *const FutureWakeTarget) };
    unsafe { __boltffi_wake(wake_target.wasm_handle()) };
}

#[cfg(target_arch = "wasm32")]
fn wasm_wake_target_drop(data: *const ()) {
    drop(unsafe { Arc::from_raw(data as *const FutureWakeTarget) });
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

pub type RustFutureHandle = *const core::ffi::c_void;

struct RustFutureHandleAccess<T: Send + 'static> {
    future_handle: RustFutureHandle,
    future_type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> RustFutureHandleAccess<T> {
    #[inline]
    fn new(future_handle: RustFutureHandle) -> Self {
        Self {
            future_handle,
            future_type: std::marker::PhantomData,
        }
    }

    #[inline]
    fn with_future<Result>(&self, future_access: impl FnOnce(&RustFuture<T>) -> Result) -> Result {
        let rust_future = unsafe { Arc::from_raw(self.future_handle as *const RustFuture<T>) };
        let result = future_access(&rust_future);
        std::mem::forget(rust_future);
        result
    }

    #[inline]
    fn with_future_arc<Result>(
        &self,
        future_access: impl FnOnce(&Arc<RustFuture<T>>) -> Result,
    ) -> Result {
        let rust_future = unsafe { Arc::from_raw(self.future_handle as *const RustFuture<T>) };
        let result = future_access(&rust_future);
        std::mem::forget(rust_future);
        result
    }

    #[inline]
    fn consume_future<Result>(
        self,
        future_access: impl FnOnce(Arc<RustFuture<T>>) -> Result,
    ) -> Result {
        let rust_future = unsafe { Arc::from_raw(self.future_handle as *const RustFuture<T>) };
        future_access(rust_future)
    }
}

pub fn rust_future_new<F, T>(future: F) -> RustFutureHandle
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    Arc::into_raw(RustFuture::new(future)) as RustFutureHandle
}

pub fn rust_future_invalid_arg<T: Send + 'static>() -> RustFutureHandle {
    Arc::into_raw(RustFuture::<T>::from_status(FfiStatus::INVALID_ARG)) as RustFutureHandle
}

pub unsafe fn rust_future_poll<T: Send + 'static>(
    handle: RustFutureHandle,
    continuation_callback: RustFutureContinuationCallback,
    callback_data: u64,
) {
    RustFutureHandleAccess::<T>::new(handle)
        .with_future_arc(|rust_future| rust_future.poll(continuation_callback, callback_data));
}

pub unsafe fn rust_future_complete<T: Send + 'static>(
    handle: RustFutureHandle,
) -> Result<T, FfiStatus> {
    RustFutureHandleAccess::<T>::new(handle).with_future(RustFuture::complete)
}

pub unsafe fn rust_future_cancel<T: Send + 'static>(handle: RustFutureHandle) {
    RustFutureHandleAccess::<T>::new(handle).with_future(RustFuture::cancel);
}

pub unsafe fn rust_future_free<T: Send + 'static>(handle: RustFutureHandle) {
    RustFutureHandleAccess::<T>::new(handle).consume_future(RustFuture::free);
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rust_future_poll_sync<T: Send + 'static>(handle: RustFutureHandle) -> i32 {
    RustFutureHandleAccess::<T>::new(handle)
        .with_future_arc(|rust_future| rust_future.poll_sync() as i32)
}

pub unsafe fn rust_future_panic_message<T: Send + 'static>(
    handle: RustFutureHandle,
) -> Option<String> {
    RustFutureHandleAccess::<T>::new(handle).with_future(RustFuture::panic_message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    struct DelayedWakeState<T> {
        is_ready: AtomicBool,
        wake_scheduled: AtomicBool,
        output: Mutex<Option<T>>,
        pending_waker: Mutex<Option<Waker>>,
    }

    struct DelayedWakeFuture<T> {
        state: Arc<DelayedWakeState<T>>,
        delay: Duration,
    }

    impl<T: Send + 'static> DelayedWakeFuture<T> {
        fn new(output: T, delay: Duration) -> Self {
            let state = Arc::new(DelayedWakeState {
                is_ready: AtomicBool::new(false),
                wake_scheduled: AtomicBool::new(false),
                output: Mutex::new(Some(output)),
                pending_waker: Mutex::new(None),
            });

            Self { state, delay }
        }
    }

    impl<T: Send + 'static> Future for DelayedWakeFuture<T> {
        type Output = T;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.state.is_ready.load(Ordering::Acquire) {
                Poll::Ready(
                    self.state
                        .output
                        .lock()
                        .unwrap()
                        .take()
                        .expect("delayed wake future output"),
                )
            } else {
                *self.state.pending_waker.lock().unwrap() = Some(cx.waker().clone());
                if self
                    .state
                    .wake_scheduled
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    let delayed_state = Arc::clone(&self.state);
                    let delay = self.delay;
                    thread::spawn(move || {
                        thread::sleep(delay);
                        delayed_state.is_ready.store(true, Ordering::Release);
                        if let Some(waker) = delayed_state.pending_waker.lock().unwrap().take() {
                            waker.wake();
                        }
                    });
                }
                Poll::Pending
            }
        }
    }

    extern "C" fn send_poll_status(callback_data: u64, poll: RustFuturePoll) {
        let sender = unsafe { &*(callback_data as *const mpsc::Sender<RustFuturePoll>) };
        sender.send(poll).unwrap();
    }

    fn callback_data(sender: &mpsc::Sender<RustFuturePoll>) -> u64 {
        sender as *const mpsc::Sender<RustFuturePoll> as usize as u64
    }

    #[test]
    fn delayed_wake_future_completes_through_exported_handle() {
        let (sender, receiver) = mpsc::channel();
        let sender = Box::new(sender);
        let handle = rust_future_new(DelayedWakeFuture::new(
            "boltffi".to_string(),
            Duration::from_millis(10),
        ));

        unsafe {
            rust_future_poll::<String>(handle, send_poll_status, callback_data(sender.as_ref()))
        };
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(RustFuturePoll::MaybeReady)
        );

        unsafe {
            rust_future_poll::<String>(handle, send_poll_status, callback_data(sender.as_ref()))
        };
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(RustFuturePoll::Ready)
        );
        assert_eq!(
            unsafe { rust_future_complete::<String>(handle) },
            Ok("boltffi".to_string())
        );

        unsafe { rust_future_free::<String>(handle) };
    }

    #[test]
    fn invalid_arg_future_returns_invalid_arg_status() {
        let (sender, receiver) = mpsc::channel();
        let sender = Box::new(sender);
        let handle = rust_future_invalid_arg::<String>();

        unsafe {
            rust_future_poll::<String>(handle, send_poll_status, callback_data(sender.as_ref()))
        };
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(RustFuturePoll::Ready)
        );
        assert_eq!(
            unsafe { rust_future_complete::<String>(handle) },
            Err(FfiStatus::INVALID_ARG)
        );

        unsafe { rust_future_free::<String>(handle) };
    }
}
