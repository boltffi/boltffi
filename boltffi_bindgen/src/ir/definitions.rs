use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use boltffi_ffi_rules::classification::{self, PassableCategory};

use crate::ir::abi::CallId;
use crate::ir::ids::{
    CallbackId, ClassId, ConverterPath, CustomTypeId, EnumId, FieldName, FunctionId, MethodId,
    ParamName, QualifiedName, RecordId, StreamId, VariantName,
};
use crate::ir::types::{PrimitiveType, TypeExpr};

#[derive(Debug, Clone)]
pub struct DeprecationInfo {
    pub message: Option<String>,
    pub since: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordDef {
    pub id: RecordId,
    pub is_repr_c: bool,
    pub is_error: bool,
    pub fields: Vec<FieldDef>,
    pub constructors: Vec<ConstructorDef>,
    pub methods: Vec<MethodDef>,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

impl RecordDef {
    pub fn has_methods(&self) -> bool {
        !self.constructors.is_empty() || !self.methods.is_empty()
    }

    pub fn constructor_calls(&self) -> impl Iterator<Item = (CallId, &ConstructorDef)> {
        self.constructors.iter().enumerate().map(|(i, ctor)| {
            (
                CallId::RecordConstructor {
                    record_id: self.id.clone(),
                    index: i,
                },
                ctor,
            )
        })
    }

    pub fn method_calls(&self) -> impl Iterator<Item = (CallId, &MethodDef)> {
        self.methods.iter().map(|m| {
            (
                CallId::RecordMethod {
                    record_id: self.id.clone(),
                    method_id: m.id.clone(),
                },
                m,
            )
        })
    }

    pub fn is_blittable(&self) -> bool {
        matches!(
            classification::classify_struct_fields(
                self.is_repr_c,
                self.fields.iter().map(|f| match &f.type_expr {
                    TypeExpr::Primitive(p) => Some(*p),
                    _ => None,
                }),
            ),
            PassableCategory::Blittable,
        )
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
    EnumVariant {
        enum_name: String,
        variant_name: String,
    },
    Null,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub id: EnumId,
    pub repr: EnumRepr,
    pub is_error: bool,
    pub constructors: Vec<ConstructorDef>,
    pub methods: Vec<MethodDef>,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

impl EnumDef {
    pub fn has_methods(&self) -> bool {
        !self.constructors.is_empty() || !self.methods.is_empty()
    }

    pub fn constructor_calls(&self) -> impl Iterator<Item = (CallId, &ConstructorDef)> {
        self.constructors.iter().enumerate().map(|(i, ctor)| {
            (
                CallId::EnumConstructor {
                    enum_id: self.id.clone(),
                    index: i,
                },
                ctor,
            )
        })
    }

    pub fn method_calls(&self) -> impl Iterator<Item = (CallId, &MethodDef)> {
        self.methods.iter().map(|m| {
            (
                CallId::EnumMethod {
                    enum_id: self.id.clone(),
                    method_id: m.id.clone(),
                },
                m,
            )
        })
    }

    pub fn variant_docs(&self) -> Vec<Option<String>> {
        match &self.repr {
            EnumRepr::CStyle { variants, .. } => variants.iter().map(|v| v.doc.clone()).collect(),
            EnumRepr::Data { variants, .. } => variants.iter().map(|v| v.doc.clone()).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EnumRepr {
    CStyle {
        tag_type: PrimitiveType,
        variants: Vec<CStyleVariant>,
    },
    Data {
        tag_type: PrimitiveType,
        variants: Vec<DataVariant>,
    },
}

#[derive(Debug, Clone)]
pub struct CStyleVariant {
    pub name: VariantName,
    pub discriminant: i128,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DataVariant {
    pub name: VariantName,
    pub discriminant: i128,
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
    pub execution_kind: ExecutionKind,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

impl FunctionDef {
    pub fn is_async(&self) -> bool {
        self.execution_kind == ExecutionKind::Async
    }

    pub fn callable_form(&self) -> CallableForm {
        CallableForm::Function
    }

    pub fn execution_kind(&self) -> ExecutionKind {
        self.execution_kind
    }
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
pub struct ClassDef {
    pub id: ClassId,
    pub constructors: Vec<ConstructorDef>,
    pub methods: Vec<MethodDef>,
    pub streams: Vec<StreamDef>,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

#[derive(Debug, Clone)]
pub enum ConstructorDef {
    Default {
        params: Vec<ParamDef>,
        is_fallible: bool,
        is_optional: bool,
        doc: Option<String>,
        deprecated: Option<DeprecationInfo>,
    },
    NamedFactory {
        name: MethodId,
        is_fallible: bool,
        is_optional: bool,
        doc: Option<String>,
        deprecated: Option<DeprecationInfo>,
    },
    NamedInit {
        name: MethodId,
        first_param: ParamDef,
        rest_params: Vec<ParamDef>,
        is_fallible: bool,
        is_optional: bool,
        doc: Option<String>,
        deprecated: Option<DeprecationInfo>,
    },
}

impl ConstructorDef {
    pub fn params(&self) -> Vec<&ParamDef> {
        match self {
            Self::Default { params, .. } => params.iter().collect(),
            Self::NamedFactory { .. } => vec![],
            Self::NamedInit {
                first_param,
                rest_params,
                ..
            } => std::iter::once(first_param).chain(rest_params).collect(),
        }
    }

    pub fn is_fallible(&self) -> bool {
        match self {
            Self::Default { is_fallible, .. }
            | Self::NamedFactory { is_fallible, .. }
            | Self::NamedInit { is_fallible, .. } => *is_fallible,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            Self::Default { is_optional, .. }
            | Self::NamedFactory { is_optional, .. }
            | Self::NamedInit { is_optional, .. } => *is_optional,
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
            Self::Default { doc, .. }
            | Self::NamedFactory { doc, .. }
            | Self::NamedInit { doc, .. } => doc.as_deref(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MethodDef {
    pub id: MethodId,
    pub receiver: Receiver,
    pub params: Vec<ParamDef>,
    pub returns: ReturnDef,
    pub execution_kind: ExecutionKind,
    pub doc: Option<String>,
    pub deprecated: Option<DeprecationInfo>,
}

impl MethodDef {
    pub fn is_async(&self) -> bool {
        self.execution_kind == ExecutionKind::Async
    }

    pub fn callable_form(&self) -> CallableForm {
        self.receiver.callable_form()
    }

    pub fn execution_kind(&self) -> ExecutionKind {
        self.execution_kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receiver {
    Static,
    RefSelf,
    RefMutSelf,
    OwnedSelf,
}

impl Receiver {
    pub fn callable_form(self) -> CallableForm {
        match self {
            Self::Static => CallableForm::StaticMethod,
            Self::RefSelf | Self::RefMutSelf | Self::OwnedSelf => CallableForm::InstanceMethod,
        }
    }
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
    pub execution_kind: ExecutionKind,
    pub doc: Option<String>,
}

impl CallbackMethodDef {
    pub fn is_async(&self) -> bool {
        self.execution_kind == ExecutionKind::Async
    }

    pub fn execution_kind(&self) -> ExecutionKind {
        self.execution_kind
    }
}

#[derive(Debug, Clone)]
pub struct CustomTypeDef {
    pub id: CustomTypeId,
    pub rust_type: QualifiedName,
    pub repr: TypeExpr,
    pub converters: ConverterPath,
    pub doc: Option<String>,
}
