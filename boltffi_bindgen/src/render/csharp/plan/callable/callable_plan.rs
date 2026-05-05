//! Shared template-facing questions for C# callables.

use super::super::super::ast::CSharpType;
use super::async_call::CSharpAsyncCallPlan;
use super::return_kind::{CSharpReturnKind, native_return_type};

/// Common template-facing view of a generated C# callable.
///
/// Top-level functions and methods live in different plan structs because
/// methods have receiver-specific details (`self`, class handles, extension
/// methods). The C# templates still need to ask the same return/async
/// questions of both shapes. This trait keeps that shared rendering vocabulary
/// in one place while each concrete plan owns only its structural fields.
pub trait CSharpCallablePlan {
    /// Async runtime entry points when the callable starts a Rust future.
    /// `None` means the wrapper calls the native function synchronously.
    fn async_call(&self) -> Option<&CSharpAsyncCallPlan>;

    /// Public C# return type before async wrapping. For async callables this is
    /// the `T` in `Task<T>`; for void callables it is `void`.
    fn return_type(&self) -> &CSharpType;

    /// ABI return transport and decode strategy used by native declarations
    /// and wrapper completion bodies.
    fn return_kind(&self) -> &CSharpReturnKind;

    /// Whether the public wrapper should render async overloads.
    fn is_async(&self) -> bool {
        self.async_call().is_some()
    }

    /// Whether an async wrapper should return `Task<T>` instead of `Task`.
    fn returns_task_value(&self) -> bool {
        !self.return_type().is_void()
    }

    /// Public return type for async overloads: either `Task` or `Task<T>`.
    fn task_return_type(&self) -> String {
        if self.returns_task_value() {
            format!(
                "global::System.Threading.Tasks.Task<{}>",
                self.return_type()
            )
        } else {
            "global::System.Threading.Tasks.Task".to_string()
        }
    }

    /// Return type for the native complete/sync DllImport declaration.
    /// Wire-decoded values cross the boundary as `FfiBuf`; direct values use
    /// their public C# type.
    fn native_return_type(&self) -> String {
        native_return_type(self.return_type(), self.return_kind())
    }
}
