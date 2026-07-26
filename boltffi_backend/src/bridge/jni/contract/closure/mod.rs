//! Inline closure signatures owned by the JVM.
//!
//! Inline closures are not named callback traits. They cross the C ABI as a call
//! function, a context handle, and a release function. The actual callable lives
//! on the JVM, so Rust needs a generated trampoline for each closure signature it
//! may call.
//!
//! This module owns that registration table. It deduplicates closure signatures
//! across exported functions, callback methods, nested closure arguments, and
//! returned closures. Each registration records the generated JVM bridge class,
//! cached method ids, C call and release trampolines, argument conversion, and
//! return conversion needed for that exact signature.

mod argument;
mod callback_handle;
mod names;
mod parameter;
mod registration;

pub(crate) use argument::ClosureRecordArgument;
pub use argument::{
    ClosureArgument, ClosureBytesArgument, ClosureCParameter, ClosureDirectVectorArgument,
    ClosureHandleArgument,
};
pub use callback_handle::CallbackClosureHandle;
pub use parameter::ClosureParameter;
pub use registration::ClosureRegistration;
