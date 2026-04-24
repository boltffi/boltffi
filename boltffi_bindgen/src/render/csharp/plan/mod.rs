//! View model for the C# backend: the data shapes the templates
//! consume. `CSharpType` is the central vocabulary: every record
//! field, param, return, and variant field resolves to one. All wire
//! expressions (decode, size, encode) are pre-rendered strings
//! produced by the lowerer; templates only interpolate.
//!
//! `CSharpType` owns its IR-to-type constructors
//! (`impl From<PrimitiveType>`, `enum_backing_for`, `for_enum`,
//! `from_read_op`, `from_type_expr`), so one place answers "what C#
//! type does this become?".
//!
//! No dependency on `emit` or `lower`: the plan is passive data.
//! `lower` produces it, `templates` consume it.

mod callable;
mod code;
mod enumeration;
mod field;
mod identifier;
mod module;
mod record;
mod type_shape;

pub use callable::{
    CSharpFunction, CSharpMethod, CSharpParam, CSharpParamKind, CSharpReceiver, CSharpReturnKind,
    CSharpWireWriter,
};
pub use code::{
    CSharpBinaryOp, CSharpExpression, CSharpIdent, CSharpLiteral, CSharpLocalDecl, CSharpStatement,
};
pub use enumeration::{CSharpEnum, CSharpEnumKind, CSharpEnumVariant};
pub use field::CSharpField;
pub use identifier::{
    CFunctionName, CSharpClassName, CSharpLocalName, CSharpMethodName, CSharpNamespace,
    CSharpParamName, CSharpPropertyName, CSharpTypeReference,
};
pub use module::CSharpModule;
pub use record::CSharpRecord;
pub use type_shape::CSharpType;
