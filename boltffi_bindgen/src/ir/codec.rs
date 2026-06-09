use boltffi_ffi_rules::transport::EnumTagStrategy;

use crate::ir::ids::{BuiltinId, CustomTypeId, EnumId, FieldName, RecordId, VariantName};
use crate::ir::types::{PrimitiveType, TypeExpr};

/// Describes how a type is laid out on the wire: either blittable (fixed-size,
/// no pointers, can be read at known offsets) or variable-length encoded
/// (needs length prefixes and position tracking).
///
/// Recursive types like a tree node containing children of the same type
/// always get encoded layout because their size is not fixed.
#[derive(Debug, Clone)]
pub enum CodecPlan {
    Void,
    Primitive(PrimitiveType),
    String,
    Builtin(BuiltinId),

    Option(Box<CodecPlan>),
    Vec {
        element: Box<CodecPlan>,
        layout: VecLayout,
    },
    Result {
        ok: Box<CodecPlan>,
        err: Box<CodecPlan>,
    },

    Record {
        id: RecordId,
        layout: RecordLayout,
    },
    Enum {
        id: EnumId,
        layout: EnumLayout,
    },
    Custom {
        id: CustomTypeId,
        underlying: Box<CodecPlan>,
    },
}

impl From<&CodecPlan> for TypeExpr {
    fn from(codec: &CodecPlan) -> Self {
        match codec {
            CodecPlan::Void => TypeExpr::Void,
            CodecPlan::Primitive(p) => TypeExpr::Primitive(*p),
            CodecPlan::String => TypeExpr::String,
            CodecPlan::Builtin(id) => TypeExpr::Builtin(id.clone()),
            CodecPlan::Option(inner) => TypeExpr::Option(Box::new(TypeExpr::from(inner.as_ref()))),
            CodecPlan::Vec { element, .. } => {
                TypeExpr::Vec(Box::new(TypeExpr::from(element.as_ref())))
            }
            CodecPlan::Result { ok, err } => TypeExpr::Result {
                ok: Box::new(TypeExpr::from(ok.as_ref())),
                err: Box::new(TypeExpr::from(err.as_ref())),
            },
            CodecPlan::Record { id, .. } => TypeExpr::Record(id.clone()),
            CodecPlan::Enum { id, .. } => TypeExpr::Enum(id.clone()),
            CodecPlan::Custom { id, .. } => TypeExpr::Custom(id.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum VecLayout {
    Blittable { element_size: usize },
    Encoded,
}

#[derive(Debug, Clone)]
pub enum RecordLayout {
    Blittable {
        size: usize,
        fields: Vec<BlittableField>,
    },
    Encoded {
        fields: Vec<EncodedField>,
    },
    Recursive,
}

impl RecordLayout {
    pub fn is_blittable(&self) -> bool {
        matches!(self, RecordLayout::Blittable { .. })
    }
}

#[derive(Debug, Clone)]
pub struct BlittableField {
    pub name: FieldName,
    pub offset: usize,
    pub primitive: PrimitiveType,
}

#[derive(Debug, Clone)]
pub struct EncodedField {
    pub name: FieldName,
    pub codec: CodecPlan,
}

#[derive(Debug, Clone)]
pub enum EnumLayout {
    CStyle {
        tag_type: PrimitiveType,
        tag_strategy: EnumTagStrategy,
        is_error: bool,
    },
    Data {
        tag_type: PrimitiveType,
        tag_strategy: EnumTagStrategy,
        variants: Vec<VariantLayout>,
    },
    Recursive,
}

#[derive(Debug, Clone)]
pub struct VariantLayout {
    pub name: VariantName,
    pub discriminant: i128,
    pub payload: VariantPayloadLayout,
}

#[derive(Debug, Clone)]
pub enum VariantPayloadLayout {
    Unit,
    Fields(Vec<EncodedField>),
}
