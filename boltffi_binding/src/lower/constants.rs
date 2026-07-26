//! Constant declaration lowering.
//!
//! Walks every [`ConstantDef`] the source contract exposes and produces
//! a [`ConstantDecl<S>`] that names the constant, records its
//! declaration metadata, and carries the value as a
//! [`ConstantValueDecl`].
//!
//! Delivery splits on whether the value has a portable inline spelling:
//!
//! - Bool, integer (range-checked against the declared primitive
//!   width), float, string, and enum-variant (via `ConstExpr::Path(...)`
//!   on an enum-typed constant) lower to [`ConstantValueDecl::Inline`]:
//!   the literal is emitted directly in generated source.
//! - Byte-string, array, tuple, and raw (unevaluated) expressions, and
//!   any constant whose declared type is not an inline scalar type,
//!   lower to [`ConstantValueDecl::Accessor`]: a synchronous Rust-side
//!   getter foreign code calls once to read the value. To a foreign
//!   consumer the result is still a named immutable value; inline versus
//!   accessor is only how the bytes are delivered.
//!
//! A value that cannot inhabit its declared type (a bool literal on a
//! string constant, a path to an unknown enum variant) is rejected with
//! [`LowerErrorKind::InvalidConstantValue`].
//!
//! [`ConstantDef`]: boltffi_ast::ConstantDef
//! [`ConstantDecl<S>`]: crate::ConstantDecl
//! [`ConstantValueDecl`]: crate::ConstantValueDecl
//! [`ConstantValueDecl::Inline`]: crate::ConstantValueDecl::Inline
//! [`ConstantValueDecl::Accessor`]: crate::ConstantValueDecl::Accessor
//! [`LowerErrorKind::InvalidConstantValue`]: super::error::LowerErrorKind::InvalidConstantValue

use boltffi_ast::{
    ConstExpr, ConstantDef as SourceConstant, ConstantOwner as SourceConstantOwner,
    EnumDef as SourceEnum, Literal, Path as SourcePath, Primitive as SourcePrimitive, SourceName,
    TypeExpr, VariantPayload,
};

use crate::{CanonicalName, ConstantDecl, ConstantValueDecl, DefaultValue, IntegerValue};

use super::{
    LowerError, callable,
    ids::DeclarationIds,
    index::Index,
    metadata,
    surface::SurfaceLower,
    symbol::{SymbolAllocator, to_snake_case},
    types,
};

pub fn lower<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<ConstantDecl<S>>, LowerError> {
    index
        .constants()
        .iter()
        .map(|constant| lower_one::<S>(index, ids, allocator, constant))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    constant: &SourceConstant,
) -> Result<ConstantDecl<S>, LowerError> {
    let constant_id = ids.constant(&constant.id)?;
    let owner = callable::CallableOwner::from_constant(index, constant.owner.as_ref())?;
    let type_expr = callable::substitute_self_type(owner, &constant.type_expr)?;
    let value = lower_value_decl::<S>(index, ids, allocator, owner, constant, &type_expr)?;
    let declaration = ConstantDecl::new(
        constant_id,
        CanonicalName::from(&constant.name),
        metadata::decl_meta(constant.doc.as_ref(), constant.deprecated.as_ref()),
        value,
    );
    match constant.owner.as_ref() {
        Some(owner) => Ok(declaration.with_owner(ids.constant_owner(owner)?)),
        None => Ok(declaration),
    }
}

fn lower_value_decl<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: callable::CallableOwner<'_>,
    constant: &SourceConstant,
    type_expr: &TypeExpr,
) -> Result<ConstantValueDecl<S>, LowerError> {
    // Chooses inline versus accessor delivery for one constant.
    //
    // A constant inlines when its value is a literal that inhabits the
    // declared type and has a portable spelling. Everything else is
    // delivered through a Rust-side getter the foreign side calls once:
    // byte-string, array, tuple, and raw (unevaluated) expressions, plus
    // any constant whose declared type is not an inline scalar type.
    match inline_default::<S>(index, constant, type_expr)? {
        Some(value) => {
            let ty = types::lower(ids, type_expr)?;
            Ok(ConstantValueDecl::inline(ty, value))
        }
        None => {
            let symbol = allocator.mint_constant_accessor(constant.id.as_str())?;
            let callable =
                callable::lower_constant_accessor::<S>(index, ids, allocator, owner, type_expr)?;
            Ok(ConstantValueDecl::accessor(symbol, Box::new(callable)))
        }
    }
}

