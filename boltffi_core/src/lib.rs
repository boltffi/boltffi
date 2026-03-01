extern crate self as boltffi_core;

#[cfg(feature = "fast-alloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_arch = "wasm32")]
pub mod async_callback;
pub mod async_iterator;
pub mod callback;
pub mod custom_ffi;
pub mod handle;
pub mod pending;
pub mod ringbuffer;
pub mod rustfuture;
pub mod safety;
pub mod status;
pub mod subscription;
pub mod types;
pub mod wasm;
pub mod wire;

#[cfg(target_arch = "wasm32")]
pub use async_callback::{
    AsyncCallbackCompletionCode, CallbackRequestId, CompleteResult, CompletionPayload, RequestGuard, allocate_request, cancel_request, complete_request, complete_request_from_ffi,
    remove_request, set_request_waker, take_request_result,
};
pub use async_iterator::{IteratorHandle, Stream as FfiStream, iterator_free, iterator_new, iterator_next};
pub use boltffi_macros::{Data, FfiType, custom_ffi, custom_type, data, default, error, export, ffi_async_iter, ffi_class, ffi_export, ffi_stream, ffi_trait, name, skip};
#[cfg(target_arch = "wasm32")]
pub use callback::WasmCallbackOwner;
pub use callback::{CallbackForeignType, CallbackHandle, FromCallbackHandle};
pub use custom_ffi::CustomFfiConvertible;
pub use handle::HandleBox;
pub use pending::{CancellationToken, PendingHandle};
pub use ringbuffer::SpscRingBuffer;
pub use rustfuture::{RustFuture, RustFutureContinuationCallback, RustFutureHandle, RustFuturePoll};
#[cfg(target_arch = "wasm32")]
pub use rustfuture::{WasmPollStatus, rust_future_panic_message, rust_future_poll_sync};
pub use safety::catch_ffi_panic;
pub use status::{FfiStatus, clear_last_error, set_last_error, take_last_error};
pub use subscription::{EventSubscription, StreamContinuationCallback, StreamPollResult, StreamProducer, SubscriptionHandle, WaitResult};
pub use types::{FfiBuf, FfiError, FfiOption, FfiSlice, FfiString};
pub use wasm::WASM_ABI_VERSION;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmCallbackOutBuf;
#[cfg(target_arch = "wasm32")]
pub use wasm::write_return_slot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpectedFfiCallbackError(pub String);

impl UnexpectedFfiCallbackError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UnexpectedFfiCallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unexpected ffi callback error: {}", self.0)
    }
}

impl std::error::Error for UnexpectedFfiCallbackError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomTypeConversionError;

impl std::fmt::Display for CustomTypeConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "custom type conversion failed")
    }
}

impl std::error::Error for CustomTypeConversionError {}

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_version_major() -> u32 {
    VERSION_MAJOR
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_version_minor() -> u32 {
    VERSION_MINOR
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_version_patch() -> u32 {
    VERSION_PATCH
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_free_string(string: FfiString) {
    drop(string);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn boltffi_last_error_message(out: *mut FfiString) -> FfiStatus {
    if out.is_null() {
        return FfiStatus::NULL_POINTER;
    }

    match take_last_error() {
        Some(message) => {
            unsafe { *out = FfiString::from(message) };
            FfiStatus::OK
        }
        None => {
            unsafe { *out = FfiString::from("") };
            FfiStatus::OK
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn boltffi_clear_last_error() {
    clear_last_error();
}

pub fn fail_with_error(status: FfiStatus, message: impl Into<String>) -> FfiStatus {
    set_last_error(message);
    status
}
