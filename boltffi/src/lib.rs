pub use boltffi_core::{
    ArcFromCallbackHandle, BoxFromCallbackHandle, CallbackForeignType, CallbackHandle,
    CustomFfiConvertible, CustomTypeConversionError, Data, EventSubscription, FfiType,
    InternedString, InternedStringPool, InternedStringRepr, StreamProducer,
    UnexpectedFfiCallbackError, custom_ffi, custom_type, data, default, error, export, ffi_stream,
    interned_string_pool, name, skip,
};

#[doc(hidden)]
pub mod __private {
    pub use boltffi_core::{
        ArcFromCallbackHandle, AsyncCallback, AsyncCallbackString, AsyncCallbackVoid,
        BoxFromCallbackHandle, CallbackForeignType, CallbackHandle, EventSubscription, FfiBuf,
        FfiSpan, FfiStatus, InternedString, InternedStringPool, InternedStringRepr,
        NativeCallbackOwner, Passable, RustFutureContinuationCallback, RustFutureHandle,
        StreamContinuationCallback, StreamPollResult, SubscriptionHandle, VecTransport, WaitResult,
        WirePassable, rustfuture, set_last_error, take_last_error, wire,
    };
    #[cfg(target_arch = "wasm32")]
    pub use boltffi_core::{
        AsyncCallbackCompletion, AsyncCallbackCompletionCode, AsyncCallbackCompletionResult,
        AsyncCallbackRegistry, AsyncCallbackRequestGuard, AsyncCallbackRequestId,
        WasmCallbackOutBuf, WasmCallbackOwner, rust_future_panic_message, rust_future_poll_sync,
        take_packed_bytes, take_packed_utf8_string, take_return_slot_vec, write_return_slot,
    };
}
