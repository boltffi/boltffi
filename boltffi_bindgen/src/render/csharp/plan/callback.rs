use super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpExpression, CSharpIdentity, CSharpLocalName,
    CSharpMethodName, CSharpParamName, CSharpParameter, CSharpParameterList, CSharpPropertyName,
    CSharpType,
};
use super::CFunctionName;
use super::callable::CSharpWireWriterPlan;

#[derive(Debug, Clone)]
pub struct CSharpCallbackPlan {
    pub public_name: CSharpClassName,
    pub proxy_name: CSharpClassName,
    pub bridge_name: CSharpClassName,
    pub methods: Vec<CSharpCallbackMethodPlan>,
    pub register_fn: CFunctionName,
    pub create_fn: CFunctionName,
    pub has_async_methods: bool,
    pub needs_wire_reader: bool,
    pub needs_wire_writer: bool,
    pub needs_ffi_buf: bool,
}

#[derive(Debug, Clone)]
pub struct CSharpClosurePlan {
    pub public_name: CSharpClassName,
    pub bridge_name: CSharpClassName,
    pub method: CSharpClosureMethodPlan,
    pub needs_wire_reader: bool,
    pub needs_wire_writer: bool,
    pub needs_ffi_buf: bool,
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackMethodPlan {
    pub name: CSharpMethodName,
    pub vtable_field: CSharpLocalName,
    pub return_type: CSharpType,
    pub is_async: bool,
    pub public_params: Vec<CSharpCallbackParamPlan>,
    pub entry: CSharpCallbackEntryPlan,
    pub proxy: CSharpCallbackProxyPlan,
    pub delegates: CSharpCallbackDelegatePlan,
}

#[derive(Debug, Clone)]
pub struct CSharpClosureMethodPlan {
    pub return_type: CSharpType,
    pub public_params: Vec<CSharpCallbackParamPlan>,
    pub native_return_type: CSharpType,
    pub native_params: CSharpParameterList,
    pub bridge_params: Vec<CSharpCallbackBridgeParamPlan>,
    pub invoke: CSharpClosureInvokePlan,
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackParamPlan {
    pub csharp_type: CSharpType,
    pub name: CSharpParamName,
}

#[derive(Debug, Clone)]
pub enum CSharpClosureInvokePlan {
    Void {
        decoded_args: CSharpArgumentList,
    },
    Direct {
        decoded_args: CSharpArgumentList,
        native_value_expr: CSharpExpression,
    },
    Encoded {
        is_result: bool,
        decoded_args: CSharpArgumentList,
        result_assignment: Option<Box<CSharpCallbackResultAssignmentPlan>>,
        writer: Box<CSharpWireWriterPlan>,
    },
}

#[derive(Debug, Clone)]
pub enum CSharpCallbackEntryPlan {
    Sync(Box<CSharpSyncCallbackEntryPlan>),
    Async(Box<CSharpAsyncCallbackEntryPlan>),
}

#[derive(Debug, Clone)]
pub struct CSharpSyncCallbackEntryPlan {
    pub native_params: CSharpParameterList,
    pub out_initializer: CSharpSyncCallbackOutInitializerPlan,
    pub bridge_params: Vec<CSharpCallbackBridgeParamPlan>,
    pub success: CSharpSyncCallbackSuccessPlan,
}

#[derive(Debug, Clone)]
pub enum CSharpSyncCallbackOutInitializerPlan {
    Void,
    Direct { default_value: CSharpExpression },
    Encoded,
}

#[derive(Debug, Clone)]
pub enum CSharpSyncCallbackSuccessPlan {
    Void {
        decoded_args: CSharpArgumentList,
    },
    Direct {
        decoded_args: CSharpArgumentList,
        native_value_expr: CSharpExpression,
    },
    Encoded {
        is_result: bool,
        decoded_args: CSharpArgumentList,
        result_assignment: Option<Box<CSharpCallbackResultAssignmentPlan>>,
        writer: Box<CSharpWireWriterPlan>,
    },
}

#[derive(Debug, Clone)]
pub struct CSharpAsyncCallbackEntryPlan {
    pub native_params: CSharpParameterList,
    pub bridge_params: Vec<CSharpCallbackBridgeParamPlan>,
    pub decoded_args: CSharpArgumentList,
    pub invalid_handle_completion: CSharpAsyncCallbackFailurePlan,
    pub canceled_completion: CSharpAsyncCallbackFailurePlan,
    pub faulted_completion: CSharpAsyncCallbackFaultPlan,
    pub success_completion: CSharpAsyncCallbackSuccessPlan,
    pub catch_completion: CSharpAsyncCallbackFailurePlan,
}

#[derive(Debug, Clone)]
pub enum CSharpAsyncCallbackFailurePlan {
    Void,
    Direct { default_value: CSharpExpression },
    Encoded,
}

#[derive(Debug, Clone)]
pub enum CSharpAsyncCallbackSuccessPlan {
    Void,
    Direct {
        native_value_expr: CSharpExpression,
    },
    Encoded {
        is_result: bool,
        result_type: CSharpResultTypePlan,
        writer: Box<CSharpWireWriterPlan>,
    },
}

#[derive(Debug, Clone)]
pub enum CSharpAsyncCallbackFaultPlan {
    Failure(CSharpAsyncCallbackFailurePlan),
    EncodedResult {
        exception_type: Option<CSharpType>,
        error_value_expr: Box<CSharpExpression>,
        result_type: Box<CSharpResultTypePlan>,
        writer: Box<CSharpWireWriterPlan>,
        fallback: Option<CSharpAsyncCallbackFailurePlan>,
    },
}

#[derive(Debug, Clone)]
pub enum CSharpCallbackProxyPlan {
    AsyncUnsupported {
        public_params: CSharpParameterList,
        result_type: Option<CSharpType>,
    },
    Sync(Box<CSharpSyncCallbackProxyPlan>),
}

#[derive(Debug, Clone)]
pub struct CSharpSyncCallbackProxyPlan {
    pub public_params: CSharpParameterList,
    pub return_type: CSharpType,
    pub bridge_params: Vec<CSharpCallbackBridgeParamPlan>,
    pub has_cleanup: bool,
    pub call: CSharpCallbackProxyCallPlan,
}

#[derive(Debug, Clone)]
pub enum CSharpCallbackProxyCallPlan {
    Void {
        args: CSharpArgumentList,
    },
    Direct {
        args: CSharpArgumentList,
        native_out_type: CSharpType,
        public_expr: CSharpExpression,
    },
    Encoded {
        args: CSharpArgumentList,
        decode_expr: Option<CSharpExpression>,
        result_decode: Option<CSharpCallbackResultDecodePlan>,
    },
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackDelegatePlan {
    pub entry_params: CSharpParameterList,
    pub completion_params: Option<CSharpParameterList>,
    pub proxy_params: Option<CSharpParameterList>,
}

#[derive(Debug, Clone)]
pub struct CSharpResultTypePlan {
    pub ok_type: CSharpType,
    pub err_type: CSharpType,
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackResultAssignmentPlan {
    pub result_type: CSharpResultTypePlan,
    pub ok: CSharpCallbackResultOkPlan,
    pub catch: Option<CSharpCallbackResultCatchPlan>,
}

#[derive(Debug, Clone)]
pub enum CSharpCallbackResultOkPlan {
    Void {
        receiver: CSharpExpression,
        method_name: CSharpMethodName,
        args: CSharpArgumentList,
    },
    Value {
        receiver: CSharpExpression,
        method_name: CSharpMethodName,
        args: CSharpArgumentList,
    },
}

#[derive(Debug, Clone)]
pub enum CSharpCallbackResultCatchPlan {
    TypedException { exception_type: CSharpType },
    ExceptionMessage,
}

#[derive(Debug, Clone)]
pub struct CSharpCallbackResultDecodePlan {
    pub err_expr: CSharpExpression,
    pub ok_expr: Option<CSharpExpression>,
}

#[derive(Debug, Clone)]
pub enum CSharpCallbackBridgeParamPlan {
    Direct {
        public_param: CSharpParameter,
        native_param: CSharpParameter,
        decoded_arg: CSharpExpression,
        proxy_arg: CSharpExpression,
    },
    WireEncoded {
        public_param: CSharpParameter,
        native_ptr_param: CSharpParameter,
        native_len_param: CSharpParameter,
        reader_local: CSharpLocalName,
        decoded_arg: CSharpExpression,
        writer: Box<CSharpWireWriterPlan>,
        pin_local: CSharpLocalName,
        ptr_local: CSharpLocalName,
    },
}

impl CSharpCallbackBridgeParamPlan {
    pub fn public_param(&self) -> &CSharpParameter {
        match self {
            Self::Direct { public_param, .. } | Self::WireEncoded { public_param, .. } => {
                public_param
            }
        }
    }

