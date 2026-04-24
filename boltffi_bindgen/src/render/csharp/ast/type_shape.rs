//! The central type vocabulary for the C# backend. `CSharpType` is
//! the shape every record field, param, return, and variant field
//! resolves to. Named-type variants (`Record`, `CStyleEnum`,
//! `DataEnum`) carry a [`CSharpTypeReference`] rather than a raw
//! string, so callers can distinguish a bare class name from a
//! `global::`-qualified one structurally.
//!
//! All IR-to-type constructors live here (`From<PrimitiveType>`,
//! `enum_backing_for`, `for_enum`, `from_read_op`, `from_type_expr`)
//! so one file answers "what C# type does this IR node become?".

use std::collections::HashSet;
use std::fmt;

use crate::ir::codec::EnumLayout;
use crate::ir::definitions::{EnumDef, EnumRepr};
use crate::ir::ops::ReadOp;
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::{CSharpClassName, CSharpNamespace, CSharpTypeReference};

/// A C# type keyword. Includes `Void` so return types and value types share
/// one enum; params never carry `Void` because the lowerer rejects it before
/// constructing a [`CSharpParam`](super::CSharpParam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpType {
    Void,
    Bool,
    SByte,
    Byte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    NInt,
    NUInt,
    Float,
    Double,
    String,
    /// A user-defined record.
    Record(CSharpTypeReference),
    /// A user-defined C-style enum (all variants are unit). Renders as a
    /// C# `enum` with an `int` backing type. Blittable: passes directly
    /// across P/Invoke as its underlying integer, and stays blittable when
    /// embedded in a `[StructLayout(Sequential)]` record.
    CStyleEnum(CSharpTypeReference),
    /// A user-defined data enum (at least one variant carries a payload).
    /// Renders as an `abstract record` with nested `sealed record` variants.
    /// Always wire-encoded (never blittable) because variant payloads
    /// are variable-width.
    DataEnum(CSharpTypeReference),
    /// A `Vec<T>` projected into the C# surface as a `T[]` jagged array.
    /// Uniform representation across every element kind: primitives ride
    /// the blittable bulk-copy path, composites walk element-by-element.
    /// Nested vecs fall out naturally via recursive `Array(Array(...))`.
    Array(Box<CSharpType>),
    /// An `Option<T>` projected into the C# surface as `T?`. Uniform
    /// across value-type and reference-type inners: for value types it
    /// desugars to `Nullable<T>`, for reference types it reads as a
    /// nullable-annotated reference (both require `#nullable enable`,
    /// which the generated files opt in to). Always travels
    /// wire-encoded across the ABI. The 1-byte tag + payload form does
    /// not line up with any CLR primitive layout.
    Nullable(Box<CSharpType>),
}

