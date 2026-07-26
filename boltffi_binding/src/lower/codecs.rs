use boltffi_ast::{BaseTrait, TypeExpr};

use crate::{CodecNode, CodecPlan, FieldKey, Op, Primitive, ReadPlan, ValueRef, WritePlan};

use super::{
    LowerError, enums, ids::DeclarationIds, index::Index, records, surface::SurfaceLower, types,
};

#[derive(Clone, Copy)]
pub enum RootEncoding {
    Surface,
    Framed,
}

impl RootEncoding {
    pub fn node<S: SurfaceLower>(
        self,
        index: &Index,
        ids: &DeclarationIds,
        type_expr: &TypeExpr,
        value: ValueRef,
    ) -> Result<CodecNode, LowerError> {
        match (self, type_expr) {
            (Self::Surface, TypeExpr::String) => Ok(S::root_string_codec()),
            _ => node(index, ids, type_expr, value),
        }
    }
}

/// Lowers a source type expression into one [`CodecNode`] in the
/// codec tree.
///
/// `value` names the already-bound value the resulting plan will read
/// or write. The pass threads it through container nodes so a
/// `Vec<T>` measures its element count from the outer value while the
/// element codec recurses with [`ValueRef::self_value`] for the
/// per-element binding.
pub fn node(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    value: ValueRef,
) -> Result<CodecNode, LowerError> {
    Ok(match type_expr {
        TypeExpr::Primitive(primitive) => CodecNode::Primitive(Primitive::from(*primitive)),
        TypeExpr::String | TypeExpr::Str => CodecNode::String,
        TypeExpr::InternedString { static_values, .. } => CodecNode::InternedString {
            static_values: static_values.clone(),
        },
        TypeExpr::Builtin(kind) => CodecNode::Builtin(*kind),
        TypeExpr::Vec(inner) | TypeExpr::Slice(inner) if types::is_byte_primitive(inner) => {
            CodecNode::Bytes
        }
        TypeExpr::Record { id, .. } => {
            let record = index
                .record(id)
                .ok_or_else(|| LowerError::unknown_record(id))?;
            let id = ids.record(id)?;
            if records::is_direct(record) {
                CodecNode::DirectRecord(id)
            } else {
                CodecNode::EncodedRecord(id)
            }
        }
        TypeExpr::Enum { id, .. } => {
            let enumeration = index
                .enumeration(id)
                .ok_or_else(|| LowerError::unknown_enum(id))?;
            let id = ids.enumeration(id)?;
            if enums::is_c_style(enumeration) {
                CodecNode::CStyleEnum(id)
            } else {
                CodecNode::DataEnum(id)
            }
        }
        TypeExpr::Class { id, .. } => CodecNode::ClassHandle(ids.class(id)?),
        TypeExpr::ImplTrait(bounds) => match &bounds.base {
            BaseTrait::Named { id, .. } => CodecNode::CallbackHandle(ids.callback(id)?),
            BaseTrait::Function(_) => {
                return Err(LowerError::unsupported_type(
                    super::error::UnsupportedType::ClosureInValuePosition,
                ));
            }
        },
        TypeExpr::Boxed(inner) | TypeExpr::Arc(inner) => match inner.as_ref() {
            TypeExpr::Dyn(bounds) => match &bounds.base {
                BaseTrait::Named { id, .. } => CodecNode::CallbackHandle(ids.callback(id)?),
                BaseTrait::Function(_) => {
                    return Err(LowerError::unsupported_type(
                        super::error::UnsupportedType::ClosureInValuePosition,
                    ));
                }
            },
            _ => {
                return Err(LowerError::unsupported_type(
                    super::error::UnsupportedType::OpaqueRustContainer,
                ));
            }
        },
        TypeExpr::FnPtr(_) => {
            return Err(LowerError::unsupported_type(
                super::error::UnsupportedType::ClosureInValuePosition,
            ));
        }
        TypeExpr::Dyn(bounds) => {
            return Err(LowerError::unsupported_type(match &bounds.base {
                BaseTrait::Named { .. } => super::error::UnsupportedType::OpaqueRustContainer,
                BaseTrait::Function(_) => super::error::UnsupportedType::ClosureInValuePosition,
            }));
        }
        TypeExpr::Custom { id, .. } => {
            let custom = index
                .custom(id)
                .ok_or_else(|| LowerError::unknown_custom(id))?;
            CodecNode::Custom {
                id: ids.custom(id)?,
                representation: Box::new(node(index, ids, &custom.repr, value)?),
            }
        }
        TypeExpr::Vec(element) | TypeExpr::Slice(element) => {
            let element = node(index, ids, element, ValueRef::self_value())?;
            CodecNode::Sequence {
                len: Op::sequence_len(value),
                element: Box::new(element),
            }
        }
        TypeExpr::Option(inner) => CodecNode::Optional(Box::new(node(index, ids, inner, value)?)),
        TypeExpr::Tuple(elements) => CodecNode::Tuple(
            elements
                .iter()
                .enumerate()
                .map(|(field_index, element)| {
                    let field = FieldKey::position(field_index)
                        .ok_or_else(LowerError::field_position_overflow)?;
                    node(index, ids, element, value.clone().field(field))
                })
                .collect::<Result<Vec<_>, LowerError>>()?,
        ),
        TypeExpr::Result { ok, err } => CodecNode::Result {
            ok: Box::new(node(index, ids, ok, ValueRef::self_value())?),
            err: Box::new(node(index, ids, err, ValueRef::self_value())?),
        },
        TypeExpr::Map {
            kind,
            key,
            value: item,
        } => CodecNode::Map {
            kind: *kind,
            key: Box::new(node(index, ids, key, ValueRef::self_value())?),
            value: Box::new(node(index, ids, item, ValueRef::self_value())?),
        },
        TypeExpr::Unit => {
            return Err(LowerError::unsupported_type(
                super::error::UnsupportedType::UnitInValuePosition,
            ));
        }
        TypeExpr::SelfType => {
            return Err(LowerError::unsupported_type(
                super::error::UnsupportedType::SelfType,
            ));
        }
        TypeExpr::Parameter(_) => {
            return Err(LowerError::unsupported_type(
                super::error::UnsupportedType::TypeParameter,
            ));
        }
    })
}

/// Lowers a source type expression into a bidirectional [`CodecPlan`].
///
/// Wraps [`node`] with the read/write framing every encoded field and
/// whole-record codec carries. `value` is reused for the [`WritePlan`]
/// so generated code does not have to re-derive the path expression.
pub fn plan(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    value: ValueRef,
) -> Result<CodecPlan, LowerError> {
    let root = node(index, ids, type_expr, value.clone())?;
    Ok(CodecPlan::new(
        ReadPlan::new(root.clone()),
        WritePlan::new(value, root),
    ))
}
