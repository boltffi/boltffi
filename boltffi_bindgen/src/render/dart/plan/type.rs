use crate::{
    ir::{
        AbiParam, AbiType, BuiltinId, CallbackId, CallbackKind, ClassId, CustomTypeId, EnumId,
        ErrorTransport, ParamRole, PrimitiveType, RecordId, ReturnDef, ReturnShape, SpanContent,
        Transport, TypeCatalog, TypeExpr,
    },
    render::dart::{NamingConvention, emit},
};

#[derive(Clone, Debug)]
pub enum DartNativeFunctionKind {
    InlineClosure,
    Callback,
}

#[derive(Clone, Debug)]
pub enum DartNativeType {
    Void,
    Primitive(PrimitiveType),
    Composite(RecordId),
    Function {
        kind: DartNativeFunctionKind,
        params: Vec<DartNativeType>,
        return_ty: Box<DartNativeType>,
    },
    Pointer(Box<DartNativeType>),
    OwnedBuffer,
    CallbackHandle,
    Status,
    Custom(String),
}

impl DartNativeType {
    pub fn from_abi_type(abi_type: &AbiType) -> Self {
        match abi_type {
            AbiType::Void => DartNativeType::Void,
            AbiType::Bool => DartNativeType::Primitive(PrimitiveType::Bool),
            AbiType::I8 => DartNativeType::Primitive(PrimitiveType::I8),
            AbiType::U8 => DartNativeType::Primitive(PrimitiveType::U8),
            AbiType::I16 => DartNativeType::Primitive(PrimitiveType::I16),
            AbiType::U16 => DartNativeType::Primitive(PrimitiveType::U16),
            AbiType::I32 => DartNativeType::Primitive(PrimitiveType::I32),
            AbiType::U32 => DartNativeType::Primitive(PrimitiveType::U32),
            AbiType::I64 => DartNativeType::Primitive(PrimitiveType::I64),
            AbiType::U64 => DartNativeType::Primitive(PrimitiveType::U64),
            AbiType::ISize => DartNativeType::Primitive(PrimitiveType::ISize),
            AbiType::USize => DartNativeType::Primitive(PrimitiveType::USize),
            AbiType::F32 => DartNativeType::Primitive(PrimitiveType::F32),
            AbiType::F64 => DartNativeType::Primitive(PrimitiveType::F64),
            AbiType::Pointer(primitive) => {
                DartNativeType::Pointer(Box::new(DartNativeType::Primitive(*primitive)))
            }
            AbiType::OwnedBuffer => DartNativeType::OwnedBuffer,
            AbiType::InlineCallbackFn {
                params,
                return_type,
            } => DartNativeType::Function {
                params: params.iter().map(Self::from_abi_type).collect(),
                return_ty: Box::new(Self::from_abi_type(return_type)),
                kind: DartNativeFunctionKind::InlineClosure,
            },
            AbiType::Handle(_) => DartNativeType::Pointer(Box::new(DartNativeType::Void)),
            AbiType::CallbackHandle => DartNativeType::CallbackHandle,
            AbiType::Struct(record_id) => {
                DartNativeType::Custom(NamingConvention::record_struct_name(record_id.as_str()))
            }
        }
    }
    pub fn native_type(&self) -> String {
        match self {
            DartNativeType::Void => "$$ffi.Void".to_string(),
            DartNativeType::Primitive(primitive) => {
                emit::primitive_native_type(*primitive).to_string()
            }
            DartNativeType::Composite(record) => {
                NamingConvention::record_struct_name(record.as_str())
            }
            DartNativeType::Function {
                params,
                return_ty,
                kind,
            } => format!(
                "$$ffi.Pointer<$$ffi.NativeFunction<{} Function({})>>",
                return_ty.native_type(),
                params.iter().fold(
                    match kind {
                        DartNativeFunctionKind::InlineClosure =>
                        // closure context pointer
                            DartNativeType::Pointer(Box::new(DartNativeType::Void)).native_type(),
                        DartNativeFunctionKind::Callback =>
                        // async context handle
                            DartNativeType::Primitive(PrimitiveType::U64).native_type(),
                    },
                    |acc, ty| acc + ", " + ty.native_type().as_str()
                )
            ),
            DartNativeType::Pointer(inner) => format!("$$ffi.Pointer<{}>", inner.native_type()),
            DartNativeType::OwnedBuffer => "_$$FFIBuf".to_string(),
            DartNativeType::CallbackHandle => "_$$BoltFFICallbackHandle".to_string(),
            DartNativeType::Status => "_$$FFIStatus".to_string(),
            DartNativeType::Custom(ty) => ty.clone(),
        }
    }

