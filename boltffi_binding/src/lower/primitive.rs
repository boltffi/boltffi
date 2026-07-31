use boltffi_ast::{Primitive as SourcePrimitive, ReprAttr, ReprItem, TypeExpr};

use crate::{DirectFieldType, IntegerRepr, Primitive};

pub fn classification_primitive(
    primitive: SourcePrimitive,
) -> boltffi_ffi_rules::primitive::Primitive {
    use boltffi_ffi_rules::primitive::Primitive as RulesPrimitive;
    match primitive {
        SourcePrimitive::Bool => RulesPrimitive::Bool,
        SourcePrimitive::I8 => RulesPrimitive::I8,
        SourcePrimitive::U8 => RulesPrimitive::U8,
        SourcePrimitive::I16 => RulesPrimitive::I16,
        SourcePrimitive::U16 => RulesPrimitive::U16,
        SourcePrimitive::I32 => RulesPrimitive::I32,
        SourcePrimitive::U32 => RulesPrimitive::U32,
        SourcePrimitive::I64 => RulesPrimitive::I64,
        SourcePrimitive::U64 => RulesPrimitive::U64,
        SourcePrimitive::ISize => RulesPrimitive::ISize,
        SourcePrimitive::USize => RulesPrimitive::USize,
        SourcePrimitive::F32 => RulesPrimitive::F32,
        SourcePrimitive::F64 => RulesPrimitive::F64,
    }
}

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

pub fn has_repr_c(repr: &ReprAttr) -> bool {
    repr.items.iter().any(|item| matches!(item, ReprItem::C))
}

pub fn has_effective_repr_c(repr: &ReprAttr) -> bool {
    repr.items.is_empty() || has_repr_c(repr)
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
