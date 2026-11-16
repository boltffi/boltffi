use boltffi_ast::{
    DefaultValue as SourceDefaultValue, DeprecationInfo as SourceDeprecationInfo,
    DocComment as SourceDocComment, FloatLiteral as SourceFloatLiteral, Primitive, TypeExpr,
};

use crate::{
    DeclMeta, DefaultValue, DeprecationInfo, DocComment, ElementMeta, FloatValue, IntegerValue,
};

use super::{LowerError, constants, error::UnsupportedType, index::Index};

pub fn decl_meta(
    doc: Option<&SourceDocComment>,
    deprecated: Option<&SourceDeprecationInfo>,
) -> DeclMeta {
    DeclMeta::new(doc.map(DocComment::from), deprecated.map(Into::into))
}

pub fn element_meta(
    doc: Option<&SourceDocComment>,
    deprecated: Option<&SourceDeprecationInfo>,
) -> ElementMeta {
    ElementMeta::new(doc.map(DocComment::from), deprecated.map(Into::into), None)
}

pub fn value_meta(
    index: &Index,
    type_expr: &TypeExpr,
    doc: Option<&SourceDocComment>,
    deprecated: Option<&SourceDeprecationInfo>,
    default: Option<&SourceDefaultValue>,
) -> Result<ElementMeta, LowerError> {
    Ok(ElementMeta::new(
        doc.map(DocComment::from),
        deprecated.map(Into::into),
        default
            .map(|default| lower_default(index, type_expr, default))
            .transpose()?,
    ))
}

impl From<&SourceDocComment> for DocComment {
    fn from(doc: &SourceDocComment) -> Self {
        Self::new(doc.as_str())
    }
}

impl From<&SourceDeprecationInfo> for DeprecationInfo {
    fn from(deprecated: &SourceDeprecationInfo) -> Self {
        Self::new(deprecated.note.clone(), deprecated.since.clone())
    }
}

impl TryFrom<&SourceDefaultValue> for DefaultValue {
    type Error = LowerError;

    fn try_from(default: &SourceDefaultValue) -> Result<Self, Self::Error> {
        match default {
            SourceDefaultValue::Bool(value) => Ok(DefaultValue::Bool(*value)),
            SourceDefaultValue::Integer(value) => {
                Ok(DefaultValue::Integer(IntegerValue::new(value.value)))
            }
            SourceDefaultValue::Float(literal) => parse_float_literal(literal)
                .map(DefaultValue::Float)
                .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::DefaultValue)),
            SourceDefaultValue::String(value) => Ok(DefaultValue::String(value.clone())),
            SourceDefaultValue::None => Ok(DefaultValue::Null),
            SourceDefaultValue::Bytes(_) | SourceDefaultValue::Path(_) => {
                Err(LowerError::unsupported_type(UnsupportedType::DefaultValue))
            }
        }
    }
}

fn lower_default(
    index: &Index,
    type_expr: &TypeExpr,
    default: &SourceDefaultValue,
) -> Result<DefaultValue, LowerError> {
    if let Some(expected) = literal_mismatch(index, type_expr, default) {
        return Err(LowerError::mismatched_default(expected));
    }
    match (type_expr, default) {
        (TypeExpr::Enum { id, .. }, SourceDefaultValue::Path(path)) => index
            .enumeration(id)
            .and_then(|enumeration| constants::enum_variant_default(enumeration, path))
            .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::DefaultValue)),
        (_, SourceDefaultValue::Path(_)) => {
            Err(LowerError::unsupported_type(UnsupportedType::DefaultValue))
        }
        // `None` is only a value of an `Option`: on anything else no host
        // could spell it, so reject it here instead of in every backend.
        (type_expr, SourceDefaultValue::None) if !is_option(index, type_expr) => {
            Err(LowerError::unsupported_type(UnsupportedType::DefaultValue))
        }
        (_, default) => DefaultValue::try_from(default),
    }
}

