//! [`CSharpEnum`] and its variants. A Rust enum lifts into two shapes:
//! C-style (all unit variants) renders as a native C# `enum` and rides
//! P/Invoke as its integral backing type; data (at least one payload)
//! renders as an `abstract record` hierarchy and travels wire-encoded.
//! [`CSharpEnumKind`] carries that choice; [`CSharpEnumVariant`] holds
//! the per-variant payload using [`CSharpField`](super::CSharpField),
//! the same type record fields use.

use crate::ir::types::PrimitiveType;

use super::{CSharpField, CSharpMethod, CSharpClassName};

/// A Rust enum lifted into the C# type surface. C-style enums (all unit
/// variants) render as native `enum` declarations and ride the CLR's
/// transparent int-marshaling; data enums render as `abstract record`
/// hierarchies and travel wire-encoded.
#[derive(Debug, Clone)]
pub struct CSharpEnum {
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
    /// For C-style enums, the declared integral repr primitive. `None` for
    /// data enums, whose public surface is always a reference type and whose
    /// wire tag stays an implementation detail of the codec.
    pub c_style_tag_type: Option<PrimitiveType>,
    /// Variants, in declaration order. The wire tag is the variant's index
    /// in this list (per `EnumTagStrategy::OrdinalIndex`), so order is
    /// load-bearing.
    pub variants: Vec<CSharpEnumVariant>,
    /// Methods and factory constructors declared via `#[data(impl)]`. For
    /// C-style enums these render in [`Self::methods_class_name`]; for
    /// data enums they go directly on the abstract record. The Rust IR
    /// separates constructors from methods, but at the C# call site
    /// they're both just static or instance methods, merged into one
    /// list here.
    pub methods: Vec<CSharpMethod>,
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

/// One variant of a [`CSharpEnum`]. For C-style enums, `fields` is always
/// empty; for data enums, a unit variant also has empty `fields` (and
/// renders as `sealed record Name() : Enum`).
#[derive(Debug, Clone)]
pub struct CSharpEnumVariant {
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
    pub fields: Vec<CSharpField>,
}

impl CSharpEnum {
    pub fn is_c_style(&self) -> bool {
        self.kind == CSharpEnumKind::CStyle
    }

    pub fn is_data(&self) -> bool {
        self.kind == CSharpEnumKind::Data
    }

    pub fn has_methods(&self) -> bool {
        !self.methods.is_empty()
    }

    fn c_style_tag_type(&self) -> PrimitiveType {
        self.c_style_tag_type
            .expect("c-style enum helpers only apply to C-style enums")
    }

    /// The C# enum backing type keyword (`byte`, `short`, `int`, `long`,
    /// etc.). C# does not permit `nint` / `nuint` enum base types, so those
    /// reprs are filtered out before a plan is ever constructed.
    pub fn c_style_backing_type(&self) -> &'static str {
        match self.c_style_tag_type() {
            PrimitiveType::I8 => "sbyte",
            PrimitiveType::U8 => "byte",
            PrimitiveType::I16 => "short",
            PrimitiveType::U16 => "ushort",
            PrimitiveType::I32 => "int",
            PrimitiveType::U32 => "uint",
            PrimitiveType::I64 => "long",
            PrimitiveType::U64 => "ulong",
            PrimitiveType::Bool
            | PrimitiveType::ISize
            | PrimitiveType::USize
            | PrimitiveType::F32
            | PrimitiveType::F64 => panic!("unsupported C# enum backing type"),
        }
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

impl CSharpEnumVariant {
    /// Whether this variant carries no payload. True for every C-style
    /// variant, and for data enum "unit" variants like `Shape::Point`.
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{CSharpPropertyName, CSharpType};

    fn c_style_enum(source_name: &str, tag_type: PrimitiveType) -> CSharpEnum {
        let class_name = CSharpClassName::from_source(source_name);
        let wire_class_name = CSharpClassName::wire_helper(&class_name);
        CSharpEnum {
            class_name,
            wire_class_name,
            methods_class_name: None,
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(tag_type),
            variants: vec![],
            methods: vec![],
        }
    }

    fn data_enum(source_name: &str) -> CSharpEnum {
        let class_name = CSharpClassName::from_source(source_name);
        let wire_class_name = CSharpClassName::wire_helper(&class_name);
        CSharpEnum {
            class_name,
            wire_class_name,
            methods_class_name: None,
            kind: CSharpEnumKind::Data,
            c_style_tag_type: None,
            variants: vec![],
            methods: vec![],
        }
    }

    /// A variant with no payload fields is a unit: true for every C-style
    /// variant and for data-enum unit variants like `Shape::Point`.
    #[test]
    fn variant_with_empty_fields_is_unit() {
        let variant = CSharpEnumVariant {
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
        let variant = CSharpEnumVariant {
            name: CSharpClassName::from_source("circle"),
            tag: 0,
            wire_tag: 0,
            fields: vec![CSharpField {
                name: CSharpPropertyName::from_source("radius"),
                csharp_type: CSharpType::Double,
                wire_decode_expr: "reader.ReadF64()".to_string(),
                wire_size_expr: "8".to_string(),
                wire_encode_expr: "wire.WriteF64(this.Radius)".to_string(),
            }],
        };
        assert!(!variant.is_unit());
    }

    #[test]
    fn c_style_kind_is_c_style_and_not_data() {
        let enumeration = c_style_enum("status", PrimitiveType::I32);
        assert!(enumeration.is_c_style());
        assert!(!enumeration.is_data());
    }

    #[test]
    fn data_kind_is_data_and_not_c_style() {
        let enumeration = data_enum("shape");
        assert!(enumeration.is_data());
        assert!(!enumeration.is_c_style());
    }

    /// `c_style_backing_type` drives only the public enum declaration
    /// (`public enum LogLevel : byte`). The wire codec is width-fixed at
    /// 4 bytes across every boltffi backend, so there is no per-backing-
    /// type read/write method to resolve: the template hardcodes
    /// `ReadI32`/`WriteI32` around an ordinal-tag switch.
    #[test]
    fn c_style_backing_type_maps_primitive_to_csharp_keyword() {
        let enumeration = CSharpEnum {
            class_name: CSharpClassName::from_source("log_level"),
            wire_class_name: CSharpClassName::from_source("log_level_wire"),
            methods_class_name: None,
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(PrimitiveType::U8),
            variants: vec![],
            methods: vec![],
        };

        assert_eq!(enumeration.c_style_backing_type(), "byte");
    }
}