fn inline_default<S: SurfaceLower>(
    index: &Index,
    constant: &SourceConstant,
    type_expr: &TypeExpr,
) -> Result<Option<DefaultValue>, LowerError> {
    // Returns the inline literal for a constant, `None` when the value
    // must be delivered through an accessor, or an error when the value
    // cannot inhabit its declared type.
    let Some(expected) = InlineConstantType::from_type_expr::<S>(index, type_expr) else {
        return Ok(None);
    };
    expected.lower_value(constant)
}

#[derive(Clone, Copy)]
enum InlineConstantType<'src> {
    Bool,
    Integer(IntegerBounds),
    Float,
    String,
    Enum(&'src SourceEnum),
}

impl<'src> InlineConstantType<'src> {
    fn from_type_expr<S: SurfaceLower>(index: &Index<'src>, type_expr: &TypeExpr) -> Option<Self> {
        match type_expr {
            TypeExpr::Primitive(SourcePrimitive::Bool) => Some(Self::Bool),
            TypeExpr::Primitive(SourcePrimitive::F32 | SourcePrimitive::F64) => Some(Self::Float),
            TypeExpr::Primitive(primitive) => {
                IntegerBounds::for_primitive::<S>(*primitive).map(Self::Integer)
            }
            TypeExpr::String | TypeExpr::Str => Some(Self::String),
            TypeExpr::Enum { id, .. } => index.enumeration(id).map(Self::Enum),
            _ => None,
        }
    }

    fn lower_value(self, constant: &SourceConstant) -> Result<Option<DefaultValue>, LowerError> {
        match (self, &constant.value) {
            (Self::Bool, ConstExpr::Literal(Literal::Bool(value))) => {
                Ok(Some(DefaultValue::Bool(*value)))
            }
            (Self::Integer(bounds), ConstExpr::Literal(Literal::Integer(value)))
                if bounds.contains(value.value) =>
            {
                Ok(Some(DefaultValue::Integer(IntegerValue::new(value.value))))
            }
            (Self::Float, ConstExpr::Literal(Literal::Float(literal))) => {
                metadata::parse_float_literal(literal)
                    .map(DefaultValue::Float)
                    .map(Some)
                    .ok_or_else(|| LowerError::invalid_constant_value(&constant.id))
            }
            (Self::String, ConstExpr::Literal(Literal::String(value))) => {
                Ok(Some(DefaultValue::String(value.clone())))
            }
            (Self::Enum(enumeration), ConstExpr::Path(path)) => {
                enum_variant_from_path(enumeration, path, constant)
            }
            (_, ConstExpr::Path(_)) => Ok(None),
            (_, ConstExpr::Literal(Literal::Bytes(_)))
            | (_, ConstExpr::Array(_))
            | (_, ConstExpr::Tuple(_))
            | (_, ConstExpr::Raw(_)) => Ok(None),
            _ => Err(LowerError::invalid_constant_value(&constant.id)),
        }
    }
}

fn enum_variant_from_path(
    enumeration: &SourceEnum,
    path: &SourcePath,
    constant: &SourceConstant,
) -> Result<Option<DefaultValue>, LowerError> {
    let Some((variant_segment, qualifier)) = path.segments.split_last() else {
        return Ok(None);
    };
    let associated_self = matches!(
        (constant.owner.as_ref(), qualifier),
        (
            Some(SourceConstantOwner::Enum(owner)),
            [segment]
        ) if owner == &enumeration.id && segment.name.as_str() == "Self"
    );
    if !enum_qualifier_matches(enumeration, qualifier) && !associated_self {
        return Ok(None);
    }
    let variant = enumeration.variants.iter().find(|variant| {
        matches!(variant.payload, VariantPayload::Unit)
            && canonical_name_matches_segment(&variant.name, variant_segment.name.as_str())
    });
    let Some(variant) = variant else {
        return Err(LowerError::invalid_constant_value(&constant.id));
    };
    Ok(Some(DefaultValue::EnumVariant {
        enum_name: CanonicalName::from(&enumeration.name),
        variant_name: CanonicalName::from(&variant.name),
    }))
}