/// The literal `type_expr` takes, when `default` is a literal it can't hold.
fn literal_mismatch(
    index: &Index,
    type_expr: &TypeExpr,
    default: &SourceDefaultValue,
) -> Option<String> {
    let expected = match type_expr {
        TypeExpr::Option(inner) => return literal_mismatch(index, inner, default),
        TypeExpr::Custom { id, .. } => {
            return index
                .custom(id)
                .and_then(|custom| literal_mismatch(index, &custom.repr, default));
        }
        TypeExpr::Primitive(Primitive::Bool) => "`true` or `false`".to_owned(),
        TypeExpr::Primitive(Primitive::F32 | Primitive::F64) => "a number literal".to_owned(),
        TypeExpr::Primitive(primitive) => {
            let (min, max) = integer_bounds(*primitive);
            match default {
                SourceDefaultValue::Integer(_) => format!("an integer from {min} to {max}"),
                _ => "an integer literal".to_owned(),
            }
        }
        TypeExpr::String | TypeExpr::Str => "a string literal".to_owned(),
        _ => return None,
    };
    let fits = match (type_expr, default) {
        (
            _,
            SourceDefaultValue::None | SourceDefaultValue::Path(_) | SourceDefaultValue::Bytes(_),
        ) => true,
        (TypeExpr::Primitive(Primitive::Bool), SourceDefaultValue::Bool(_)) => true,
        (TypeExpr::Primitive(Primitive::Bool), _) => false,
        (TypeExpr::Primitive(Primitive::F32 | Primitive::F64), SourceDefaultValue::Float(_)) => {
            true
        }
        (TypeExpr::Primitive(Primitive::F32 | Primitive::F64), SourceDefaultValue::Integer(_)) => {
            true
        }
        (TypeExpr::Primitive(primitive), SourceDefaultValue::Integer(value)) => {
            let (min, max) = integer_bounds(*primitive);
            (min..=max).contains(&value.value)
        }
        (TypeExpr::String | TypeExpr::Str, SourceDefaultValue::String(_)) => true,
        _ => false,
    };
    (!fits).then_some(expected)
}

/// The values an integer primitive holds, taking `isize` and `usize` at 64 bits.
fn integer_bounds(primitive: Primitive) -> (i128, i128) {
    match primitive {
        Primitive::I8 => (i8::MIN.into(), i8::MAX.into()),
        Primitive::U8 => (0, u8::MAX.into()),
        Primitive::I16 => (i16::MIN.into(), i16::MAX.into()),
        Primitive::U16 => (0, u16::MAX.into()),
        Primitive::I32 => (i32::MIN.into(), i32::MAX.into()),
        Primitive::U32 => (0, u32::MAX.into()),
        Primitive::U64 | Primitive::USize => (0, u64::MAX.into()),
        _ => (i64::MIN.into(), i64::MAX.into()),
    }
}

/// Whether `type_expr` crosses as an `Option`: a custom type does when its
/// representation does, since hosts spell its values as that
/// representation's.
fn is_option(index: &Index, type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Option(_) => true,
        TypeExpr::Custom { id, .. } => index
            .custom(id)
            .is_some_and(|custom| is_option(index, &custom.repr)),
        _ => false,
    }
}

/// Parses a Rust-source float literal spelling into an
/// [`FloatValue`] by IEEE-754 bit pattern.
///
/// Strips the `f32`/`f64` type suffix and any digit separators, then
/// parses through `f64::from_str`. Returns `None` for unparseable
/// literals; callers route that to a categorical rejection. `FloatValue`
/// stores the f64 bits, so f32 source literals round-trip through f64.
pub fn parse_float_literal(literal: &SourceFloatLiteral) -> Option<FloatValue> {
    let raw = literal.source.as_str();
    let trimmed = raw
        .trim_end_matches("f64")
        .trim_end_matches("f32")
        .trim_end_matches('_');
    let normalized: String = trimmed
        .chars()
        .filter(|character| *character != '_')
        .collect();
    normalized.parse::<f64>().ok().map(FloatValue::from_f64)
}
