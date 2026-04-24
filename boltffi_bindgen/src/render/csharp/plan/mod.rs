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
    CSharpFunction, CSharpMethod, CSharpParam, CSharpParamKind, CSharpReceiver, CSharpReturnKind,
    CSharpWireWriter,
};
pub use enumeration::{CSharpEnum, CSharpEnumKind, CSharpEnumVariant};
pub use field::CSharpField;
pub use identifier::CFunctionName;
pub use module::CSharpModule;
pub use record::CSharpRecord;