fn enum_qualifier_matches(
    enumeration: &SourceEnum,
    qualifier: &[boltffi_ast::PathSegment],
) -> bool {
    // Checks that the path's qualifier (every segment before the
    // variant) names the enum the constant is typed as.
    //
    // The qualifier is matched against the enum's canonical id (the raw
    // `::`-path the scanner derived the identity from), allowing the
    // leading module path to be omitted: an enum `demo::Mode` accepts
    // `Mode::Fast` and `demo::Mode::Fast`, but rejects
    // `other::Mode::Fast`, which names a different enum that happens to
    // share the leaf name. Matching the id rather than the normalized
    // name also keeps a multi-word enum identifier (which splits into
    // several canonical-name parts) comparable to its single raw path
    // segment.
    if qualifier.is_empty() {
        return false;
    }
    let id_segments: Vec<&str> = enumeration
        .id
        .as_str()
        .split("::")
        .filter(|segment| !segment.is_empty())
        .collect();
    if qualifier.len() > id_segments.len() {
        return false;
    }
    let id_suffix = &id_segments[id_segments.len() - qualifier.len()..];
    qualifier
        .iter()
        .zip(id_suffix)
        .all(|(segment, id_segment)| segment.name.as_str() == *id_segment)
}

fn canonical_name_matches_segment(name: &SourceName, segment: &str) -> bool {
    let parts = name.parts().map(|part| part.as_str()).collect::<Vec<_>>();
    parts.as_slice() == [segment] || parts == source_name_parts(segment)
}

fn source_name_parts(segment: &str) -> Vec<String> {
    to_snake_case(segment.strip_prefix("r#").unwrap_or(segment))
        .split('_')
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Clone, Copy)]
struct IntegerBounds {
    min: i128,
    max: i128,
}

impl IntegerBounds {
    const fn new(min: i128, max: i128) -> Self {
        Self { min, max }
    }

    fn for_primitive<S: SurfaceLower>(primitive: SourcePrimitive) -> Option<Self> {
        match primitive {
            SourcePrimitive::I8 => Some(Self::new(i8::MIN as i128, i8::MAX as i128)),
            SourcePrimitive::U8 => Some(Self::new(0, u8::MAX as i128)),
            SourcePrimitive::I16 => Some(Self::new(i16::MIN as i128, i16::MAX as i128)),
            SourcePrimitive::U16 => Some(Self::new(0, u16::MAX as i128)),
            SourcePrimitive::I32 => Some(Self::new(i32::MIN as i128, i32::MAX as i128)),
            SourcePrimitive::U32 => Some(Self::new(0, u32::MAX as i128)),
            SourcePrimitive::I64 => Some(Self::new(i64::MIN as i128, i64::MAX as i128)),
            SourcePrimitive::U64 => Some(Self::new(0, u64::MAX as i128)),
            SourcePrimitive::ISize => Some(Self::signed_pointer::<S>()),
            SourcePrimitive::USize => Some(Self::unsigned_pointer::<S>()),
            SourcePrimitive::Bool | SourcePrimitive::F32 | SourcePrimitive::F64 => None,
        }
    }

