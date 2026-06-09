use boltffi_ffi_rules::callable::CallableForm;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReturnType {
    #[default]
    Void,
    Value(Type),
    Fallible {
        ok: Type,
        err: Type,
    },
}

impl ReturnType {
    pub fn value(ty: Type) -> Self {
        if ty.is_void() {
            Self::Void
        } else {
            Self::Value(ty)
        }
    }

    pub fn fallible(ok: Type, err: Type) -> Self {
        Self::Fallible { ok, err }
    }

    pub fn from_output(ty: Type) -> Self {
        match ty.result_types() {
            Some((ok, err)) => Self::Fallible {
                ok: ok.clone(),
                err: err.clone(),
            },
            None => Self::value(ty),
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_fallible(&self) -> bool {
        matches!(self, Self::Fallible { .. })
    }

    pub fn throws(&self) -> bool {
        self.is_fallible()
    }

    pub fn ok_type(&self) -> Option<&Type> {
        match self {
            Self::Void => None,
            Self::Value(ty) => Some(ty),
            Self::Fallible { ok, .. } => Some(ok),
        }
    }

    pub fn err_type(&self) -> Option<&Type> {
        match self {
            Self::Fallible { err, .. } => Some(err),
            _ => None,
        }
    }

    pub fn has_return_value(&self) -> bool {
        match self {
            Self::Void => false,
            Self::Value(ty) => !ty.is_void(),
            Self::Fallible { ok, .. } => !ok.is_void(),
        }
    }

    pub fn value_type(&self) -> Option<&Type> {
        match self {
            Self::Value(ty) => Some(ty),
            _ => None,
        }
    }

    pub fn as_result_types(&self) -> Option<(&Type, &Type)> {
        match self {
            Self::Fallible { ok, err } => Some((ok, err)),
            _ => None,
        }
    }
}

pub type Primitive = boltffi_ffi_rules::primitive::Primitive;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureSignature {
    pub params: Vec<Type>,
    pub returns: Box<Type>,
}

impl ClosureSignature {
    pub fn new(params: Vec<Type>, returns: Type) -> Self {
        Self {
            params,
            returns: Box::new(returns),
        }
    }

    pub fn void_return(params: Vec<Type>) -> Self {
        Self::new(params, Type::Void)
    }

    pub fn single_param(param: Type) -> Self {
        Self::void_return(vec![param])
    }

    pub fn is_void_return(&self) -> bool {
        self.returns.is_void()
    }

    pub fn signature_id(&self) -> String {
        let params_id = self
            .params
            .iter()
            .map(|p| p.type_id())
            .collect::<Vec<_>>()
            .join("_");
        let ret_id = self.returns.type_id();
        if self.is_void_return() {
            if params_id.is_empty() {
                "Void".to_string()
            } else {
                params_id
            }
        } else if params_id.is_empty() {
            format!("To{}", ret_id)
        } else {
            format!("{}To{}", params_id, ret_id)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinId {
    Duration,
    SystemTime,
    Uuid,
    Url,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSpec {
    pub id: BuiltinId,
    pub rust_paths: &'static [&'static str],
}

const BUILTIN_SPECS: &[BuiltinSpec] = &[
    BuiltinSpec {
        id: BuiltinId::Duration,
        rust_paths: &["std::time::Duration", "core::time::Duration"],
    },
    BuiltinSpec {
        id: BuiltinId::SystemTime,
        rust_paths: &["std::time::SystemTime"],
    },
    BuiltinSpec {
        id: BuiltinId::Uuid,
        rust_paths: &["uuid::Uuid"],
    },
    BuiltinSpec {
        id: BuiltinId::Url,
        rust_paths: &["url::Url"],
    },
];

impl BuiltinId {
    pub fn from_rust_path(path: &str) -> Option<Self> {
        let trimmed = path.trim();
        BUILTIN_SPECS
            .iter()
            .find_map(|spec| spec.rust_paths.contains(&trimmed).then_some(spec.id))
    }

    pub fn type_id(self) -> &'static str {
        match self {
            Self::Duration => "Duration",
            Self::SystemTime => "SystemTime",
            Self::Uuid => "Uuid",
            Self::Url => "Url",
        }
    }

    pub fn fixed_wire_size(self) -> Option<usize> {
        match self {
            Self::Duration => Some(12),
            Self::SystemTime => Some(12),
            Self::Uuid => Some(16),
            Self::Url => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    Primitive(Primitive),
    String,
    Builtin(BuiltinId),
    Slice(Box<Type>),
    MutSlice(Box<Type>),
    Vec(Box<Type>),
    Option(Box<Type>),
    Result { ok: Box<Type>, err: Box<Type> },
    Closure(ClosureSignature),
    Custom { name: String, repr: Box<Type> },
    Object(String),
    Record(String),
    Enum(String),
    BoxedTrait(String),
    Void,
}

impl Type {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, Self::Primitive(_))
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub fn is_result(&self) -> bool {
        matches!(self, Self::Result { .. })
    }

    pub fn inner_type(&self) -> Option<&Type> {
        match self {
            Self::Vec(inner) | Self::Option(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn result_types(&self) -> Option<(&Type, &Type)> {
        match self {
            Self::Result { ok, err } => Some((ok, err)),
            _ => None,
        }
    }

    pub fn named_type(&self) -> Option<&str> {
        match self {
            Self::Custom { name, .. }
            | Self::Object(name)
            | Self::Record(name)
            | Self::Enum(name) => Some(name),
            _ => None,
        }
    }

    pub fn vec(element: Type) -> Self {
        Self::Vec(Box::new(element))
    }

    pub fn option(inner: Type) -> Self {
        Self::Option(Box::new(inner))
    }

    pub fn result(ok: Type, err: Type) -> Self {
        Self::Result {
            ok: Box::new(ok),
            err: Box::new(err),
        }
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum(_))
    }

    pub fn is_vec(&self) -> bool {
        matches!(self, Self::Vec(_))
    }

    pub fn primitive(&self) -> Option<Primitive> {
        match self {
            Self::Primitive(p) => Some(*p),
            _ => None,
        }
    }

    pub fn builtin_id(&self) -> Option<BuiltinId> {
        match self {
            Self::Builtin(id) => Some(*id),
            _ => None,
        }
    }

    pub fn record_name(&self) -> Option<&str> {
        match self {
            Self::Record(name) => Some(name),
            _ => None,
        }
    }

    pub fn enum_name(&self) -> Option<&str> {
        match self {
            Self::Enum(name) => Some(name),
            _ => None,
        }
    }

    pub fn vec_inner(&self) -> Option<&Type> {
        match self {
            Self::Vec(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn type_id(&self) -> String {
        match self {
            Self::Void => "Void".into(),
            Self::Primitive(p) => p.type_id().into(),
            Self::String => "String".into(),
            Self::Builtin(id) => id.type_id().into(),
            Self::Vec(inner) => format!("Vec{}", inner.type_id()),
            Self::Option(inner) => format!("Opt{}", inner.type_id()),
            Self::Slice(inner) => format!("Slice{}", inner.type_id()),
            Self::MutSlice(inner) => format!("MutSlice{}", inner.type_id()),
            Self::Result { ok, err } => format!("Result{}Err{}", ok.type_id(), err.type_id()),
            Self::Custom { name, .. } => heck::AsUpperCamelCase(name).to_string(),
            Self::Record(name) => heck::AsUpperCamelCase(name).to_string(),
            Self::Enum(name) => heck::AsUpperCamelCase(name).to_string(),
            Self::Object(name) => heck::AsUpperCamelCase(name).to_string(),
            Self::BoxedTrait(name) => heck::AsUpperCamelCase(name).to_string(),
            Self::Closure(sig) => format!("Fn{}", sig.signature_id()),
        }
    }
}

use super::layout::{CLayout, Layout};

impl CLayout for Type {
    fn c_layout(&self) -> Layout {
        match self {
            Self::Primitive(primitive) => primitive.c_layout(),
            Self::String | Self::Vec(_) | Self::Slice(_) | Self::MutSlice(_) => Layout::new(24, 8),
            Self::Object(_) | Self::BoxedTrait(_) | Self::Closure(_) => Layout::new(8, 8),
            Self::Builtin(_) | Self::Record(_) | Self::Enum(_) | Self::Custom { .. } => {
                Layout::new(8, 8)
            }
            Self::Option(inner) => {
                let inner_layout = inner.c_layout();
                Layout::new(
                    inner_layout.size.as_usize() + inner_layout.alignment.as_usize(),
                    inner_layout.alignment.as_usize(),
                )
            }
            Self::Result { ok, .. } => ok.c_layout(),
            Self::Void => Layout::new(0, 1),
        }
    }
}

impl CLayout for Primitive {
    fn c_layout(&self) -> Layout {
        match self {
            Self::Bool | Self::I8 | Self::U8 => Layout::new(1, 1),
            Self::I16 | Self::U16 => Layout::new(2, 2),
            Self::I32 | Self::U32 | Self::F32 => Layout::new(4, 4),
            Self::I64 | Self::U64 | Self::F64 | Self::USize | Self::ISize => Layout::new(8, 8),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Receiver {
    None,
    Ref,
    RefMut,
}

impl Receiver {
    pub fn callable_form(self) -> CallableForm {
        match self {
            Self::None => CallableForm::StaticMethod,
            Self::Ref | Self::RefMut => CallableForm::InstanceMethod,
        }
    }

    pub fn is_static(self) -> bool {
        self.callable_form() == CallableForm::StaticMethod
    }

    pub fn is_mutable(self) -> bool {
        matches!(self, Self::RefMut)
    }

    pub fn takes_self(self) -> bool {
        !self.is_static()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Deprecation {
    pub message: Option<String>,
    pub since: Option<String>,
}

impl Deprecation {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            since: None,
        }
    }

    pub fn with_since(mut self, version: impl Into<String>) -> Self {
        self.since = Some(version.into());
        self
    }
}
