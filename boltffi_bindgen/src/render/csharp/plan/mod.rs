//! View model for the C# backend: the FFI-shaped data the templates
//! consume. Records, enums, functions, methods, params: each plan type
//! models an FFI shape and carries
//! [`super::ast`](super::ast) values as field payloads.
//! `plan` speaks FFI vocabulary; `ast` speaks C# grammar.
//!
//! No dependency on `emit` or `lower`: the plan is passive data.
//! `lower` produces it, `templates` consume it.

mod callable;
mod enumeration;
mod field;
mod identifier;
mod module;
mod record;

pub use callable::{
    CSharpFunctionPlan, CSharpMethodPlan, CSharpParamPlan, CSharpParamKind, CSharpReceiver, CSharpReturnKind,
    CSharpWireWriterPlan,
};
pub use enumeration::{CSharpEnumPlan, CSharpEnumKind, CSharpEnumVariantPlan};
pub use field::CSharpFieldPlan;
pub use identifier::CFunctionName;
pub use module::CSharpModulePlan;
pub use record::CSharpRecordPlan;
