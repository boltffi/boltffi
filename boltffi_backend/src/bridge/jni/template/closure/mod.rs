//! Source-shaped views for JVM-owned inline closure trampolines.
//!
//! A JVM-owned closure is retained as a global Java object and called from Rust
//! through a C function pointer. The generated source therefore needs a call
//! trampoline, a release trampoline, argument setup, return conversion, and
//! callback-handle helpers for every registered closure signature.
//!
//! This module prepares those template views from the closure contract. The same
//! path is used whether the closure appears as a function parameter, callback
//! method parameter, nested closure argument, or returned closure, so closure
//! rendering cannot drift by declaration kind.

mod argument;
mod callback_handle;
mod registration;

pub use argument::{
    ClosureBytesArgumentView, ClosureCParameterView, ClosureDirectVectorArgumentView,
    ClosureHandleArgumentView, ClosureRecordArgumentView,
};
pub use callback_handle::CallbackClosureHandleView;
pub use registration::ClosureRegistrationView;
