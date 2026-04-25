//! [`CSharpRecordPlan`]: a Rust struct exposed as a C# `readonly record
//! struct`. Holds its fields as [`CSharpFieldPlan`](super::CSharpFieldPlan)
//! (the same field type used by data-enum variants) and carries a
//! blittability flag that decides whether the record rides the direct
//! P/Invoke path or goes through wire encoding.

use super::super::ast::CSharpClassName;
use super::CSharpFieldPlan;

/// A record (Rust struct) exposed as a C# `readonly record struct`.
///
/// Each record is emitted to its own `.cs` file. Blittable records (all
/// fields are primitives, layout matches Rust's `#[repr(C)]`) get a
/// `[StructLayout(LayoutKind.Sequential)]` attribute so the CLR passes
/// them directly across the P/Invoke boundary by value, no wire encoding
/// needed. Non-blittable records carry `Decode` / `WireEncodedSize` /
/// `WireEncodeTo` members and travel as wire-encoded buffers.
#[derive(Debug, Clone)]
pub struct CSharpRecordPlan {
    /// Class name (e.g., `"Point"`).
    pub class_name: CSharpClassName,
    /// The record's fields, in declaration order.
    pub fields: Vec<CSharpFieldPlan>,
    /// Whether the record can cross the P/Invoke boundary as a direct
    /// `[StructLayout(Sequential)]` value. True when the Rust type is
    /// `#[repr(C)]` with blittable fields only.
    pub is_blittable: bool,
}

impl CSharpRecordPlan {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Whether the record has at least one field whose type contains a
    /// string at any nesting depth (bare `string`, `string?`, `string[]`,
    /// nested vecs of strings). Used by the record template to decide
    /// whether to import `System.Text` (for `Encoding.UTF8.GetByteCount`).
    /// Required because `TreatWarningsAsErrors` flags unused usings.
    pub fn has_string_fields(&self) -> bool {
        self.fields.iter().any(|f| f.csharp_type.contains_string())
    }
}
