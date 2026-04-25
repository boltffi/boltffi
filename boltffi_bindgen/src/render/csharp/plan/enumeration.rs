//! [`CSharpEnumPlan`] and its variants. A Rust enum lifts into two shapes:
//! C-style (all unit variants) renders as a native C# `enum` and rides
//! P/Invoke as its integral backing type; data (at least one payload)
//! renders as an `abstract record` hierarchy and travels wire-encoded.
//! [`CSharpEnumKind`] carries that choice; [`CSharpEnumVariantPlan`] holds
//! the per-variant payload using [`CSharpFieldPlan`](super::CSharpFieldPlan),
//! the same type record fields use.

use super::super::ast::{CSharpClassName, CSharpEnumUnderlyingType};
use super::{CSharpFieldPlan, CSharpMethodPlan};

/// A Rust enum lifted into the C# type surface. C-style enums (all unit
/// variants) render as native `enum` declarations and ride the CLR's
/// transparent int-marshaling; data enums render as `abstract record`
/// hierarchies and travel wire-encoded.
#[derive(Debug, Clone)]
pub struct CSharpEnumPlan {
    /// Class name (e.g., `"Shape"`, `"Status"`).
    pub class_name: CSharpClassName,
    /// Companion static class holding the wire codec (`Decode` and the
    /// `WireEncodeTo` extension method) for a C-style enum. Always
    /// populated; only referenced by the c-style template.
    pub wire_class_name: CSharpClassName,
    /// Companion static class hosting methods declared on a C-style
    /// enum (since C# enums can't carry members themselves). `None`
    /// when the enum has no methods, or for data enums (whose methods
    /// live on the abstract record directly).
    pub methods_class_name: Option<CSharpClassName>,
    /// Whether this is a C-style or data enum. Drives the rendering shape.
    pub kind: CSharpEnumKind,
    /// For C-style enums, the C# integral type that follows the `:` in
    /// `enum Foo : byte`. `None` for data enums, whose public surface is a
    /// reference type with no underlying base.
    pub underlying_type: Option<CSharpEnumUnderlyingType>,
    /// Variants, in declaration order. The wire tag is the variant's index
    /// in this list (per `EnumTagStrategy::OrdinalIndex`), so order is
    /// load-bearing.
    pub variants: Vec<CSharpEnumVariantPlan>,
    /// Methods and factory constructors declared via `#[data(impl)]`. For
    /// C-style enums these render in [`Self::methods_class_name`]; for
    /// data enums they go directly on the abstract record. The Rust IR
    /// separates constructors from methods, but at the C# call site
    /// they're both just static or instance methods, merged into one
    /// list here.
    pub methods: Vec<CSharpMethodPlan>,
}

/// The two flavors the enum renderer knows how to produce. The `#[repr]`
/// type could inform the C# backing type of a C-style enum, but for now
/// we always use `int`, which matches the i32 wire tag and keeps the DllImport
/// signatures uniform with the free-function enum param/return shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpEnumKind {
    /// Every variant is a unit variant. Renders as `public enum Name : int`
    /// plus a `NameWire` static helper class with `Decode` and a
    /// `WireEncodeTo` extension method for when the enum embeds inside a
    /// wire-encoded record.
    CStyle,
    /// At least one variant carries fields. Renders as
    /// `public abstract record Name` with nested `sealed record` variants
    /// and switch-expression wire codec.
    Data,
}

/// One variant of a [`CSharpEnumPlan`]. For C-style enums, `fields` is always
/// empty; for data enums, a unit variant also has empty `fields` (and
/// renders as `sealed record Name() : Enum`).
#[derive(Debug, Clone)]
pub struct CSharpEnumVariantPlan {
    /// Variant name — for data enums this becomes the nested
    /// `sealed record` class name; for C-style enums it's the enum
    /// member identifier.
    pub name: CSharpClassName,
    /// Numeric value rendered in the *public* surface. For C-style enums
    /// this is the Rust discriminant (`HttpCode.NotFound = 404`), so
    /// client code reading or comparing the enum sees real values, not
    /// ordinals. For data-enum variants this equals `wire_tag`; their
    /// public surface is a `sealed record`, not a numbered enum member,
    /// and only the codec uses the value.
    pub tag: i32,
    /// Ordinal index on the wire (0, 1, 2…), matching
    /// `EnumTagStrategy::OrdinalIndex`. Every boltffi backend wire-encodes
    /// C-style and data enums alike as a 4-byte little-endian `i32` of
    /// this tag, so C# must too, even for enums whose public `tag`
    /// diverges from their declaration order (gapped or negative
    /// discriminants). Keeping `wire_tag` separate from `tag` makes the
    /// two concepts explicit instead of hoping they'll always match.
    pub wire_tag: i32,
    /// Variant fields. Empty for unit variants and for every C-style
    /// variant.
    pub fields: Vec<CSharpFieldPlan>,
}

impl CSharpEnumPlan {
    /// Unwraps [`Self::underlying_type`] for the c-style enum template,
    /// which only renders for c-style enums and so always sees `Some`.
    /// Panics on data enums by design.
    pub fn c_style_underlying_type(&self) -> &CSharpEnumUnderlyingType {
        self.underlying_type
            .as_ref()
            .expect("c_style_underlying_type called on data enum")
    }

    /// Whether any variant payload field's type contains a string at any
    /// nesting depth. Drives the `using System.Text;` import in the data
    /// enum template, needed because string-valued wire-size expressions
    /// call `Encoding.UTF8.GetByteCount(...)`, which lives in
    /// `System.Text`.
    pub fn has_string_fields(&self) -> bool {
        self.variants
            .iter()
            .flat_map(|v| v.fields.iter())
            .any(|f| f.csharp_type.contains_string())
    }
}

impl CSharpEnumVariantPlan {
    /// Whether this variant carries no payload. True for every C-style
    /// variant, and for data enum "unit" variants like `Shape::Point`.
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::ast::{
        CSharpArgumentList, CSharpExpression, CSharpIdentity, CSharpLiteral, CSharpLocalName,
        CSharpMethodName, CSharpPropertyName, CSharpStatement, CSharpType,
    };

    /// A variant with no payload fields is a unit: true for every C-style
    /// variant and for data-enum unit variants like `Shape::Point`.
    #[test]
    fn variant_with_empty_fields_is_unit() {
        let variant = CSharpEnumVariantPlan {
            name: CSharpClassName::from_source("active"),
            tag: 0,
            wire_tag: 0,
            fields: vec![],
        };
        assert!(variant.is_unit());
    }

    /// A variant with at least one payload field is not a unit. The
    /// renderer emits a positional `sealed record Foo(double Radius)`
    /// rather than the empty-paren `sealed record Foo()` shape.
    #[test]
    fn variant_with_payload_is_not_unit() {
        let variant = CSharpEnumVariantPlan {
            name: CSharpClassName::from_source("circle"),
            tag: 0,
            wire_tag: 0,
            fields: vec![CSharpFieldPlan {
                name: CSharpPropertyName::from_source("radius"),
                csharp_type: CSharpType::Double,
                wire_decode_expr: CSharpExpression::MethodCall {
                    receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                        CSharpLocalName::new("reader"),
                    ))),
                    method: CSharpMethodName::from_source("read_f64"),
                    type_args: vec![],
                    args: CSharpArgumentList::default(),
                },
                wire_size_expr: CSharpExpression::Literal(CSharpLiteral::Int(8)),
                wire_encode_stmts: vec![CSharpStatement::Expression(CSharpExpression::Literal(
                    CSharpLiteral::Int(0),
                ))],
            }],
        };
        assert!(!variant.is_unit());
    }
}