    fn signed_pointer<S: SurfaceLower>() -> Self {
        match S::POINTER_SIZE.get() {
            4 => Self::new(i32::MIN as i128, i32::MAX as i128),
            8 => Self::new(i64::MIN as i128, i64::MAX as i128),
            bytes => {
                let bits = bytes * 8;
                Self::new(-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
            }
        }
    }

    fn unsigned_pointer<S: SurfaceLower>() -> Self {
        match S::POINTER_SIZE.get() {
            4 => Self::new(0, u32::MAX as i128),
            8 => Self::new(0, u64::MAX as i128),
            bytes => Self::new(0, (1_i128 << (bytes * 8)) - 1),
        }
    }

    const fn contains(self, value: i128) -> bool {
        self.min <= value && value <= self.max
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, ConstExpr, ConstantDef, ConstantId as SourceConstantId,
        ConstantOwner as SourceConstantOwner, DeprecationInfo as SourceDeprecationInfo,
        DocComment as SourceDocComment, FloatLiteral, IntegerLiteral, Literal,
        PackageInfo as SourcePackage, Path as SourcePath, Primitive, RecordDef,
        RecordId as SourceRecordId, SourceContract, TypeExpr,
    };

    use crate::lower::{LowerError, LowerErrorKind, lower};
    use crate::{
        Bindings, CanonicalName, ConstantDecl, ConstantId, ConstantOwner, ConstantValueDecl, Decl,
        DefaultValue, DirectValueType, IntegerValue, Native, Primitive as BindingPrimitive,
        ReturnPlan, SurfaceLower, TypeRef, Wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn constant(
        id: &str,
        constant_name: &str,
        type_expr: TypeExpr,
        value: ConstExpr,
    ) -> ConstantDef {
        ConstantDef::new(
            SourceConstantId::new(id),
            name(constant_name),
            type_expr,
            value,
        )
    }

    fn bytes_type() -> TypeExpr {
        TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))
    }

