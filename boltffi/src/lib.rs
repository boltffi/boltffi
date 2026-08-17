extern crate self as boltffi;

/// Re-exported so generated user-crate stubs and Dart `@Native` bindings
/// can reach the dual-path runtime without a second crate dependency.
#[cfg(feature = "dart")]
#[doc(hidden)]
pub use boltffi_dart_runtime as __dart_runtime;

/// Sync-export enter/leave used by generated `no_mangle` wrappers.
///
/// Real tracking lives in `boltffi_dart_runtime` when the `dart` feature is
/// on; otherwise this is a no-op so non-Dart crates never need
/// `cfg(boltffi_dart)` (which would trip `unexpected_cfgs`).
#[doc(hidden)]
pub mod __dart_sync_ffi {
    #[cfg(feature = "dart")]
    pub use boltffi_dart_runtime::SyncFfiScope;

    #[cfg(not(feature = "dart"))]
    pub struct SyncFfiScope;

    #[cfg(not(feature = "dart"))]
    impl SyncFfiScope {
        #[inline(always)]
        pub fn enter() -> Self {
            Self
        }
    }
}

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