    pub fn dart_sub_type(&self) -> String {
        match self {
            DartNativeType::Void => "void".to_string(),
            DartNativeType::Primitive(primitive) => emit::primitive_dart_type(*primitive),
            DartNativeType::Composite(record) => record.to_string(),
            o @ (DartNativeType::Function { .. }
            | DartNativeType::Pointer(..)
            | DartNativeType::OwnedBuffer
            | DartNativeType::CallbackHandle
            | DartNativeType::Status
            | DartNativeType::Custom(..)) => o.native_type(),
        }
    }

    pub fn from_return_shape_and_error_transport(
        return_shape: &ReturnShape,
        error_transport: &ErrorTransport,
    ) -> Self {
        if let Some(Transport::Handle { class_id, .. }) = &return_shape.transport {
            return Self::from_abi_type(&AbiType::Handle(class_id.clone()));
        }

        if matches!(return_shape.transport, Some(Transport::Callback { .. })) {
            return Self::from_abi_type(&AbiType::CallbackHandle);
        }

        if matches!(error_transport, ErrorTransport::Encoded { .. }) {
            return Self::from_abi_type(&AbiType::OwnedBuffer);
        }

        match &return_shape.transport {
            None => {
                if matches!(error_transport, ErrorTransport::StatusCode) {
                    Self::Status
                } else {
                    Self::from_abi_type(&AbiType::Void)
                }
            }
            Some(Transport::Scalar(origin)) => Self::Primitive(origin.primitive()),
            Some(Transport::Composite(layout)) => {
                Self::from_abi_type(&AbiType::Struct(layout.record_id.clone()))
            }
            Some(Transport::Span(_)) => Self::from_abi_type(&AbiType::OwnedBuffer),
            Some(Transport::Handle { .. } | Transport::Callback { .. }) => unreachable!(),
        }
    }

    pub fn from_abi_param(abi_param: &AbiParam) -> Self {
        if let ParamRole::CallbackContext { .. } = &abi_param.role {
            return Self::Pointer(Box::new(Self::Void));
        }

        if let ParamRole::StatusOut = &abi_param.role {
            return Self::Pointer(Box::new(Self::Status));
        }

        let native_type = Self::from_abi_type(&abi_param.abi_type);

        match &abi_param.role {
            ParamRole::OutDirect | ParamRole::OutLen { .. } => Self::Pointer(Box::new(native_type)),
            _ => native_type,
        }
    }

    pub fn field_annot(&self) -> String {
        match self {
            DartNativeType::Void
            | DartNativeType::Composite(_)
            | DartNativeType::Function { .. }
            | DartNativeType::Pointer(_)
            | DartNativeType::OwnedBuffer
            | DartNativeType::CallbackHandle
            | DartNativeType::Status
            | DartNativeType::Custom(_) => String::new(),
            primitive @ DartNativeType::Primitive(_) => format!("@{}()", primitive.native_type()),
        }
    }

