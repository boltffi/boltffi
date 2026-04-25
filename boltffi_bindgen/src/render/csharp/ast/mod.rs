//! Pure C# AST. Each type's [`fmt::Display`](std::fmt::Display)
//! produces standalone C# source. No FFI concepts, no plan vocabulary,
//! no awareness of anything downstream of this module.
//!
//! Constructors that lift from the IR (`CSharpType::from_read_op`,
//! `From<&RecordId> for CSharpClassName`, etc.) live next to the
//! types they produce because the result *is* a C# AST node; the
//! input vocabulary is the IR, which sits upstream of every render
//! backend. ast/ depends on the IR; nothing in render/csharp/ outside
//! of ast/ depends in the other direction.

mod argument_list;
mod attribute;
mod code;
mod enum_underlying_type;
mod identifier;
mod parameter_list;
mod type_shape;

pub use argument_list::CSharpArgumentList;
pub use attribute::{CSharpAttribute, CSharpAttributeArg};
pub use code::{
    CSharpBinaryOp, CSharpExpression, CSharpIdent, CSharpLiteral, CSharpLocalDecl, CSharpStatement,
};
pub use enum_underlying_type::CSharpEnumUnderlyingType;
pub use identifier::{
    CSharpClassName, CSharpLocalName, CSharpMethodName, CSharpNamespace, CSharpParamName,
    CSharpPropertyName, CSharpTypeReference,
};
pub use parameter_list::{CSharpParameter, CSharpParameterList};
pub use type_shape::CSharpType;
