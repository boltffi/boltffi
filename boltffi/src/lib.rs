extern crate self as boltffi;

/// The generated Dart callback-dispatch shim (see `boltffi_backend`'s
/// `target::dart::render::shim`), staged into `OUT_DIR` by this crate's
/// `build.rs`.
#[cfg(feature = "dart")]
#[doc(hidden)]
#[allow(
    unsafe_op_in_unsafe_fn,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]
mod __dart_shims {
    include!(concat!(env!("OUT_DIR"), "/dart_shims.rs"));
}

/// Re-exported so `boltffi_dart_runtime_create_instance`/`signal_gate_ok`/
/// `signal_gate_error` -- called directly by generated Dart code -- stay on
/// the final link's object list.
#[cfg(feature = "dart")]
#[doc(hidden)]
pub use boltffi_dart_runtime as __dart_runtime;

pub use boltffi_core::{
    ArcFromCallbackHandle, BoxFromCallbackHandle, CallbackForeignType, CallbackHandle,
    CustomFfiConvertible, CustomTypeConversionError, EventSubscription, FfiType, InternedString,
    InternedStringPool, InternedStringRepr, StreamProducer, UnexpectedFfiCallbackError, custom_ffi,
    custom_type, data, default, error, export, ffi_stream, name, skip,
};

/// Defines a static interned-string pool.
///
/// Pool values must be unique so one semantic string cannot have multiple wire IDs.
///
/// ```
/// boltffi::interned_string_pool! {
///     pub BrowserName {
///         Chrome = "Chrome",
///     }
/// }
///
/// let value = boltffi::InternedString::<BrowserName>::from_str("Chrome");
/// assert_eq!(value, BrowserName::CHROME);
/// ```
///
/// ```compile_fail
/// boltffi::interned_string_pool! {
///     pub BrowserName {
///         CHROME = "Chrome",
///         CHROMIUM = "Chrome",
///     }
/// }
/// ```
pub use boltffi_core::interned_string_pool;

#[doc(hidden)]
pub mod __private {
    pub use boltffi_core::{
        ArcFromCallbackHandle, AsyncCallback, AsyncCallbackString, AsyncCallbackVoid,
        BoxFromCallbackHandle, CallbackForeignType, CallbackHandle, EventSubscription, FfiBuf,
        FfiSpan, FfiStatus, ForeignCall, InternedString, InternedStringPool, InternedStringRepr,
        NativeCallbackOwner, Passable, RustFutureContinuationCallback, RustFutureHandle,
        StreamContinuationCallback, StreamPollResult, SubscriptionHandle,
        UnexpectedFfiCallbackError, UnexpectedFfiCallbackPayload, VecTransport, WaitResult,
        WirePassable, rustfuture, set_last_error, set_last_error_debug, set_last_error_display,
        set_last_error_len, take_last_error, wire,
    };
    #[cfg(target_arch = "wasm32")]
    pub use boltffi_core::{
        AsyncCallbackCompletion, AsyncCallbackCompletionCode, AsyncCallbackCompletionResult,
        AsyncCallbackRegistry, AsyncCallbackRequestGuard, AsyncCallbackRequestId,
        AsyncCallbackWait, WasmCallbackOutBuf, WasmCallbackOwner, rust_future_panic_message,
        rust_future_poll_sync, take_packed_bytes, take_packed_utf8_string, take_return_slot_vec,
        write_option_f64_presence, write_return_slot,
    };
}

#[cfg(test)]
mod interned_string_pool_tests {
    crate::interned_string_pool! {
        InternalBrowserName {
            Chrome = "Chrome",
        }
    }

    #[test]
    fn expands_with_the_facade_crate_self_alias() {
        let value = crate::InternedString::<InternalBrowserName>::from_str("Chrome");
        assert_eq!(value, InternalBrowserName::CHROME);
    }
}
