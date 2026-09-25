//! Source-shaped views for callback vtable dispatch into the JVM.
//!
//! Rust calls callback traits through C vtable slots. The generated JNI source
//! turns each slot into a call to a static JVM method, with local JNI setup,
//! argument conversion, return conversion, cleanup, and optional async
//! completion dispatch.
//!
//! This module prepares the template data for that source. It keeps C syntax out
//! of the callback contract while still making the templates consume typed
//! values. Callback ownership, method ids, payload shapes, and completion rules
//! are not recomputed here.

mod argument;
mod closure_return;
mod completion;
mod direct_vector;
mod handle_method;
mod method;
mod registration;

pub use argument::{
    CallbackBytesArgumentView, CallbackClosureArgumentView, CallbackCompletionArgumentView,
    CallbackHandleArgumentView, CallbackRecordArgumentView,
};
pub use closure_return::CallbackClosureReturnView;
pub use completion::CallbackCompletionInvokerView;
pub use direct_vector::CallbackDirectVectorArgumentView;
pub use handle_method::CallbackHandleMethodView;
pub use method::CallbackMethodView;
pub use registration::CallbackRegistrationView;
