use super::super::ast::CSharpClassName;
use super::CSharpFieldPlan;

/// A Rust struct exposed as a C# `readonly record struct`, emitted to its own `.cs` file.
///
/// Examples:
/// ```csharp
/// // Blittable record: crosses P/Invoke by value
/// [StructLayout(LayoutKind.Sequential)]
/// public readonly record struct Point(double X, double Y);
///
/// // Non-blittable record: travels as a wire-encoded buffer
/// public readonly record struct Person(string Name, int Age)
/// {
///     internal static Person Decode(WireReader reader) => ...;
///     internal int WireEncodedSize() => ...;
///     internal void WireEncodeTo(WireWriter wire) { ... }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CSharpRecordPlan {
    /// Class name (e.g., `"Point"`).
    pub class_name: CSharpClassName,
    /// The record's fields, in declaration order.
    pub fields: Vec<CSharpFieldPlan>,
    /// Whether the record is blittable: `#[repr(C)]` Rust layout with all
    /// blittable fields. Blittable records get `[StructLayout(LayoutKind.Sequential)]`
    /// and cross P/Invoke by value; otherwise the record carries
    /// `Decode`/`WireEncodedSize`/`WireEncodeTo` and travels as a wire buffer.
    pub is_blittable: bool,
}

impl CSharpRecordPlan {
    /// True for records with no fields. The template uses this to short-circuit
    /// `WireEncodedSize()` to `0` instead of emitting an empty sum.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Whether any field's type contains a string at any depth. Gates the
    /// `using System.Text;` import in the record template.
    pub fn has_string_fields(&self) -> bool {
        self.fields.iter().any(|f| f.csharp_type.contains_string())
    }
}
