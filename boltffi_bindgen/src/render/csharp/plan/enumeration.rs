use super::super::ast::{CSharpClassName, CSharpComment, CSharpEnumUnderlyingType};
use super::{CSharpFieldPlan, CSharpMethodPlan};

/// A Rust enum exposed in C# as either a C-style `enum` or a data
/// `abstract record` hierarchy, emitted to its own `.cs` file.
///
/// Examples:
/// ```csharp
/// // C-style enum: every variant is unit
/// public enum HttpCode : int
/// {
///     Ok = 200,
///     NotFound = 404,
/// }
///
/// // Data enum: at least one variant carries fields
/// public abstract record Shape
/// {
///     public sealed record Point() : Shape;
///     public sealed record Circle(double Radius) : Shape;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CSharpEnumPlan {
    /// Renders a `<summary>` block comment, when `Some`.
    pub summary_doc: Option<CSharpComment>,
    /// Class name (e.g., `"HttpCode"`, `"Shape"`).
    pub class_name: CSharpClassName,
    /// Companion static class holding the C-style enum's wire codec
    /// (`Decode` and the `WireEncodeTo` extension method). Always
    /// populated; only read by the C-style template.
    pub wire_class_name: CSharpClassName,
    /// Companion static class hosting `#[data(impl)]` methods for a
    /// C-style enum, since C# `enum`s can't carry members. `None` when
    /// the enum has no methods, or for data enums (whose methods live
    /// on the abstract record).
    pub methods_class_name: Option<CSharpClassName>,
    /// Whether this is a C-style or data enum. Selects the rendering shape.
    pub kind: CSharpEnumKind,
    /// For C-style enums, the integral type after the `:` in `enum Foo : int`.
    /// `None` for data enums, whose public surface is a reference type.
    pub underlying_type: Option<CSharpEnumUnderlyingType>,
    /// Variants in declaration order. Order is significant: each variant's
    /// `wire_tag` is its index in this list.
    pub variants: Vec<CSharpEnumVariantPlan>,
    /// `#[data(impl)]` constructors and methods, merged into one list since
    /// at the C# call site they're both static or instance methods. C-style
    /// enums render these in `methods_class_name`; data enums put them on
    /// the abstract record directly.
    pub methods: Vec<CSharpMethodPlan>,
}

/// Which rendering shape the enum takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpEnumKind {
    /// Every variant is unit. Renders as `public enum Name : int` plus a
    /// `NameWire` static helper class for the wire codec.
    CStyle,
    /// At least one variant carries fields. Renders as
    /// `public abstract record Name` with nested `sealed record` variants.
    Data,
}

/// One variant of a [`CSharpEnumPlan`].
///
/// Examples:
/// ```csharp
/// // C-style variant: an enum member
/// public enum HttpCode : int
/// {
///     NotFound = 404,
/// //  ^^^^^^^^^^^^^^
/// }
///
/// // Data variant: a nested sealed record
/// public abstract record Shape
/// {
///     public sealed record Circle(double Radius) : Shape;
/// //  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CSharpEnumVariantPlan {
    /// Renders a `<summary>` block comment, when `Some`.
    pub summary_doc: Option<CSharpComment>,
    /// Variant name. For C-style enums it's the enum member identifier;
    /// for data enums it's the nested `sealed record` class name.
    pub name: CSharpClassName,
    /// Numeric value on the public surface. For C-style enums this is the
    /// Rust discriminant (e.g., `HttpCode.NotFound = 404`). For data enums
    /// the public surface has no number, so this equals `wire_tag`.
    pub tag: i32,
    /// On-wire tag, always the variant's ordinal index per
    /// `EnumTagStrategy::OrdinalIndex`. Kept separate from `tag` because a
    /// C-style enum's public discriminants can be gapped or negative while
    /// the wire format is always the ordinal.
    pub wire_tag: i32,
    /// Variant fields. Empty for unit variants and for every C-style variant.
    pub fields: Vec<CSharpFieldPlan>,
}

impl CSharpEnumPlan {
    /// Unwraps `underlying_type` for the C-style enum template, which only
    /// renders for C-style enums and so always sees `Some`. Panics on data enums.
    pub fn c_style_underlying_type(&self) -> &CSharpEnumUnderlyingType {
        self.underlying_type
            .as_ref()
            .expect("c_style_underlying_type called on data enum")
    }

    /// Whether any variant payload field's type contains a string at any
    /// depth. Gates the `using System.Text;` import in the data enum template.
    pub fn has_string_fields(&self) -> bool {
        self.variants
            .iter()
            .flat_map(|v| v.fields.iter())
            .any(|f| f.csharp_type.contains_string())
    }
}

impl CSharpEnumVariantPlan {
    /// True for a variant with no payload fields (every C-style variant, plus
    /// data-enum unit variants like `Shape::Point`).
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::{
        CSharpArgumentList, CSharpExpression, CSharpIdentity, CSharpLiteral, CSharpLocalName,
        CSharpMethodName, CSharpPropertyName, CSharpStatement, CSharpType,
    };
    use super::*;

    /// A variant with no payload fields is a unit: true for every C-style
    /// variant and for data-enum unit variants like `Shape::Point`.
    #[test]
    fn variant_with_empty_fields_is_unit() {
        let variant = CSharpEnumVariantPlan {
            summary_doc: None,
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
            summary_doc: None,
            name: CSharpClassName::from_source("circle"),
            tag: 0,
            wire_tag: 0,
            fields: vec![CSharpFieldPlan {
                summary_doc: None,
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