    fn enum_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::enumeration(id.into(), SourcePath::single(path))
    }

    fn lower_constants<S: SurfaceLower>(
        constants: Vec<ConstantDef>,
    ) -> Result<Bindings<S>, LowerError> {
        let mut contract = package();
        contract.constants = constants;
        lower::<S>(&contract)
    }

    fn lower_constants_ok<S: SurfaceLower>(constants: Vec<ConstantDef>) -> Bindings<S> {
        lower_constants::<S>(constants).expect("constants should lower")
    }

    fn constant_decls<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&ConstantDecl<S>> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Constant(constant) => Some(constant.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn only_constant<S: SurfaceLower>(bindings: &Bindings<S>) -> &ConstantDecl<S> {
        let decls = constant_decls(bindings);
        assert_eq!(decls.len(), 1, "expected exactly one constant declaration");
        decls[0]
    }

    #[test]
    fn bool_constant_lowers_inline_with_bool_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::ENABLED",
            "ENABLED",
            TypeExpr::Primitive(Primitive::Bool),
            ConstExpr::Literal(Literal::Bool(true)),
        )]);
        let decl = only_constant(&bindings);

        assert_eq!(decl.name(), &CanonicalName::single("ENABLED"));
        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::Bool));
                assert_eq!(value, &DefaultValue::Bool(true));
            }
            other => panic!("expected inline constant value, got {other:?}"),
        }
    }

    #[test]
    fn integer_constant_lowers_inline_with_integer_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::MAX_SIZE",
            "MAX_SIZE",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1024, "1024"))),
        )]);
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::U32));
                assert_eq!(value, &DefaultValue::Integer(IntegerValue::new(1024)));
            }
            other => panic!("expected inline integer constant, got {other:?}"),
        }
    }

    #[test]
    fn string_constant_lowers_inline_with_string_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::GREETING",
            "GREETING",
            TypeExpr::String,
            ConstExpr::Literal(Literal::String("hello".to_owned())),
        )]);
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(value, &DefaultValue::String("hello".to_owned()));
            }
            other => panic!("expected inline string constant, got {other:?}"),
        }
    }

    #[test]
    fn constant_literal_must_match_declared_type() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::ENABLED",
            "ENABLED",
            TypeExpr::String,
            ConstExpr::Literal(Literal::Bool(true)),
        )])
        .expect_err("mismatched constant literal must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::ENABLED"
        ));
    }

    #[test]
    fn integer_constant_must_fit_declared_type() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::BYTE",
            "BYTE",
            TypeExpr::Primitive(Primitive::U8),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(256, "256"))),
        )])
        .expect_err("out-of-range integer constant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::BYTE"
        ));
    }

    #[test]
    fn usize_constant_bounds_follow_surface_pointer_width() {
        let value = u32::MAX as i128 + 1;
        let native = lower_constants_ok::<Native>(vec![constant(
            "demo::LIMIT",
            "LIMIT",
            TypeExpr::Primitive(Primitive::USize),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(value, "4294967296"))),
        )]);

        assert!(matches!(
            only_constant(&native).value(),
            ConstantValueDecl::Inline { .. }
        ));

        let wasm = lower_constants::<Wasm32>(vec![constant(
            "demo::LIMIT",
            "LIMIT",
            TypeExpr::Primitive(Primitive::USize),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(value, "4294967296"))),
        )])
        .expect_err("usize constant larger than u32 must reject on wasm32");

        assert!(matches!(
            wasm.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::LIMIT"
        ));
    }

    #[test]
    fn constants_use_same_inline_value_on_wasm32() {
        let bindings = lower_constants_ok::<Wasm32>(vec![constant(
            "demo::MAX_SIZE",
            "MAX_SIZE",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1024, "1024"))),
        )]);

        match only_constant(&bindings).value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::U32));
                assert_eq!(value, &DefaultValue::Integer(IntegerValue::new(1024)));
            }
            other => panic!("expected inline integer constant, got {other:?}"),
        }
    }

    #[test]
    fn float_constant_lowers_inline_with_float_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::HALF",
            "HALF",
            TypeExpr::Primitive(Primitive::F64),
            ConstExpr::Literal(Literal::Float(FloatLiteral::new("0.5"))),
        )]);
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::F64));
                match value {
                    DefaultValue::Float(float) => {
                        assert!((float.to_f64() - 0.5).abs() < 1e-12);
                    }
                    other => panic!("expected DefaultValue::Float, got {other:?}"),
                }
            }
            other => panic!("expected inline float constant, got {other:?}"),
        }
    }

    #[test]
    fn float_constant_strips_type_suffix_and_underscores() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::HZ",
            "HZ",
            TypeExpr::Primitive(Primitive::F64),
            ConstExpr::Literal(Literal::Float(FloatLiteral::new("1_000.5f64"))),
        )]);

        match only_constant(&bindings).value() {
            ConstantValueDecl::Inline {
                value: DefaultValue::Float(float),
                ..
            } => {
                assert!((float.to_f64() - 1000.5).abs() < 1e-12);
            }
            other => panic!("expected inline float constant, got {other:?}"),
        }
    }

    #[test]
    fn float_constant_with_garbled_source_is_rejected_with_invalid_value() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::BAD",
            "BAD",
            TypeExpr::Primitive(Primitive::F64),
            ConstExpr::Literal(Literal::Float(FloatLiteral::new("not_a_number"))),
        )])
        .expect_err("unparseable float literal must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::BAD"
        ));
    }

    #[test]
    fn bytes_constant_lowers_via_accessor() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::MAGIC",
            "MAGIC",
            bytes_type(),
            ConstExpr::Literal(Literal::Bytes(vec![0xCA, 0xFE])),
        )]);
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Accessor { symbol, callable } => {
                assert_eq!(symbol.name().as_str(), "boltffi_const_demo_magic");
                assert!(callable.receiver().is_none());
                assert!(callable.params().is_empty());
            }
            other => panic!("expected accessor constant value, got {other:?}"),
        }
    }

    #[test]
    fn associated_self_constant_keeps_typed_owner_and_concrete_return_type() {
        let mut contract = package();
        let record_id = SourceRecordId::new("demo::Color");
        contract
            .records
            .push(RecordDef::new(record_id.clone(), name("Color")));
        let mut black = constant(
            "demo::Color::BLACK",
            "BLACK",
            TypeExpr::SelfType,
            ConstExpr::Raw("Self::rgba(0, 0, 0, 255)".to_owned()),
        );
        black.owner = Some(SourceConstantOwner::Record(record_id));
        contract.constants.push(black);

        let bindings = lower::<Native>(&contract).expect("associated constant should lower");
        let color = bindings
            .decls()
            .iter()
            .find_map(|declaration| match declaration {
                Decl::Record(record) => Some(record.id()),
                _ => None,
            })
            .expect("record declaration");
        let constant = only_constant(&bindings);

        assert_eq!(constant.owner(), Some(ConstantOwner::Record(color)));
        match constant.value() {
            ConstantValueDecl::Accessor { callable, .. } => {
                assert!(callable.params().is_empty());
                assert!(callable.receiver().is_none());
                assert!(matches!(
                    callable.returns().plan(),
                    ReturnPlan::DirectViaReturnSlot {
                        ty: DirectValueType::Record(id)
                    }
                        | ReturnPlan::DirectViaOutPointer {
                            ty: DirectValueType::Record(id)
                        }
                        | ReturnPlan::EncodedViaReturnSlot {
                            ty: TypeRef::Record(id),
                            ..
                        }
                        | ReturnPlan::EncodedViaOutPointer {
                            ty: TypeRef::Record(id),
                            ..
                        } if *id == color
                ));
            }
            other => panic!("expected associated accessor constant, got {other:?}"),
        }
    }

    #[test]
    fn missing_owner_deserializes_as_top_level_constant() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::ANSWER",
            "ANSWER",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(42, "42"))),
        )]);
        let mut serialized =
            serde_json::to_value(only_constant(&bindings)).expect("constant serializes");
        let serde_json::Value::Object(fields) = &mut serialized else {
            panic!("constant serialization should be an object");
        };
        fields.remove("owner");

        let decoded: ConstantDecl<Native> =
            serde_json::from_value(serialized).expect("legacy constant deserializes");

        assert_eq!(decoded.owner(), None);
    }

    #[test]
    fn path_constant_on_non_enum_type_lowers_via_accessor() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::ALIAS",
            "ALIAS",
            TypeExpr::String,
            ConstExpr::Path(SourcePath::single("OTHER")),
        )]);

        match only_constant(&bindings).value() {
            ConstantValueDecl::Accessor { symbol, callable } => {
                assert_eq!(symbol.name().as_str(), "boltffi_const_demo_alias");
                assert!(callable.params().is_empty());
            }
            other => panic!("expected accessor constant value, got {other:?}"),
        }
    }

    #[test]
    fn enum_constant_lowers_inline_with_enum_variant_default() {
        use boltffi_ast::{EnumDef, EnumId as SourceEnumId, PathRoot, PathSegment, VariantDef};

        // Set up an enum `demo::Mode` with variants Fast/Slow, then a
        // constant `DEFAULT_MODE: Mode = Mode::Fast`.
        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef::unit(name("Fast")));
        mode.variants.push(VariantDef::unit(name("Slow")));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                PathRoot::Relative,
                vec![PathSegment::new("Mode"), PathSegment::new("Fast")],
            )),
        ));

        let bindings = lower::<Native>(&contract).expect("enum constant should lower");
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Inline {
                value:
                    DefaultValue::EnumVariant {
                        enum_name,
                        variant_name,
                    },
                ..
            } => {
                assert_eq!(enum_name, &CanonicalName::single("Mode"));
                assert_eq!(variant_name, &CanonicalName::single("Fast"));
            }
            other => panic!("expected inline enum-variant constant, got {other:?}"),
        }
    }

    #[test]
    fn enum_constant_accepts_source_spelled_multi_word_variant_path() {
        use boltffi_ast::{
            EnumDef, EnumId as SourceEnumId, NamePart, PathRoot, PathSegment, VariantDef,
        };

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef::unit(SourceName::new(vec![
            NamePart::new("very"),
            NamePart::new("fast"),
        ])));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                PathRoot::Relative,
                vec![PathSegment::new("Mode"), PathSegment::new("VeryFast")],
            )),
        ));

        let bindings = lower::<Native>(&contract).expect("enum constant should lower");

        match only_constant(&bindings).value() {
            ConstantValueDecl::Inline {
                value:
                    DefaultValue::EnumVariant {
                        enum_name,
                        variant_name,
                    },
                ..
            } => {
                assert_eq!(enum_name, &CanonicalName::single("Mode"));
                assert_eq!(variant_name.as_path_string(), "very::fast");
            }
            other => panic!("expected enum-variant default, got {other:?}"),
        }
    }

    #[test]
    fn enum_constant_accepts_qualified_path() {
        use boltffi_ast::{EnumDef, EnumId as SourceEnumId, PathRoot, PathSegment, VariantDef};

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef::unit(name("Fast")));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                PathRoot::Crate,
                vec![
                    PathSegment::new("demo"),
                    PathSegment::new("Mode"),
                    PathSegment::new("Fast"),
                ],
            )),
        ));

        let bindings =
            lower::<Native>(&contract).expect("qualified enum-path constant should lower");

        match only_constant(&bindings).value() {
            ConstantValueDecl::Inline {
                value:
                    DefaultValue::EnumVariant {
                        enum_name,
                        variant_name,
                    },
                ..
            } => {
                assert_eq!(enum_name, &CanonicalName::single("Mode"));
                assert_eq!(variant_name, &CanonicalName::single("Fast"));
            }
            other => panic!("expected enum-variant default, got {other:?}"),
        }
    }

    #[test]
    fn enum_constant_path_for_same_named_enum_in_other_module_lowers_via_accessor() {
        use boltffi_ast::{EnumDef, EnumId as SourceEnumId, PathRoot, PathSegment, VariantDef};

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef::unit(name("Fast")));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                PathRoot::Relative,
                vec![
                    PathSegment::new("other"),
                    PathSegment::new("Mode"),
                    PathSegment::new("Fast"),
                ],
            )),
        ));

        let bindings = lower::<Native>(&contract).expect("enum alias path should lower");

        match only_constant(&bindings).value() {
            ConstantValueDecl::Accessor { symbol, callable } => {
                assert_eq!(symbol.name().as_str(), "boltffi_const_demo_default_mode");
                assert!(callable.params().is_empty());
            }
            other => panic!("expected accessor constant value, got {other:?}"),
        }
    }

    #[test]
    fn enum_constant_path_for_another_qualifier_lowers_via_accessor() {
        use boltffi_ast::{EnumDef, EnumId as SourceEnumId, PathSegment, VariantDef};

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef::unit(name("Fast")));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                boltffi_ast::PathRoot::Relative,
                vec![PathSegment::new("Other"), PathSegment::new("Fast")],
            )),
        ));

        let bindings = lower::<Native>(&contract).expect("enum alias path should lower");

        match only_constant(&bindings).value() {
            ConstantValueDecl::Accessor { symbol, callable } => {
                assert_eq!(symbol.name().as_str(), "boltffi_const_demo_default_mode");
                assert!(callable.params().is_empty());
            }
            other => panic!("expected accessor constant value, got {other:?}"),
        }
    }

    #[test]
    fn enum_constant_rejects_unknown_variant_path() {
        use boltffi_ast::{EnumDef, EnumId as SourceEnumId, PathSegment, VariantDef};

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef::unit(name("Fast")));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                boltffi_ast::PathRoot::Relative,
                vec![PathSegment::new("Mode"), PathSegment::new("Slow")],
            )),
        ));

        let error = lower::<Native>(&contract).expect_err("unknown enum variant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::DEFAULT_MODE"
        ));
    }

    #[test]
    fn enum_constant_rejects_payload_variant_path() {
        use boltffi_ast::{
            EnumDef, EnumId as SourceEnumId, FieldDef, PathSegment, VariantDef, VariantPayload,
        };

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants.push(VariantDef {
            name: name("Fast").into(),
            discriminant: None,
            payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(Primitive::U32)]),
            doc: None,
            user_attrs: Vec::new(),
            source: boltffi_ast::Source::exported(),
            source_span: None,
        });
        mode.variants.push(VariantDef {
            name: name("Slow").into(),
            discriminant: None,
            payload: VariantPayload::Struct(vec![FieldDef::new(
                name("value"),
                TypeExpr::Primitive(Primitive::U32),
            )]),
            doc: None,
            user_attrs: Vec::new(),
            source: boltffi_ast::Source::exported(),
            source_span: None,
        });
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::new(
                boltffi_ast::PathRoot::Relative,
                vec![PathSegment::new("Mode"), PathSegment::new("Fast")],
            )),
        ));

        let error = lower::<Native>(&contract).expect_err("payload enum variant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::DEFAULT_MODE"
        ));
    }

    #[test]
    fn enum_constant_with_bare_path_lowers_via_accessor() {
        use boltffi_ast::{EnumDef, EnumId as SourceEnumId};

        let mut contract = package();
        let mut mode = EnumDef::new(SourceEnumId::new("demo::Mode"), name("Mode"));
        mode.variants
            .push(boltffi_ast::VariantDef::unit(name("Fast")));
        contract.enums.push(mode);
        contract.constants.push(constant(
            "demo::DEFAULT_MODE",
            "DEFAULT_MODE",
            enum_type("demo::Mode", "Mode"),
            ConstExpr::Path(SourcePath::single("Fast")),
        ));

        let bindings = lower::<Native>(&contract).expect("bare enum path should lower");

        match only_constant(&bindings).value() {
            ConstantValueDecl::Accessor { symbol, callable } => {
                assert_eq!(symbol.name().as_str(), "boltffi_const_demo_default_mode");
                assert!(callable.params().is_empty());
            }
            other => panic!("expected accessor constant value, got {other:?}"),
        }
    }

    #[test]
    fn raw_constant_lowers_via_accessor() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::COMPUTED",
            "COMPUTED",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Raw("1 + 2".to_owned()),
        )]);

        match only_constant(&bindings).value() {
            ConstantValueDecl::Accessor { symbol, callable } => {
                assert_eq!(symbol.name().as_str(), "boltffi_const_demo_computed");
                assert!(callable.params().is_empty());
            }
            other => panic!("expected accessor constant value, got {other:?}"),
        }
    }

    #[test]
    fn tuple_constant_lowers_via_accessor() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::PAIR",
            "PAIR",
            TypeExpr::Tuple(vec![
                TypeExpr::Primitive(Primitive::U32),
                TypeExpr::Primitive(Primitive::U32),
            ]),
            ConstExpr::Tuple(vec![
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ]),
        )]);

        assert!(matches!(
            only_constant(&bindings).value(),
            ConstantValueDecl::Accessor { .. }
        ));
    }

    #[test]
    fn accessor_constant_registers_native_symbol() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::MAGIC",
            "MAGIC",
            bytes_type(),
            ConstExpr::Literal(Literal::Bytes(vec![0xCA, 0xFE])),
        )]);
        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();

        assert!(
            names.contains(&"boltffi_const_demo_magic"),
            "accessor symbol missing from {names:?}"
        );
    }

    #[test]
    fn duplicate_constant_source_ids_are_rejected() {
        let error = lower_constants::<Native>(vec![
            constant(
                "demo::DUP",
                "DUP",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
            ),
            constant(
                "demo::DUP",
                "DUP_AGAIN",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ),
        ])
        .expect_err("duplicate constant id must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::DuplicateSourceId { .. }
        ));
    }

    #[test]
    fn multiple_constants_get_sequential_ids_in_source_order() {
        let bindings = lower_constants_ok::<Native>(vec![
            constant(
                "demo::ONE",
                "ONE",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
            ),
            constant(
                "demo::TWO",
                "TWO",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ),
            constant(
                "demo::THREE",
                "THREE",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(3, "3"))),
            ),
        ]);
        let ids: Vec<u32> = constant_decls(&bindings)
            .into_iter()
            .map(|decl| decl.id().raw())
            .collect();

        assert_eq!(ids, vec![0, 1, 2]);
        assert_eq!(constant_decls(&bindings)[0].id(), ConstantId::from_raw(0));
    }

    #[test]
    fn constant_doc_and_deprecation_propagate_to_decl_meta() {
        let mut greeting = constant(
            "demo::GREETING",
            "GREETING",
            TypeExpr::String,
            ConstExpr::Literal(Literal::String("hello".to_owned())),
        );
        greeting.doc = Some(SourceDocComment::new("standard greeting"));
        greeting.deprecated = Some(SourceDeprecationInfo {
            note: Some("use GREETING_V2".to_owned()),
            since: Some("0.5".to_owned()),
        });

        let bindings = lower_constants_ok::<Native>(vec![greeting]);
        let meta = only_constant(&bindings).meta();

        assert_eq!(
            meta.doc().map(|doc| doc.as_str()),
            Some("standard greeting")
        );
        assert_eq!(
            meta.deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use GREETING_V2")
        );
    }

    #[test]
    fn constant_does_not_register_any_native_symbols() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::MAX",
            "MAX",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(99, "99"))),
        )]);

        assert_eq!(bindings.symbols().symbols().len(), 0);
    }
}
