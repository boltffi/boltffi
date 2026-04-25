//! C# language syntax primitives, that one would read in a textbook on the C# Programming Language,
//! and as such does not contain any reference to FFI.
//!
//! Rendering occurs through the `templates` module, so a full AST is not needed, so here we create
//! enough primitives to be able to reason about what code will be generated, in C# language terms,
//! as we create a plan.
//!
//! Ultimately all of these elements implement `Display`, and so are available in the `plan` module
//! directly by the templates for direct rendering.
mod argument_list;
mod attribute;
mod code;
mod enum_underlying_type;
mod identifier;
mod parameter_list;
mod type_shape;

pub(super) use argument_list::CSharpArgumentList;
pub(super) use attribute::{CSharpAttribute, CSharpAttributeArg};
pub(super) use code::{
    CSharpBinaryOp, CSharpExpression, CSharpIdentity, CSharpLiteral, CSharpLocalDecl,
    CSharpStatement,
};
pub(super) use enum_underlying_type::CSharpEnumUnderlyingType;
pub(super) use identifier::{
    CSharpClassName, CSharpLocalName, CSharpMethodName, CSharpNamespace, CSharpParamName,
    CSharpPropertyName, CSharpTypeReference,
};
pub(super) use parameter_list::{CSharpParameter, CSharpParameterList};
pub(super) use type_shape::CSharpType;