impl CSharpType {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    /// Whether this type contains `string` at any nesting depth. Used for
    /// import decisions where `string[]` / `string[][]` still require
    /// `System.Text` because their encode path calls `Encoding.UTF8`.
    pub fn contains_string(&self) -> bool {
        match self {
            Self::String => true,
            Self::Array(inner) | Self::Nullable(inner) => inner.contains_string(),
            _ => false,
        }
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, Self::CStyleEnum(_))
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self, Self::DataEnum(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    /// If this is `Array(inner)`, returns `Some(inner)`; otherwise `None`.
    pub fn array_element(&self) -> Option<&CSharpType> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    /// Recursively qualify any named-type references inside `self` that
    /// are shadowed by an enclosing scope. Primitives and unnamed types
    /// pass through unchanged; `Array` and `Nullable` recurse. The
    /// per-reference decision lives on
    /// [`CSharpTypeReference::qualify_if_shadowed`].
    pub fn qualify_if_shadowed(
        self,
        shadowed: &HashSet<CSharpClassName>,
        namespace: &CSharpNamespace,
    ) -> Self {
        match self {
            Self::Record(r) => Self::Record(r.qualify_if_shadowed(shadowed, namespace)),
            Self::CStyleEnum(r) => Self::CStyleEnum(r.qualify_if_shadowed(shadowed, namespace)),
            Self::DataEnum(r) => Self::DataEnum(r.qualify_if_shadowed(shadowed, namespace)),
            Self::Array(inner) => {
                Self::Array(Box::new((*inner).qualify_if_shadowed(shadowed, namespace)))
            }
            Self::Nullable(inner) => {
                Self::Nullable(Box::new((*inner).qualify_if_shadowed(shadowed, namespace)))
            }
            other => other,
        }
    }

    /// Apply [`Self::qualify_if_shadowed`] when `shadowed` is `Some`;
    /// pass through when it's `None`. Used at call sites that might or
    /// might not be rendering inside a shadowing scope (e.g. an enum
    /// method whose owner may be a data enum or a C-style enum).
    pub fn qualify_if_shadowed_opt(
        self,
        shadowed: Option<&HashSet<CSharpClassName>>,
        namespace: &CSharpNamespace,
    ) -> Self {
        match shadowed {
            Some(sh) => self.qualify_if_shadowed(sh, namespace),
            None => self,
        }
    }

    /// The C# type a Rust C-style enum's tag primitive becomes when used
    /// as the enum's backing type. Returns `None` for primitives that C#
    /// does not accept as an enum base (`nint`, `nuint`, `bool`, `f32`,
    /// `f64`), so the caller can drop the enum from the supported set
    /// instead of emitting an illegal `enum : nuint`.
    pub fn enum_backing_for(tag_type: PrimitiveType) -> Option<CSharpType> {
        match tag_type {
            PrimitiveType::I8 => Some(CSharpType::SByte),
            PrimitiveType::U8 => Some(CSharpType::Byte),
            PrimitiveType::I16 => Some(CSharpType::Short),
            PrimitiveType::U16 => Some(CSharpType::UShort),
            PrimitiveType::I32 => Some(CSharpType::Int),
            PrimitiveType::U32 => Some(CSharpType::UInt),
            PrimitiveType::I64 => Some(CSharpType::Long),
            PrimitiveType::U64 => Some(CSharpType::ULong),
            PrimitiveType::Bool
            | PrimitiveType::ISize
            | PrimitiveType::USize
            | PrimitiveType::F32
            | PrimitiveType::F64 => None,
        }
    }

    /// The C# type a Rust enum definition lifts to. The `EnumRepr` drives
    /// the split: a C-style enum (all unit variants) becomes
    /// [`CSharpType::CStyleEnum`] and rides P/Invoke as its declared
    /// backing integral type; a data enum (at least one payload-carrying
    /// variant) becomes [`CSharpType::DataEnum`] and wire-encodes.
    /// Everything downstream (the return-kind dispatch, param marshaling,
    /// record blittability) keys off this one decision.
    pub fn for_enum(enum_def: &EnumDef) -> CSharpType {
        let class_name: CSharpClassName = (&enum_def.id).into();
        match &enum_def.repr {
            EnumRepr::CStyle { .. } => CSharpType::CStyleEnum(class_name.into()),
            EnumRepr::Data { .. } => CSharpType::DataEnum(class_name.into()),
        }
    }

    /// Converts from a [`ReadOp`].
    pub fn from_read_op(op: &ReadOp) -> Self {
        match op {
            ReadOp::Primitive { primitive, .. } => Self::from(*primitive),
            ReadOp::String { .. } => Self::String,
            ReadOp::Bytes { .. } => Self::Array(Box::new(Self::Byte)),
            ReadOp::Option { some, .. } => {
                let inner = Self::from_read_op(some.ops.first().expect("option inner read op"));
                Self::Nullable(Box::new(inner))
            }
            ReadOp::Vec { element_type, .. } => {
                Self::Array(Box::new(Self::from_type_expr(element_type)))
            }
            ReadOp::Record { id, .. } => {
                let class_name: CSharpClassName = id.into();
                Self::Record(class_name.into())
            }
            ReadOp::Enum { id, layout, .. } => {
                let class_name: CSharpClassName = id.into();
                match layout {
                    EnumLayout::CStyle { .. } => Self::CStyleEnum(class_name.into()),
                    EnumLayout::Data { .. } | EnumLayout::Recursive => {
                        Self::DataEnum(class_name.into())
                    }
                }
            }
            ReadOp::Custom { underlying, .. } => {
                Self::from_read_op(underlying.ops.first().expect("custom underlying read op"))
            }
            ReadOp::Result { .. } | ReadOp::Builtin { .. } => {
                todo!("CSharpType::from_read_op: {:?}", op)
            }
        }
    }

    /// Converts from a [`TypeExpr`]. `TypeExpr::Enum` picks [`Self::DataEnum`]
    /// arbitrarily; all three named-type variants render the same through
    /// [`fmt::Display`] and [`Self::qualify_if_shadowed`].
    pub fn from_type_expr(expr: &TypeExpr) -> Self {
        match expr {
            TypeExpr::Void => Self::Void,
            TypeExpr::Primitive(p) => Self::from(*p),
            TypeExpr::String => Self::String,
            TypeExpr::Bytes => Self::Array(Box::new(Self::Byte)),
            TypeExpr::Vec(inner) => Self::Array(Box::new(Self::from_type_expr(inner))),
            TypeExpr::Option(inner) => Self::Nullable(Box::new(Self::from_type_expr(inner))),
            TypeExpr::Record(id) => {
                let class_name: CSharpClassName = id.into();
                Self::Record(class_name.into())
            }
            TypeExpr::Enum(id) => {
                let class_name: CSharpClassName = id.into();
                Self::DataEnum(class_name.into())
            }
            TypeExpr::Result { .. }
            | TypeExpr::Callback(_)
            | TypeExpr::Custom(_)
            | TypeExpr::Builtin(_)
            | TypeExpr::Handle(_) => todo!("CSharpType::from_type_expr: {:?}", expr),
        }
    }

    /// Whether this type is blittable in the CLR's sense: it can
    /// live inside a `[StructLayout(Sequential)]` record field and pass
    /// across P/Invoke without wire encoding. Primitives all qualify;
    /// `bool` does not (P/Invoke defaults to a 4-byte Win32 BOOL, so
    /// records with bool fields go through the wire path today). Strings
    /// and data enums are always wire-encoded. C-style enums are `int`
    /// underneath and ride the zero-copy path. Records are blittable or
    /// not based on their own field contents, decided elsewhere. This
    /// predicate only answers "is this *leaf* type blittable?".
    pub fn is_blittable_leaf(&self) -> bool {
        match self {
            Self::SByte
            | Self::Byte
            | Self::Short
            | Self::UShort
            | Self::Int
            | Self::UInt
            | Self::Long
            | Self::ULong
            | Self::NInt
            | Self::NUInt
            | Self::Float
            | Self::Double
            | Self::CStyleEnum(_) => true,
            Self::Void
            | Self::Bool
            | Self::String
            | Self::Record(_)
            | Self::DataEnum(_)
            | Self::Array(_)
            | Self::Nullable(_) => false,
        }
    }
}

impl From<PrimitiveType> for CSharpType {
    /// Each boltffi primitive maps to a distinct C# type. C# has native
    /// unsigned types (`byte`, `ushort`, `uint`, `ulong`) and platform-
    /// sized integers (`nint`, `nuint`), so the conversion is lossless.
    fn from(primitive: PrimitiveType) -> Self {
        match primitive {
            PrimitiveType::Bool => CSharpType::Bool,
            PrimitiveType::I8 => CSharpType::SByte,
            PrimitiveType::U8 => CSharpType::Byte,
            PrimitiveType::I16 => CSharpType::Short,
            PrimitiveType::U16 => CSharpType::UShort,
            PrimitiveType::I32 => CSharpType::Int,
            PrimitiveType::U32 => CSharpType::UInt,
            PrimitiveType::I64 => CSharpType::Long,
            PrimitiveType::U64 => CSharpType::ULong,
            PrimitiveType::ISize => CSharpType::NInt,
            PrimitiveType::USize => CSharpType::NUInt,
            PrimitiveType::F32 => CSharpType::Float,
            PrimitiveType::F64 => CSharpType::Double,
        }
    }
}

impl fmt::Display for CSharpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => f.write_str("void"),
            Self::Bool => f.write_str("bool"),
            Self::SByte => f.write_str("sbyte"),
            Self::Byte => f.write_str("byte"),
            Self::Short => f.write_str("short"),
            Self::UShort => f.write_str("ushort"),
            Self::Int => f.write_str("int"),
            Self::UInt => f.write_str("uint"),
            Self::Long => f.write_str("long"),
            Self::ULong => f.write_str("ulong"),
            Self::NInt => f.write_str("nint"),
            Self::NUInt => f.write_str("nuint"),
            Self::Float => f.write_str("float"),
            Self::Double => f.write_str("double"),
            Self::String => f.write_str("string"),
            Self::Record(r) | Self::CStyleEnum(r) | Self::DataEnum(r) => r.fmt(f),
            Self::Array(inner) => write!(f, "{inner}[]"),
            Self::Nullable(inner) => write!(f, "{inner}?"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_type(name: &str) -> CSharpType {
        CSharpType::Record(CSharpClassName::from_source(name).into())
    }

    fn c_style_enum_type(name: &str) -> CSharpType {
        CSharpType::CStyleEnum(CSharpClassName::from_source(name).into())
    }

    fn data_enum_type(name: &str) -> CSharpType {
        CSharpType::DataEnum(CSharpClassName::from_source(name).into())
    }

    #[test]
    fn record_typepe_display_uses_class_name() {
        let ty = record_type("point");
        assert_eq!(ty.to_string(), "Point");
        assert!(ty.is_record());
    }

    #[test]
    fn c_style_enum_typepe_display_uses_class_name() {
        let ty = c_style_enum_type("status");
        assert_eq!(ty.to_string(), "Status");
        assert!(ty.is_c_style_enum());
        assert!(!ty.is_data_enum());
    }

    #[test]
    fn data_enum_typepe_display_uses_class_name() {
        let ty = data_enum_type("shape");
        assert_eq!(ty.to_string(), "Shape");
        assert!(ty.is_data_enum());
        assert!(!ty.is_c_style_enum());
    }

    /// C-style enums ride P/Invoke as their declared backing integral type,
    /// so they count as blittable leaves alongside the numeric primitives.
    /// Data enums never do: their payloads are variable-width and must
    /// wire-encode.
    #[rstest::rstest]
    #[case::int(CSharpType::Int, true)]
    #[case::double(CSharpType::Double, true)]
    #[case::cstyle_enum(c_style_enum_type("status"), true)]
    #[case::bool(CSharpType::Bool, false)]
    #[case::string(CSharpType::String, false)]
    #[case::record(record_type("point"), false)]
    #[case::data_enum(data_enum_type("shape"), false)]
    #[case::nullable_int(CSharpType::Nullable(Box::new(CSharpType::Int)), false)]
    #[case::nullable_string(CSharpType::Nullable(Box::new(CSharpType::String)), false)]
    fn is_blittable_leaf_matches_marshaling_story(#[case] ty: CSharpType, #[case] expected: bool) {
        assert_eq!(ty.is_blittable_leaf(), expected);
    }

    /// `Nullable` renders as `{inner}?`, uniform for value-type inners
    /// (which desugar to `Nullable<T>`) and reference-type inners (which
    /// read as nullable-annotated references under `#nullable enable`).
    #[test]
    fn nullable_type_display_appends_question_mark() {
        assert_eq!(
            CSharpType::Nullable(Box::new(CSharpType::Int)).to_string(),
            "int?"
        );
        assert_eq!(
            CSharpType::Nullable(Box::new(CSharpType::String)).to_string(),
            "string?"
        );
        assert_eq!(
            CSharpType::Nullable(Box::new(record_type("point"))).to_string(),
            "Point?"
        );
    }

    /// `contains_string` must see through `Nullable` so a `string?` field
    /// still triggers the `System.Text` import: the wire-size expression
    /// for a nullable string still calls `Encoding.UTF8.GetByteCount`.
    #[test]
    fn contains_string_sees_through_nullable() {
        assert!(CSharpType::Nullable(Box::new(CSharpType::String)).contains_string());
        assert!(
            CSharpType::Array(Box::new(CSharpType::Nullable(Box::new(CSharpType::String))))
                .contains_string()
        );
        assert!(!CSharpType::Nullable(Box::new(CSharpType::Int)).contains_string());
    }

    mod from_read_op {
        use super::*;
        use crate::ir::codec::{EnumLayout, VecLayout};
        use crate::ir::ids::{EnumId, RecordId};
        use crate::ir::ops::{OffsetExpr, ReadOp, ReadSeq, SizeExpr, WireShape};
        use boltffi_ffi_rules::transport::EnumTagStrategy;

        fn seq(op: ReadOp) -> ReadSeq {
            ReadSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![op],
                shape: WireShape::Value,
            }
        }

        fn prim(p: PrimitiveType) -> ReadOp {
            ReadOp::Primitive {
                primitive: p,
                offset: OffsetExpr::Base,
            }
        }

        fn cstyle_layout() -> EnumLayout {
            EnumLayout::CStyle {
                tag_type: PrimitiveType::I32,
                tag_strategy: EnumTagStrategy::Discriminant,
                is_error: false,
            }
        }

        fn data_layout() -> EnumLayout {
            EnumLayout::Data {
                tag_type: PrimitiveType::I32,
                tag_strategy: EnumTagStrategy::Discriminant,
                variants: vec![],
            }
        }

        #[test]
        fn primitive_maps_to_backing_type() {
            assert_eq!(
                CSharpType::from_read_op(&prim(PrimitiveType::I32)),
                CSharpType::Int
            );
            assert_eq!(
                CSharpType::from_read_op(&prim(PrimitiveType::F64)),
                CSharpType::Double
            );
        }

        #[test]
        fn string_maps_to_string() {
            let op = ReadOp::String {
                offset: OffsetExpr::Base,
            };
            assert_eq!(CSharpType::from_read_op(&op), CSharpType::String);
        }

        #[test]
        fn record_maps_to_record_with_class_name() {
            let op = ReadOp::Record {
                id: RecordId::new("point"),
                offset: OffsetExpr::Base,
                fields: vec![],
            };
            assert_eq!(CSharpType::from_read_op(&op), record_type("point"));
        }

        #[test]
        fn enum_cstyle_layout_maps_to_cstyle_enum() {
            let op = ReadOp::Enum {
                id: EnumId::new("status"),
                offset: OffsetExpr::Base,
                layout: cstyle_layout(),
            };
            assert_eq!(CSharpType::from_read_op(&op), c_style_enum_type("status"));
        }

        #[test]
        fn enum_data_layout_maps_to_data_enum() {
            let op = ReadOp::Enum {
                id: EnumId::new("shape"),
                offset: OffsetExpr::Base,
                layout: data_layout(),
            };
            assert_eq!(CSharpType::from_read_op(&op), data_enum_type("shape"));
        }

        #[test]
        fn option_wraps_inner_in_nullable() {
            let op = ReadOp::Option {
                tag_offset: OffsetExpr::Base,
                some: Box::new(seq(prim(PrimitiveType::I32))),
            };
            assert_eq!(
                CSharpType::from_read_op(&op),
                CSharpType::Nullable(Box::new(CSharpType::Int))
            );
        }

        #[test]
        fn vec_wraps_element_type_in_array() {
            let op = ReadOp::Vec {
                len_offset: OffsetExpr::Base,
                element_type: TypeExpr::Record(RecordId::new("point")),
                element: Box::new(seq(ReadOp::Record {
                    id: RecordId::new("point"),
                    offset: OffsetExpr::Base,
                    fields: vec![],
                })),
                layout: VecLayout::Encoded,
            };
            assert_eq!(
                CSharpType::from_read_op(&op),
                CSharpType::Array(Box::new(record_type("point")))
            );
        }

        #[test]
        fn option_of_vec_of_record_nests_correctly() {
            let inner_vec = ReadOp::Vec {
                len_offset: OffsetExpr::Base,
                element_type: TypeExpr::Record(RecordId::new("point")),
                element: Box::new(seq(ReadOp::Record {
                    id: RecordId::new("point"),
                    offset: OffsetExpr::Base,
                    fields: vec![],
                })),
                layout: VecLayout::Encoded,
            };
            let option_op = ReadOp::Option {
                tag_offset: OffsetExpr::Base,
                some: Box::new(seq(inner_vec)),
            };
            assert_eq!(
                CSharpType::from_read_op(&option_op),
                CSharpType::Nullable(Box::new(CSharpType::Array(Box::new(record_type("point")))))
            );
        }

        /// `qualify_if_shadowed` recurses through the typed intermediate,
        /// so a shadowed `Point` inside `Option<Vec<Point>>` still
        /// qualifies correctly.
        #[test]
        fn qualify_if_shadowed_reaches_through_nested_builder_output() {
            let option_op = ReadOp::Option {
                tag_offset: OffsetExpr::Base,
                some: Box::new(seq(ReadOp::Vec {
                    len_offset: OffsetExpr::Base,
                    element_type: TypeExpr::Record(RecordId::new("point")),
                    element: Box::new(seq(ReadOp::Record {
                        id: RecordId::new("point"),
                        offset: OffsetExpr::Base,
                        fields: vec![],
                    })),
                    layout: VecLayout::Encoded,
                })),
            };
            let ty = CSharpType::from_read_op(&option_op);
            let shadowed: HashSet<CSharpClassName> =
                std::iter::once(CSharpClassName::from_source("point")).collect();
            let namespace = CSharpNamespace::from_source("demo");
            let qualified = ty.qualify_if_shadowed(&shadowed, &namespace);
            assert_eq!(qualified.to_string(), "global::Demo.Point[]?");
        }
    }

