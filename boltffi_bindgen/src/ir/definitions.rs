use crate::ir::{
    ids::{AsyncIteratorId, CallbackId, ClassId, ConverterPath, CustomTypeId, EnumId, FieldName, FunctionId, MethodId, ParamName, QualifiedName, RecordId, StreamId, VariantName},
    types::{PrimitiveType, TypeExpr},
};

#[derive(Debug, Clone)]
pub struct DeprecationInfo {
    pub message: Option<String>,
    pub since: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordDef {
    pub id: RecordId,
    pub fields: Vec<FieldDef>,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

impl RecordDef {
    pub fn is_blittable(&self) -> bool {
        self.fields.iter().all(|f| matches!(f.type_expr, TypeExpr::Primitive(_)))
    }
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: FieldName,
    pub type_expr: TypeExpr,
    pub doc: Option<String>,
    pub default: Option<DefaultValue>,
}

#[derive(Debug, Clone)]
pub enum DefaultValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    EnumVariant { enum_name: String, variant_name: String },
    Null,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub id: EnumId,
    pub repr: EnumRepr,
    pub is_error: bool,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

impl EnumDef {
    pub fn variant_docs(&self) -> Vec<Option<String>> {
        match &self.repr {
            EnumRepr::CStyle { variants, .. } => variants.iter().map(|v| v.doc.clone()).collect(),
            EnumRepr::Data { variants, .. } => variants.iter().map(|v| v.doc.clone()).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EnumRepr {
    CStyle { tag_type: PrimitiveType, variants: Vec<CStyleVariant> },
    Data { tag_type: PrimitiveType, variants: Vec<DataVariant> },
}

#[derive(Debug, Clone)]
pub struct CStyleVariant {
    pub name: VariantName,
    pub discriminant: i64,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DataVariant {
    pub name: VariantName,
    pub discriminant: i64,
    pub payload: VariantPayload,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<TypeExpr>),
    Struct(Vec<FieldDef>),
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub id: FunctionId,
    pub params: Vec<ParamDef>,
    pub returns: ReturnDef,
    pub is_async: bool,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: ParamName,
    pub type_expr: TypeExpr,
    pub passing: ParamPassing,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamPassing {
    Value,
    Ref,
    RefMut,
    ImplTrait,
    BoxedDyn,
}

#[derive(Debug, Clone)]
pub enum ReturnDef {
    Void,
    Value(TypeExpr),
    Result { ok: TypeExpr, err: TypeExpr },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    Async,
    Batch,
    Callback,
}

#[derive(Debug, Clone)]
pub struct StreamDef {
    pub id: StreamId,
    pub item_type: TypeExpr,
    pub mode: StreamMode,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

#[derive(Debug, Clone)]
pub struct AsyncIteratorDef {
    pub id: AsyncIteratorId,
    pub item_type: TypeExpr,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub id: ClassId,
    pub constructors: Vec<ConstructorDef>,
    pub methods: Vec<MethodDef>,
    pub streams: Vec<StreamDef>,
    pub async_iterators: Vec<AsyncIteratorDef>,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

#[derive(Debug, Clone)]
pub enum ConstructorDef {
    Default {
        params: Vec<ParamDef>,
        is_fallible: bool,
        doc: Option<String>,
        deprecated: Option<DeprecationInfo>,
    },
    NamedFactory {
        name: MethodId,
        is_fallible: bool,
        doc: Option<String>,
        deprecated: Option<DeprecationInfo>,
    },
    NamedInit {
        name: MethodId,
        first_param: ParamDef,
        rest_params: Vec<ParamDef>,
        is_fallible: bool,
        doc: Option<String>,
        deprecated: Option<DeprecationInfo>,
    },
}

impl ConstructorDef {
    pub fn params(&self) -> Vec<&ParamDef> {
        match self {
            Self::Default { params, .. } => params.iter().collect(),
            Self::NamedFactory { .. } => vec![],
            Self::NamedInit { first_param, rest_params, .. } => std::iter::once(first_param).chain(rest_params).collect(),
        }
    }

    pub fn is_fallible(&self) -> bool {
        match self {
            Self::Default { is_fallible, .. } | Self::NamedFactory { is_fallible, .. } | Self::NamedInit { is_fallible, .. } => *is_fallible,
        }
    }

    pub fn name(&self) -> Option<&MethodId> {
        match self {
            Self::Default { .. } => None,
            Self::NamedFactory { name, .. } | Self::NamedInit { name, .. } => Some(name),
        }
    }

    pub fn doc(&self) -> Option<&str> {
        match self {
            Self::Default { doc, .. } | Self::NamedFactory { doc, .. } | Self::NamedInit { doc, .. } => doc.as_deref(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MethodDef {
    pub id: MethodId,
    pub receiver: Receiver,
    pub params: Vec<ParamDef>,
    pub returns: ReturnDef,
    pub is_async: bool,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receiver {
    Static,
    RefSelf,
    RefMutSelf,
    OwnedSelf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackKind {
    Trait,
    Closure,
}

#[derive(Debug, Clone)]
pub struct CallbackTraitDef {
    pub id: CallbackId,
    pub methods: Vec<CallbackMethodDef>,
    pub kind: CallbackKind,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CallbackMethodDef {
    pub id: MethodId,
    pub params: Vec<ParamDef>,
    pub returns: ReturnDef,
    pub is_async: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CustomTypeDef {
    pub id: CustomTypeId,
    pub rust_type: QualifiedName,
    pub repr: TypeExpr,
    pub converters: ConverterPath,
    pub doc: Option<String>,
}
