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
    FfiType, custom_ffi, custom_type, data, default, error, export, ffi_stream, name, skip,
    transparent,
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
pub use runtime::ForeignCall;
pub use runtime::async_callback;
pub use runtime::async_callback::{
    AsyncCallback, AsyncCallbackCompletion, AsyncCallbackCompletionCode,
    AsyncCallbackCompletionResult, AsyncCallbackRegistry, AsyncCallbackRequestGuard,
    AsyncCallbackRequestId, AsyncCallbackString, AsyncCallbackVoid, AsyncCallbackWait,
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
pub use status::{
    FfiStatus, clear_last_error, set_last_error, set_last_error_debug, set_last_error_display,
    set_last_error_len, take_last_error,
};

pub use types::{FfiBuf, FfiError, FfiOption, FfiSlice, FfiSpan, FfiString};
pub use wasm::WASM_ABI_VERSION;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmCallbackOutBuf;
#[cfg(target_arch = "wasm32")]
pub use wasm::{
    take_packed_bytes, take_packed_utf8_string, take_return_slot_vec, write_option_f64_presence,
    write_return_slot,
};

/// Describes an error thrown by a foreign callback that was not its declared Rust error type.
///
/// Callback error owners implement `From<UnexpectedFfiCallbackError>` to choose the typed Rust
/// error returned to their caller when a host-language implementation violates that declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpectedFfiCallbackError(pub String);

/// The result of inspecting a foreign callback's error payload.
///
/// This protocol detail is public only so generated code in downstream crates can distinguish
/// declared errors from unexpected host-language errors. Applications should implement
/// `From<UnexpectedFfiCallbackError>` for callback error types instead of using this enum.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnexpectedFfiCallbackPayload {
    /// The payload does not use the unexpected-error envelope and must be decoded as the callback's
    /// declared error type.
    NotUnexpected,
    /// The payload is a valid unexpected-error envelope.
    Unexpected(UnexpectedFfiCallbackError),
    /// The payload starts with the reserved marker but has an unsupported or malformed envelope.
    ///
    /// Generated code treats this as unexpected rather than attempting to decode the bytes as the
    /// declared error type.
    Malformed(UnexpectedFfiCallbackError),
}

impl UnexpectedFfiCallbackError {
    /// Marks an encoded unexpected foreign-callback error.
    ///
    /// This long marker keeps the envelope distinct from ordinary encoded callback errors while
    /// leaving their existing wire representation unchanged.
    #[doc(hidden)]
    pub const WIRE_MARKER: [u8; 16] = *b"BOLTFFI_CALLBACK";

    /// Identifies the unexpected callback error envelope format understood by this runtime.
    #[doc(hidden)]
    pub const WIRE_VERSION: u8 = 1;

    const WIRE_HEADER_LEN: usize = Self::WIRE_MARKER.len() + 1 + core::mem::size_of::<u32>();

    /// Creates an unexpected callback error with the host-language error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Returns the host-language error message.
    pub fn message(&self) -> &str {
        &self.0
    }

    /// Classifies a foreign callback error payload without decoding declared errors heuristically.
    ///
    /// The envelope is the marker, one version byte, a little-endian `u32` UTF-8 byte length, and
    /// exactly that many message bytes. Once the full marker is present, unsupported versions,
    /// truncation, trailing bytes, and invalid UTF-8 are classified as malformed unexpected errors
    /// so generated code never falls through to the declared-error decoder.
    #[doc(hidden)]
    pub fn classify_payload(payload: &[u8]) -> UnexpectedFfiCallbackPayload {
        if !payload.starts_with(&Self::WIRE_MARKER) {
            return UnexpectedFfiCallbackPayload::NotUnexpected;
        }

        if payload.len() < Self::WIRE_HEADER_LEN {
            return Self::malformed_payload("truncated header");
        }

        let version = payload[Self::WIRE_MARKER.len()];
        if version != Self::WIRE_VERSION {
            return Self::malformed_payload(format!("unsupported version {version}"));
        }

        let length_start = Self::WIRE_MARKER.len() + 1;
        let length_end = length_start + core::mem::size_of::<u32>();
        let Ok(length_bytes) = payload[length_start..length_end].try_into() else {
            return Self::malformed_payload("invalid message length");
        };
        let message_len = u32::from_le_bytes(length_bytes) as usize;
        let Some(expected_len) = Self::WIRE_HEADER_LEN.checked_add(message_len) else {
            return Self::malformed_payload("message length overflow");
        };
        if payload.len() != expected_len {
            return Self::malformed_payload("message length mismatch");
        }

        let message = match core::str::from_utf8(&payload[Self::WIRE_HEADER_LEN..]) {
            Ok(message) => message,
            Err(_) => return Self::malformed_payload("message is not valid UTF-8"),
        };
        UnexpectedFfiCallbackPayload::Unexpected(Self::new(message))
    }

