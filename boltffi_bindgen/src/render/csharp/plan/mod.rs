//! Plans are the primary data structure used by the `templates` module to produce C# code. Thus,
//! they are the `ViewModel` of the `templates` `View`.
//!
//! Plans consist of `ast` (C# grammar) primitive values, and are structured around the FFI boundary
//! code being generated.
//!
//! Templates contain most of the complexity of the C# syntax, but plans and their `ast` primitives
//! exist only to expose conditional or variable syntax.
//!
//! Plans are created by the `lower` module, and consumed by the `templates` module.

mod callable;
mod class;
mod enumeration;
mod field;
mod identifier;
mod module;
mod record;

pub use callable::{
    CSharpAsyncCallPlan, CSharpCallablePlan, CSharpFunctionPlan, CSharpMethodPlan, CSharpParamKind,
    CSharpParamPlan, CSharpReceiver, CSharpReturnKind, CSharpWireWriterPlan,
};
pub use class::{CSharpClassPlan, CSharpConstructorKind, CSharpConstructorPlan};
pub use enumeration::{CSharpEnumKind, CSharpEnumPlan, CSharpEnumVariantPlan};
pub use field::CSharpFieldPlan;
pub use identifier::CFunctionName;
pub use module::CSharpModulePlan;
pub use record::CSharpRecordPlan;
