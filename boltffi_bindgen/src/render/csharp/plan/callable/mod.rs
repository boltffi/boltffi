//! Callables: things you invoke across the ABI. Holds the two callable
//! shapes ([`CSharpFunctionPlan`] top-level, [`CSharpMethodPlan`] on a type)
//! plus the per-parameter vocabulary they both use: [`CSharpParamPlan`] +
//! [`CSharpParamKind`] decide how a value crosses the boundary, and
//! [`CSharpWireWriterPlan`] carries the setup block for wire-encoded
//! record params.

mod async_call;
mod callable_plan;
mod function;
mod method;
mod param;
mod return_kind;

pub use async_call::CSharpAsyncCallPlan;
pub use callable_plan::CSharpCallablePlan;
pub use function::CSharpFunctionPlan;
pub use method::{CSharpMethodPlan, CSharpReceiver};
pub use param::{CSharpParamKind, CSharpParamPlan};
pub(crate) use param::{native_call_arg_list, native_param_list};
pub use return_kind::CSharpReturnKind;

use super::super::ast::{CSharpExpression, CSharpLocalName, CSharpParamName, CSharpStatement};

/// Bookkeeping for a single record param that must be wire-encoded into a
/// `byte[]` before the native call. The template wraps these setup lines
/// in a `using` block so each `WireWriter` is disposed (and its rented
/// buffer recycled) even if the native call throws.
#[derive(Debug, Clone)]
pub struct CSharpWireWriterPlan {
    /// Local holding the `WireWriter` instance.
    pub binding_name: CSharpLocalName,
    /// Local holding the resulting `byte[]`.
    pub bytes_binding_name: CSharpLocalName,
    /// The param this writer encodes, used to correlate with the
    /// corresponding [`CSharpParamPlan`] at render time.
    pub param_name: CSharpParamName,
    /// Expression rendered against the param that returns its
    /// wire-encoded byte size (e.g., `point.WireEncodedSize()`).
    pub size_expr: CSharpExpression,
    /// Statements that write the param's contents into the
    /// `WireWriter` named by `binding_name`. Most params produce a
    /// single statement (`point.WireEncodeTo(_wire_point)`); a
    /// `Vec<T>` param produces two (the length prefix and the
    /// per-element loop).
    pub encode_stmts: Vec<CSharpStatement>,
}