    /// Builds a fail-closed classification for a recognized but invalid envelope.
    fn malformed_payload(reason: impl std::fmt::Display) -> UnexpectedFfiCallbackPayload {
        UnexpectedFfiCallbackPayload::Malformed(Self::new(format!(
            "malformed unexpected callback error payload: {reason}"
        )))
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

#[cfg(test)]
mod unexpected_ffi_callback_error_tests {
    use super::{UnexpectedFfiCallbackError, UnexpectedFfiCallbackPayload};

    fn payload(version: u8, message: &[u8]) -> Vec<u8> {
        let mut payload = UnexpectedFfiCallbackError::WIRE_MARKER.to_vec();
        payload.push(version);
        payload.extend_from_slice(&(message.len() as u32).to_le_bytes());
        payload.extend_from_slice(message);
        payload
    }

    #[test]
    fn classifies_valid_unexpected_callback_error_payload() {
        assert_eq!(
            UnexpectedFfiCallbackError::classify_payload(&payload(
                UnexpectedFfiCallbackError::WIRE_VERSION,
                b"ConnectError"
            )),
            UnexpectedFfiCallbackPayload::Unexpected(UnexpectedFfiCallbackError::new(
                "ConnectError"
            ))
        );
    }

    #[test]
    fn leaves_declared_error_payloads_unclassified() {
        let mut near_marker = UnexpectedFfiCallbackError::WIRE_MARKER;
        near_marker[0] ^= 0xff;

        assert_eq!(
            UnexpectedFfiCallbackError::classify_payload(&near_marker),
            UnexpectedFfiCallbackPayload::NotUnexpected
        );
    }

    #[test]
    fn rejects_truncated_unexpected_callback_error_header() {
        assert!(matches!(
            UnexpectedFfiCallbackError::classify_payload(&UnexpectedFfiCallbackError::WIRE_MARKER),
            UnexpectedFfiCallbackPayload::Malformed(_)
        ));
    }

    #[test]
    fn rejects_unknown_unexpected_callback_error_version() {
        assert!(matches!(
            UnexpectedFfiCallbackError::classify_payload(&payload(2, b"ConnectError")),
            UnexpectedFfiCallbackPayload::Malformed(_)
        ));
    }

    #[test]
    fn rejects_unexpected_callback_error_length_mismatch() {
        let mut too_short = payload(UnexpectedFfiCallbackError::WIRE_VERSION, b"ConnectError");
        too_short.pop();
        let mut trailing = payload(UnexpectedFfiCallbackError::WIRE_VERSION, b"ConnectError");
        trailing.push(0);

        assert!(matches!(
            UnexpectedFfiCallbackError::classify_payload(&too_short),
            UnexpectedFfiCallbackPayload::Malformed(_)
        ));
        assert!(matches!(
            UnexpectedFfiCallbackError::classify_payload(&trailing),
            UnexpectedFfiCallbackPayload::Malformed(_)
        ));
    }

    #[test]
    fn rejects_invalid_utf8_unexpected_callback_error_message() {
        assert!(matches!(
            UnexpectedFfiCallbackError::classify_payload(&payload(
                UnexpectedFfiCallbackError::WIRE_VERSION,
                &[0xff]
            )),
            UnexpectedFfiCallbackPayload::Malformed(_)
        ));
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