    pub fn from_transport(transport: &Transport) -> Self {
        match transport {
            Transport::Scalar(origin) => Self::Primitive(origin.primitive()),
            Transport::Composite(layout) => Self::Composite(layout.record_id.clone()),
            Transport::Span(content) => match content {
                SpanContent::Scalar(origin) => Self::Primitive(origin.primitive()),
                SpanContent::Composite(layout) => Self::Composite(layout.record_id.clone()),
                SpanContent::Utf8 | SpanContent::Encoded(..) => Self::OwnedBuffer,
            },
            Transport::Handle { .. } => Self::Pointer(Box::new(Self::Void)),
            Transport::Callback { .. } => Self::CallbackHandle,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DartType {
    Void,
    Bool,
    Int,
    Double,
    String,
    Option(Box<DartType>),
    Result {
        ok: Box<DartType>,
        err: Box<DartType>,
    },
    Bytes,
    List(Box<DartType>),
    Function {
        params: Vec<DartType>,
        ret_ty: Box<DartType>,
    },
    Class(ClassId),
    Record(RecordId),
    Enum(EnumId),
    Callback(CallbackId),
    Custom(CustomTypeId),
    Builtin(BuiltinId),
}

impl DartType {
    pub fn from_primitive(primitive: PrimitiveType) -> Self {
        match primitive {
            PrimitiveType::Bool => DartType::Bool,
            PrimitiveType::I8
            | PrimitiveType::U8
            | PrimitiveType::I16
            | PrimitiveType::U16
            | PrimitiveType::I32
            | PrimitiveType::U32
            | PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => DartType::Int,
            PrimitiveType::F32 | PrimitiveType::F64 => DartType::Double,
        }
    }
    pub fn from_type_expr(type_expr: &TypeExpr, type_catalog: &TypeCatalog) -> Self {
        match type_expr {
            TypeExpr::Void => DartType::Void,
            TypeExpr::Primitive(primitive) => Self::from_primitive(*primitive),
            TypeExpr::String => DartType::String,
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(PrimitiveType::U8) => DartType::Bytes,
                _ => DartType::List(Box::new(Self::from_type_expr(inner, type_catalog))),
            },
            TypeExpr::Option(inner) => {
                DartType::Option(Box::new(Self::from_type_expr(inner, type_catalog)))
            }
            TypeExpr::Result { ok, err } => DartType::Result {
                ok: Box::new(Self::from_type_expr(ok, type_catalog)),
                err: Box::new(Self::from_type_expr(err, type_catalog)),
            },
            TypeExpr::Record(record_id) => DartType::Record(record_id.clone()),
            TypeExpr::Enum(enum_id) => DartType::Enum(enum_id.clone()),
            TypeExpr::Callback(callback_id) => {
                let callback_def = type_catalog.resolve_callback(callback_id).unwrap();
                match callback_def.kind {
                    CallbackKind::Trait => DartType::Callback(callback_id.clone()),
                    CallbackKind::Closure => {
                        let call_method = &callback_def.methods[0];
                        assert!(call_method.id.as_str() == "call");
                        DartType::Function {
                            params: call_method
                                .params
                                .iter()
                                .map(|p| Self::from_type_expr(&p.type_expr, type_catalog))
                                .collect(),
                            ret_ty: Box::new(Self::from_return_def(
                                &call_method.returns,
                                type_catalog,
                            )),
                        }
                    }
                }
            }
            TypeExpr::Custom(custom_type_id) => DartType::Custom(custom_type_id.clone()),
            TypeExpr::Builtin(builtin_id) => DartType::Builtin(builtin_id.clone()),
            TypeExpr::Handle(class_id) => DartType::Class(class_id.clone()),
        }
    }

    pub fn from_return_def(return_def: &ReturnDef, type_catalog: &TypeCatalog) -> Self {
        match return_def {
            ReturnDef::Void => DartType::Void,
            ReturnDef::Value(ty) => DartType::from_type_expr(ty, type_catalog),
            ReturnDef::Result { ok, err } => DartType::Result {
                ok: Box::new(DartType::from_type_expr(ok, type_catalog)),
                err: Box::new(DartType::from_type_expr(err, type_catalog)),
            },
        }
    }

    pub fn from_transport(transport: &Transport) -> Self {
        match transport {
            Transport::Scalar(origin) => Self::from_primitive(origin.primitive()),
            Transport::Composite(layout) => Self::Record(layout.record_id.clone()),
            Transport::Span(content) => match content {
                SpanContent::Scalar(origin) => Self::from_primitive(origin.primitive()),
                SpanContent::Composite(layout) => Self::Record(layout.record_id.clone()),
                SpanContent::Utf8 => Self::String,
                SpanContent::Encoded(..) => Self::Bytes,
            },
            Transport::Handle { class_id, .. } => Self::Class(class_id.clone()),
            Transport::Callback { callback_id, .. } => Self::Callback(callback_id.clone()),
        }
    }

    pub fn dart_type(&self) -> String {
        match self {
            DartType::Void => "void".to_string(),
            DartType::Bool => "bool".to_string(),
            DartType::Int => "int".to_string(),
            DartType::Double => "double".to_string(),
            DartType::String => "String".to_string(),
            DartType::Option(inner) => format!("{}?", inner.dart_type()),
            DartType::Result { ok, err } => {
                format!("BoltFFIResult<{}, {}>", ok.dart_type(), err.dart_type())
            }
            DartType::Bytes => "$$typed_data.Uint8List".to_string(),
            DartType::List(inner) => format!("List<{}>", inner.dart_type()),
            DartType::Function { params, ret_ty } => format!(
                "{} Function({})",
                ret_ty.dart_type(),
                params
                    .iter()
                    .map(|ty| ty.dart_type())
                    .reduce(|acc, ty| acc + ", " + ty.as_str())
                    .unwrap_or_default()
            ),
            DartType::Class(class_id) => class_id.to_string(),
            DartType::Record(record_id) => record_id.to_string(),
            DartType::Enum(enum_id) => enum_id.to_string(),
            DartType::Callback(callback_id) => callback_id.to_string(),
            DartType::Custom(custom_type_id) => custom_type_id.to_string(),
            DartType::Builtin(builtin_id) => builtin_id.to_string(),
        }
    }
}