    pub fn native_params(&self) -> Vec<CSharpParameter> {
        match self {
            Self::Direct { native_param, .. } => vec![native_param.clone()],
            Self::WireEncoded {
                native_ptr_param,
                native_len_param,
                ..
            } => vec![native_ptr_param.clone(), native_len_param.clone()],
        }
    }

    pub fn decoded_arg(&self) -> &CSharpExpression {
        match self {
            Self::Direct { decoded_arg, .. } | Self::WireEncoded { decoded_arg, .. } => decoded_arg,
        }
    }

    pub fn proxy_args(&self) -> CSharpArgumentList {
        match self {
            Self::Direct { proxy_arg, .. } => vec![proxy_arg.clone()].into(),
            Self::WireEncoded {
                writer, ptr_local, ..
            } => vec![
                local_expr(ptr_local),
                CSharpExpression::Cast {
                    target: CSharpType::UIntPtr,
                    inner: Box::new(bytes_length_expr(&writer.bytes_binding_name)),
                },
            ]
            .into(),
        }
    }

    pub fn needs_wire_reader(&self) -> bool {
        matches!(self, Self::WireEncoded { .. })
    }

    pub fn needs_wire_writer(&self) -> bool {
        matches!(self, Self::WireEncoded { .. })
    }
}

fn local_expr(name: &CSharpLocalName) -> CSharpExpression {
    CSharpExpression::Identity(CSharpIdentity::Local(name.clone()))
}

fn bytes_length_expr(bytes_local: &CSharpLocalName) -> CSharpExpression {
    CSharpExpression::MemberAccess {
        receiver: Box::new(local_expr(bytes_local)),
        name: CSharpPropertyName::from_source("length"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::{CSharpLiteral, CSharpStatement};
    use super::*;

    fn param_expr(name: &CSharpParamName) -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Param(name.clone()))
    }

    fn local(name: &str) -> CSharpLocalName {
        CSharpLocalName::new(name)
    }

    #[test]
    fn direct_bridge_param_exposes_typed_signature_and_args() {
        let name = CSharpParamName::from_source("value");
        let plan = CSharpCallbackBridgeParamPlan::Direct {
            public_param: CSharpParameter::bare(CSharpType::Int, name.clone()),
            native_param: CSharpParameter::bare(CSharpType::Int, name.clone()),
            decoded_arg: param_expr(&name),
            proxy_arg: param_expr(&name),
        };

        assert_eq!(plan.public_param().to_string(), "int value");
        assert_eq!(
            plan.native_params()
                .into_iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>(),
            vec!["int value"]
        );
        assert_eq!(plan.decoded_arg().to_string(), "value");
        assert_eq!(plan.proxy_args().to_string(), "value");
        assert!(!plan.needs_wire_reader());
        assert!(!plan.needs_wire_writer());
    }

    #[test]
    fn wire_encoded_bridge_param_exposes_reader_writer_pin_and_cleanup_parts() {
        let value = CSharpParamName::from_source("value");
        let value_len = CSharpParamName::new("valueLen");
        let writer = CSharpWireWriterPlan {
            binding_name: local("_valueWire"),
            bytes_binding_name: local("_valueBytes"),
            param_name: value.clone(),
            size_expr: CSharpExpression::Literal(CSharpLiteral::Int(4)),
            encode_stmts: vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(local_expr(&local("_valueWire"))),
                method: CSharpMethodName::new("WriteI32"),
                type_args: vec![],
                args: vec![param_expr(&value)].into(),
            })],
        };
        let plan = CSharpCallbackBridgeParamPlan::WireEncoded {
            public_param: CSharpParameter::bare(CSharpType::String, value.clone()),
            native_ptr_param: CSharpParameter::bare(CSharpType::IntPtr, value.clone()),
            native_len_param: CSharpParameter::bare(CSharpType::UIntPtr, value_len),
            reader_local: local("__boltffiValueReader"),
            decoded_arg: local_expr(&local("__boltffiValueReader")),
            writer: Box::new(writer),
            pin_local: local("_valuePin"),
            ptr_local: local("_valuePtr"),
        };

        assert_eq!(plan.public_param().to_string(), "string value");
        assert_eq!(
            plan.native_params()
                .into_iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>(),
            vec!["IntPtr value", "UIntPtr valueLen"]
        );
        assert_eq!(
            plan.proxy_args().to_string(),
            "_valuePtr, (UIntPtr)_valueBytes.Length"
        );
        let CSharpCallbackBridgeParamPlan::WireEncoded {
            reader_local,
            writer,
            pin_local,
            ptr_local,
            ..
        } = &plan
        else {
            panic!("expected wire-encoded bridge param");
        };
        assert_eq!(reader_local.to_string(), "__boltffiValueReader");
        assert_eq!(writer.binding_name.to_string(), "_valueWire");
        assert_eq!(writer.bytes_binding_name.to_string(), "_valueBytes");
        assert_eq!(pin_local.to_string(), "_valuePin");
        assert_eq!(ptr_local.to_string(), "_valuePtr");
        assert!(plan.needs_wire_reader());
        assert!(plan.needs_wire_writer());
    }
}
