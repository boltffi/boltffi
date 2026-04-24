//! A named, typed, wire-encoded field. Used by both
//! [`CSharpRecord`](super::CSharpRecord) and
//! [`CSharpEnumVariant`](super::CSharpEnumVariant) because a data-enum
//! variant payload is structurally identical to a record field: same
//! name, same [`CSharpType`], same pre-rendered decode/size/encode
//! expressions. One type, two consumers.

use super::CSharpType;

/// A named field carrying a type and the three wire expressions
/// (decode, size, encode) pre-rendered by the lowerer so the template
/// pastes them verbatim.
#[derive(Debug, Clone)]
pub struct CSharpField {
    /// PascalCase property name (e.g., `"X"`). Records and data-enum
    /// variant payloads both expose their fields as PascalCase
    /// properties, matching idiomatic C# record syntax.
    pub name: String,
    /// C# type of the field.
    pub csharp_type: CSharpType,
    /// Expression that decodes this field from a `WireReader`
    /// (e.g., `"reader.ReadF64()"` or `"Point.Decode(reader)"`).
    pub wire_decode_expr: String,
    /// Expression that produces the wire-encoded byte size of this field
    /// (e.g., `"8"`, `"WireWriter.StringWireSize(this.Name)"`).
    pub wire_size_expr: String,
    /// Statement that writes this field to a `WireWriter` named `wire`
    /// (e.g., `"wire.WriteF64(this.X)"`).
    pub wire_encode_expr: String,
}
