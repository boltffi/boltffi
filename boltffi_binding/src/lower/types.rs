use boltffi_ast::{BaseTrait, TypeExpr};

use crate::{Primitive, TypeRef};

use super::{LowerError, error::UnsupportedType, ids::DeclarationIds};

/// Lowers a source type expression into the [`TypeRef`] foreign code
/// sees on the boundary.
///
/// Walks the expression once, resolving every nested record, enum,
/// class, callback, and custom-type reference against the typed ids the
/// caller already built. Source shapes that have no IR encoding yet are
/// rejected here so callers can rely on a successful return for the
/// shape, not the codec.
pub fn lower(ids: &DeclarationIds, type_expr: &TypeExpr) -> Result<TypeRef, LowerError> {
    Ok(match type_expr {
        TypeExpr::Primitive(primitive) => TypeRef::Primitive(Primitive::from(*primitive)),
        TypeExpr::String => TypeRef::String,
        TypeExpr::Str => TypeRef::String,
        TypeExpr::Builtin(kind) => TypeRef::Builtin(*kind),
        TypeExpr::InternedString { static_values, .. } => TypeRef::InternedString {
            static_values: static_values.clone(),
        },
        TypeExpr::Vec(inner) | TypeExpr::Slice(inner) if is_byte_primitive(inner) => TypeRef::Bytes,
        TypeExpr::Record { id, .. } => TypeRef::Record(ids.record(id)?),
        TypeExpr::Enum { id, .. } => TypeRef::Enum(ids.enumeration(id)?),
        TypeExpr::Class { id, .. } => TypeRef::Class(ids.class(id)?),
        TypeExpr::ImplTrait(bounds) => match &bounds.base {
            BaseTrait::Named { id, .. } => TypeRef::Callback(ids.callback(id)?),
            BaseTrait::Function(_) => {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::ClosureInValuePosition,
                ));
            }
        },
        TypeExpr::Boxed(inner) | TypeExpr::Arc(inner) => match inner.as_ref() {
            TypeExpr::Dyn(bounds) => match &bounds.base {
                BaseTrait::Named { id, .. } => TypeRef::Callback(ids.callback(id)?),
                BaseTrait::Function(_) => {
                    return Err(LowerError::unsupported_type(
                        UnsupportedType::ClosureInValuePosition,
                    ));
                }
            },
            _ => {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::OpaqueRustContainer,
                ));
            }
        },
        TypeExpr::Custom { id, .. } => TypeRef::Custom(ids.custom(id)?),
        TypeExpr::Vec(element) => TypeRef::Sequence(Box::new(lower(ids, element)?)),
        TypeExpr::Slice(element) => TypeRef::Sequence(Box::new(lower(ids, element)?)),
        TypeExpr::Option(inner) => TypeRef::Optional(Box::new(lower(ids, inner)?)),
        TypeExpr::Tuple(elements) => TypeRef::Tuple(
            elements
                .iter()
                .map(|element| lower(ids, element))
                .collect::<Result<Vec<_>, LowerError>>()?,
        ),
        TypeExpr::Result { ok, err } => TypeRef::Result {
            ok: Box::new(lower(ids, ok)?),
            err: Box::new(lower(ids, err)?),
        },
        TypeExpr::Map { key, value, .. } => TypeRef::Map {
            key: Box::new(lower(ids, key)?),
            value: Box::new(lower(ids, value)?),
        },
        TypeExpr::FnPtr(_) => {
            return Err(LowerError::unsupported_type(
                UnsupportedType::ClosureInValuePosition,
            ));
        }
        TypeExpr::Dyn(bounds) => {
            return Err(LowerError::unsupported_type(match &bounds.base {
                BaseTrait::Named { .. } => UnsupportedType::OpaqueRustContainer,
                BaseTrait::Function(_) => UnsupportedType::ClosureInValuePosition,
            }));
        }
        TypeExpr::Unit => {
            return Err(LowerError::unsupported_type(
                UnsupportedType::UnitInValuePosition,
            ));
        }
        TypeExpr::SelfType => {
            return Err(LowerError::unsupported_type(UnsupportedType::SelfType));
        }
        TypeExpr::Parameter(_) => {
            return Err(LowerError::unsupported_type(UnsupportedType::TypeParameter));
        }
    })
}

pub fn is_byte_primitive(type_expr: &TypeExpr) -> bool {
    matches!(type_expr, TypeExpr::Primitive(boltffi_ast::Primitive::U8))
}
