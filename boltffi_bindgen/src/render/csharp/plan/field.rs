use super::super::ast::{CSharpExpression, CSharpPropertyName, CSharpStatement, CSharpType};

/// A single field of a generated C# record, or one payload slot of a data-enum variant.
///
/// Examples:
/// ```csharp
/// // Record fields
/// public readonly record struct Point(
///     double X,
/// //  ^^^^^^^^
///     double Y
/// //  ^^^^^^^^
/// );
///
/// // Data-enum variant payload
/// public sealed record Circle(
///     double Radius
/// //  ^^^^^^^^^^^^^
/// ) : Shape;
/// ```
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
    /// Statements that write this field to a `WireWriter` named `wire`
    /// (e.g., `wire.WriteF64(this.X);`).
    pub wire_encode_stmts: Vec<CSharpStatement>,
}