    mod from_type_expr {
        use super::*;
        use crate::ir::ids::{EnumId, RecordId};

        #[test]
        fn primitive_maps_to_backing_type() {
            assert_eq!(
                CSharpType::from_type_expr(&TypeExpr::Primitive(PrimitiveType::I32)),
                CSharpType::Int
            );
        }

        #[test]
        fn string_maps_to_string() {
            assert_eq!(
                CSharpType::from_type_expr(&TypeExpr::String),
                CSharpType::String
            );
        }

        #[test]
        fn record_maps_to_record_with_class_name() {
            assert_eq!(
                CSharpType::from_type_expr(&TypeExpr::Record(RecordId::new("point"))),
                record_type("point")
            );
        }

        /// `TypeExpr::Enum` has no layout metadata available here, so we
        /// commit to [`CSharpType::DataEnum`] by convention. Display and
        /// qualification render identically for all named-type variants,
        /// so downstream rendering is unaffected.
        #[test]
        fn enum_maps_to_data_enum_by_convention() {
            assert_eq!(
                CSharpType::from_type_expr(&TypeExpr::Enum(EnumId::new("status"))),
                data_enum_type("status")
            );
        }

        #[test]
        fn vec_wraps_element_in_array() {
            let expr = TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::F64)));
            assert_eq!(
                CSharpType::from_type_expr(&expr),
                CSharpType::Array(Box::new(CSharpType::Double))
            );
        }

        #[test]
        fn option_wraps_inner_in_nullable() {
            let expr = TypeExpr::Option(Box::new(TypeExpr::String));
            assert_eq!(
                CSharpType::from_type_expr(&expr),
                CSharpType::Nullable(Box::new(CSharpType::String))
            );
        }

        #[test]
        fn option_of_vec_of_record_nests_correctly() {
            let expr = TypeExpr::Option(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Record(
                RecordId::new("point"),
            )))));
            assert_eq!(
                CSharpType::from_type_expr(&expr),
                CSharpType::Nullable(Box::new(CSharpType::Array(Box::new(record_type("point")))))
            );
        }
    }

    mod for_enum {
        use super::*;
        use crate::ir::definitions::{CStyleVariant, DataVariant, VariantPayload};
        use crate::ir::ids::EnumId;

        fn enum_def(id: &str, repr: EnumRepr) -> EnumDef {
            EnumDef {
                id: EnumId::new(id),
                repr,
                is_error: false,
                constructors: vec![],
                methods: vec![],
                doc: None,
                deprecated: None,
            }
        }

        #[test]
        fn c_style_repr_maps_to_c_style_enum_typepe() {
            let def = enum_def(
                "Status",
                EnumRepr::CStyle {
                    tag_type: PrimitiveType::I32,
                    variants: vec![CStyleVariant {
                        name: "Active".into(),
                        discriminant: 0,
                        doc: None,
                    }],
                },
            );
            assert_eq!(CSharpType::for_enum(&def), c_style_enum_type("Status"));
        }

        #[test]
        fn data_repr_maps_to_data_enum_typepe() {
            let def = enum_def(
                "Shape",
                EnumRepr::Data {
                    tag_type: PrimitiveType::I32,
                    variants: vec![DataVariant {
                        name: "Point".into(),
                        discriminant: 0,
                        payload: VariantPayload::Unit,
                        doc: None,
                    }],
                },
            );
            assert_eq!(CSharpType::for_enum(&def), data_enum_type("Shape"));
        }

        /// `class_name` runs the source `snake_case` enum name through
        /// [`CSharpClassName::from_source`], so the C# type keeps the
        /// name in PascalCase even if upstream ever shifts the ID casing.
        #[test]
        fn class_name_round_trips_through_naming_convention() {
            let def = enum_def(
                "log_level",
                EnumRepr::CStyle {
                    tag_type: PrimitiveType::I32,
                    variants: vec![],
                },
            );
            assert_eq!(CSharpType::for_enum(&def), c_style_enum_type("log_level"));
        }
    }

    mod enum_backing_for {
        use super::*;

        #[test]
        fn maps_u8_to_byte() {
            assert_eq!(
                CSharpType::enum_backing_for(PrimitiveType::U8),
                Some(CSharpType::Byte)
            );
        }

        #[test]
        fn rejects_usize() {
            assert_eq!(CSharpType::enum_backing_for(PrimitiveType::USize), None);
        }
    }
}
