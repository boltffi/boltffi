//! Typed JNI contract built from the C bridge contract.
//!
//! The C bridge has already decided how Rust is called. It knows exported C
//! functions, grouped parameters, return slots, callback vtables, stream
//! protocol functions, direct records, and owned byte buffers. The JNI bridge
//! does not get to rediscover any of that. Its job is to translate those facts
//! into JVM terms: Java parameter types, JNI descriptors, borrowed array
//! lifetimes, callback method ids, `Java_*` symbols, and cleanup paths tied to
//! `JNIEnv`.
//!
//! This module is the adaptation boundary. It reads the C bridge contract once,
//! validates that every C shape has a JVM representation, and stores the result
//! as typed values. Rendering code consumes those values directly. It does not
//! inspect `TypeRef`, re-walk codec plans, or rebuild parameter groups from raw
//! C fragments.
//!
//! The child modules are split by ownership, not by convenience. `parameter`
//! groups C arguments into Java parameters. `return_value` describes what Java
//! receives. `callback` models named callback traits implemented on the JVM.
//! `closure` models inline closure signatures. `stream` keeps stream protocols
//! together. `record`, `scalar`, `bytes`, and `direct_vector` own the reusable
//! ABI shapes shared by those paths.

mod bridge;
mod bytes;
mod callback;
mod closure;
mod continuation;
mod direct_vector;
mod jni_type;
mod jvm;
mod method;
mod parameter;
mod record;
mod return_value;
mod scalar;
mod stream;
mod success_out;

pub use bridge::JniBridgeContract;
pub use bytes::{BytesParameter, BytesWriteback};
pub use callback::{
    CallbackArgument, CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
    CallbackClosureReturn, CallbackCompletionArgument, CallbackCompletionInvoker,
    CallbackCompletionPayload, CallbackCompletionPayloadValue, CallbackDirectVectorArgument,
    CallbackHandleArgument, CallbackHandleClosureReturn, CallbackHandleCompletion,
    CallbackHandleLifecycle, CallbackHandleMethod, CallbackMethod, CallbackParameter,
    CallbackRecordArgument, CallbackRegistration, CallbackReturn,
};
pub(crate) use closure::ClosureRecordArgument;
pub use closure::{
    CallbackClosureHandle, ClosureArgument, ClosureBytesArgument, ClosureCParameter,
    ClosureDirectVectorArgument, ClosureHandleArgument, ClosureParameter, ClosureRegistration,
};
pub use continuation::ContinuationParameter;
pub use direct_vector::{DirectVectorParameter, DirectVectorStackCopy};
pub use jni_type::JniType;
pub use jvm::JvmMethodReturn;
pub use method::NativeMethod;
pub use parameter::{NativeParameter, NativeParameterKind};
pub use record::{RecordParameter, RecordValue};
pub use return_value::{EncodedErrorReturn, NativeReturn, SuccessOutReturn};
pub use scalar::{ScalarParameter, ScalarReturn};
pub use stream::{DirectStreamBatchMethod, StreamProtocolMethods};
pub use success_out::{SuccessOutArgument, SuccessOutValue, SuccessOutWriter};
