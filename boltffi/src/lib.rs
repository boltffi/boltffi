pub use boltffi_core::{
    CallbackForeignType, CallbackHandle, CustomFfiConvertible, CustomTypeConversionError, Data, EventSubscription, FfiStream, FfiType, FromCallbackHandle, StreamProducer,
    UnexpectedFfiCallbackError, custom_ffi, custom_type, data, default, error, export, ffi_async_iter, ffi_stream, name, skip,
};

#[doc(hidden)]
pub mod __private {
    #[cfg(target_arch = "wasm32")]
    pub use boltffi_core::{
        AsyncCallbackCompletionCode, CallbackRequestId, CompleteResult, CompletionPayload, RequestGuard, WasmCallbackOutBuf, WasmCallbackOwner, allocate_request, cancel_request,
        complete_request, complete_request_from_ffi, remove_request, rust_future_panic_message, rust_future_poll_sync, set_request_waker, take_request_result, write_return_slot,
    };
    pub use boltffi_core::{
        CallbackForeignType, CallbackHandle, EventSubscription, FfiBuf, FfiStatus, FromCallbackHandle, IteratorHandle, RustFutureContinuationCallback, RustFutureHandle,
        StreamContinuationCallback, StreamPollResult, SubscriptionHandle, WaitResult, iterator_free, iterator_new, iterator_next, rustfuture, set_last_error, wire,
    };
}
