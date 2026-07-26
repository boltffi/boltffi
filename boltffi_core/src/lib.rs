extern crate self as boltffi_core;

#[cfg(feature = "fast-alloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod callback;
pub mod custom_ffi;
pub mod handle;
pub mod interned_string;
pub mod passable;
pub mod ringbuffer;
pub mod runtime;
pub mod safety;
pub mod status;
pub mod types;
pub mod wasm;
pub mod wire;

pub use boltffi_macros::{
    Data, FfiType, custom_ffi, custom_type, data, default, error, export, ffi_export, ffi_stream,
    ffi_trait, name, skip,
};

/// Defines a static interned-string pool.
///
/// ```
/// boltffi_core::interned_string_pool! {
///     pub BrowserName {
///         Chrome = "Chrome",
///     }
/// }
///
/// let value = boltffi_core::InternedString::<BrowserName>::from_str("Chrome");
/// assert_eq!(value, BrowserName::CHROME);
/// ```
pub use boltffi_macros::interned_string_pool;
#[cfg(target_arch = "wasm32")]
pub use callback::WasmCallbackOwner;
pub use callback::{
    ArcFromCallbackHandle, BoxFromCallbackHandle, CallbackForeignType, CallbackHandle,
    NativeCallbackOwner,
};
pub use custom_ffi::CustomFfiConvertible;
pub use handle::HandleBox;
pub use interned_string::{InternedString, InternedStringPool, InternedStringRepr};
pub use passable::{Passable, VecTransport, WirePassable};
pub use ringbuffer::SpscRingBuffer;
pub use runtime::async_callback;
pub use runtime::async_callback::{
    AsyncCallback, AsyncCallbackCompletion, AsyncCallbackCompletionCode,
    AsyncCallbackCompletionResult, AsyncCallbackRegistry, AsyncCallbackRequestGuard,
    AsyncCallbackRequestId, AsyncCallbackString, AsyncCallbackVoid,
};
pub use runtime::future as rustfuture;
pub use runtime::future::{
    RustFuture, RustFutureContinuationCallback, RustFutureHandle, RustFuturePoll,
};
#[cfg(target_arch = "wasm32")]
pub use runtime::future::{WasmPollStatus, rust_future_panic_message, rust_future_poll_sync};
pub use runtime::pending;
pub use runtime::pending::{CancellationToken, PendingHandle};
pub use runtime::subscription;
pub use runtime::subscription::{
    EventSubscription, StreamContinuationCallback, StreamPollResult, StreamProducer,
    SubscriptionHandle, WaitResult,
};
pub use safety::catch_ffi_panic;
pub use status::{FfiStatus, clear_last_error, set_last_error, take_last_error};

pub use types::{FfiBuf, FfiError, FfiOption, FfiSlice, FfiSpan, FfiString};
pub use wasm::WASM_ABI_VERSION;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmCallbackOutBuf;
#[cfg(target_arch = "wasm32")]
pub use wasm::{
    take_packed_bytes, take_packed_utf8_string, take_return_slot_vec, write_option_f64_presence,
    write_return_slot,
};

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

impl From<UnexpectedFfiCallbackError> for String {
    fn from(error: UnexpectedFfiCallbackError) -> Self {
        error.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomTypeConversionError;

impl std::fmt::Display for CustomTypeConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "custom type conversion failed")
    }
}

impl std::error::Error for CustomTypeConversionError {}

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

#[cfg(test)]
mod interned_string_pool_tests {
    crate::interned_string_pool! {
        InternalBrowserName {
            Chrome = "Chrome",
        }
    }

    #[test]
    fn expands_with_the_core_crate_self_alias() {
        let value = crate::InternedString::<InternalBrowserName>::from_str("Chrome");
        assert_eq!(value, InternalBrowserName::CHROME);
    }
}
