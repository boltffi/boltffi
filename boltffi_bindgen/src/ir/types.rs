use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, QualifiedName, RecordId,
};

pub type PrimitiveType = boltffi_ffi_rules::primitive::Primitive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Void,
    Primitive(PrimitiveType),
    String,

    Vec(Box<TypeExpr>),
    Option(Box<TypeExpr>),
    Result {
        ok: Box<TypeExpr>,
        err: Box<TypeExpr>,
    },

    Record(RecordId),
    Enum(EnumId),
    Callback(CallbackId),
    Custom(CustomTypeId),
    Builtin(BuiltinId),

    Handle(ClassId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinKind {
    Duration,
    SystemTime,
    Uuid,
    Url,
}

#[derive(Debug, Clone)]
pub struct BuiltinDef {
    pub id: BuiltinId,
    pub kind: BuiltinKind,
    pub rust_type: QualifiedName,
}
