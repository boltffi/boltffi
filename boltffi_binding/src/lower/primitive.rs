use boltffi_ast::{Primitive as SourcePrimitive, ReprAttr, ReprItem, TypeExpr};

use crate::{DirectFieldType, IntegerRepr, Primitive};

pub fn direct_field_type(type_expr: &TypeExpr) -> Option<DirectFieldType> {
    match type_expr {
        TypeExpr::Primitive(primitive) => DirectFieldType::new((*primitive).into()),
        _ => None,
    }
}

pub fn integer_repr(repr: &ReprAttr) -> Option<IntegerRepr> {
    repr.items.iter().find_map(|item| match item {
        ReprItem::Primitive(SourcePrimitive::I8) => Some(IntegerRepr::I8),
        ReprItem::Primitive(SourcePrimitive::U8) => Some(IntegerRepr::U8),
        ReprItem::Primitive(SourcePrimitive::I16) => Some(IntegerRepr::I16),
        ReprItem::Primitive(SourcePrimitive::U16) => Some(IntegerRepr::U16),
        ReprItem::Primitive(SourcePrimitive::I32) => Some(IntegerRepr::I32),
        ReprItem::Primitive(SourcePrimitive::U32) => Some(IntegerRepr::U32),
        ReprItem::Primitive(SourcePrimitive::I64) => Some(IntegerRepr::I64),
        ReprItem::Primitive(SourcePrimitive::U64) => Some(IntegerRepr::U64),
        ReprItem::Primitive(SourcePrimitive::ISize) => Some(IntegerRepr::ISize),
        ReprItem::Primitive(SourcePrimitive::USize) => Some(IntegerRepr::USize),
        _ => None,
    })
}

/// Reports whether a record's repr pins the plain C layout the direct
/// crossing assumes: no repr at all (`#[data]` materializes `#[repr(C)]`)
/// or `repr(C)` with no layout modifier. `packed`, `align(N)`, and any
/// other item change the compiled layout, so such records cross encoded.
pub fn has_effective_repr_c(repr: &ReprAttr) -> bool {
    repr.items.iter().all(|item| matches!(item, ReprItem::C))
}

impl From<SourcePrimitive> for Primitive {
    fn from(primitive: SourcePrimitive) -> Self {
        match primitive {
            SourcePrimitive::Bool => Self::Bool,
            SourcePrimitive::I8 => Self::I8,
            SourcePrimitive::U8 => Self::U8,
            SourcePrimitive::I16 => Self::I16,
            SourcePrimitive::U16 => Self::U16,
            SourcePrimitive::I32 => Self::I32,
            SourcePrimitive::U32 => Self::U32,
            SourcePrimitive::I64 => Self::I64,
            SourcePrimitive::U64 => Self::U64,
            SourcePrimitive::ISize => Self::ISize,
            SourcePrimitive::USize => Self::USize,
            SourcePrimitive::F32 => Self::F32,
            SourcePrimitive::F64 => Self::F64,
        }
    }
}
