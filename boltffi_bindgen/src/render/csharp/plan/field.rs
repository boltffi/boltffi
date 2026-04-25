//! A named, typed, wire-encoded field. Used by both
//! [`CSharpRecordPlan`](super::CSharpRecordPlan) and
//! [`CSharpEnumVariantPlan`](super::CSharpEnumVariantPlan) because a data-enum
//! variant payload is structurally identical to a record field: same
//! name, same [`CSharpType`], same decode/size/encode trees. One
//! type, two consumers.

use super::super::ast::{CSharpExpression, CSharpPropertyName, CSharpStatement, CSharpType};

/// A named field carrying a type and the three wire trees (decode
/// expression, size expression, encode statement) the templates
/// interpolate through [`fmt::Display`](std::fmt::Display).
#[derive(Debug, Clone)]
pub struct CSharpFieldPlan {
    /// Field name as it appears on the generated record or variant.
    pub name: CSharpPropertyName,
    /// C# type of the field.
    pub csharp_type: CSharpType,
    /// Expression that decodes this field from a `WireReader`
    /// (e.g., `reader.ReadF64()` or `Point.Decode(reader)`).
    pub wire_decode_expr: CSharpExpression,
    /// Expression that produces the wire-encoded byte size of this
    /// field (e.g., `8`, `WireWriter.StringWireSize(this.Name)`).
    pub wire_size_expr: CSharpExpression,
    /// Statements that write this field to a `WireWriter` named
    /// `wire`. Most fields produce a single statement; a length-
    /// prefixed encoded array produces two (the length write and
    /// the per-element loop).
    pub wire_encode_stmts: Vec<CSharpStatement>,
}
