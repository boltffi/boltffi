use crate::ir::abi::{AbiCallbackInvocation, AbiCallbackMethod, AbiParam, ParamRole, ReturnShape};
use crate::ir::definitions::{CallbackKind, CallbackMethodDef, CallbackTraitDef, ReturnDef};
use crate::ir::ids::CallbackId;
use crate::ir::ops::{ReadSeq, WriteSeq};
use crate::ir::plan::{AbiType, Transport};
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::super::ast::{
    CSharpArgumentList, CSharpAttribute, CSharpAttributeArg, CSharpClassName, CSharpExpression,
    CSharpIdentity, CSharpLiteral, CSharpLocalName, CSharpMethodName, CSharpParamName,
    CSharpParameter, CSharpParameterList, CSharpPropertyName, CSharpType, CSharpTypeReference,
};
use super::super::plan::{
    CFunctionName, CSharpAsyncCallbackEntryPlan, CSharpAsyncCallbackFailurePlan,
    CSharpAsyncCallbackFaultPlan, CSharpAsyncCallbackSuccessPlan, CSharpCallbackBridgeParamPlan,
    CSharpCallbackDelegatePlan, CSharpCallbackEntryPlan, CSharpCallbackMethodPlan,
    CSharpCallbackParamPlan, CSharpCallbackPlan, CSharpCallbackProxyCallPlan,
    CSharpCallbackProxyPlan, CSharpCallbackResultAssignmentPlan, CSharpCallbackResultCatchPlan,
    CSharpCallbackResultDecodePlan, CSharpCallbackResultOkPlan, CSharpClosureInvokePlan,
    CSharpClosureMethodPlan, CSharpClosurePlan, CSharpResultTypePlan, CSharpSyncCallbackEntryPlan,
    CSharpSyncCallbackOutInitializerPlan, CSharpSyncCallbackProxyPlan,
    CSharpSyncCallbackSuccessPlan, CSharpWireWriterPlan,
};
use super::lowerer::CSharpLowerer;
use super::{decode, encode, size, value};

const STATUS_OUT: &str = "__boltffiStatus";
const OUT_PTR: &str = "__boltffiOutPtr";
const OUT_LEN: &str = "__boltffiOutLen";
const RETURN_VALUE: &str = "__boltffiValue";
const RESULT_VALUE: &str = "__boltffiResult";
const ERROR_RESULT_VALUE: &str = "__boltffiErrorResult";
const RETURN_READER: &str = "__boltffiReader";
const INVOKE_LOCAL: &str = "__boltffiInvoke";
const ASYNC_COMPLETED: &str = "__boltffiCompleted";
const ASYNC_EXCEPTION: &str = "__boltffiException";
const ERROR_OUT_PTR: &str = "__boltffiErrorOutPtr";
const ERROR_OUT_LEN: &str = "__boltffiErrorOutLen";

#[derive(Debug, Clone)]
enum CallbackReturn {
    Void,
    Direct {
        public_type: CSharpType,
        native_type: CSharpType,
        native_out_type: CSharpType,
        sync_default_value: CSharpExpression,
        async_default_value: CSharpExpression,
        native_value: CallbackNativeValuePlan,
        public_value: CallbackPublicValuePlan,
        marshals_bool: bool,
    },
    Encoded {
        public_type: CSharpType,
        decode_expr: Option<CSharpExpression>,
        encode_ops: WriteSeq,
        decode_ops: Option<ReadSeq>,
        is_result: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallbackReturnMode {
    CallbackVtable,
    InlineClosure,
}

#[derive(Debug, Clone)]
struct CallbackMethodShape {
    name: CSharpMethodName,
    vtable_field: CSharpLocalName,
    is_async: bool,
    bridge_params: Vec<CSharpCallbackBridgeParamPlan>,
    ret: CallbackReturn,
}

impl CallbackMethodShape {
    fn public_return_type(&self) -> CSharpType {
        self.ret.public_type()
    }

    fn public_param_plans(&self) -> Vec<CSharpCallbackParamPlan> {
        self.bridge_params
            .iter()
            .map(|param| {
                let public_param = param.public_param();
                CSharpCallbackParamPlan {
                    csharp_type: public_param.csharp_type.clone(),
                    name: public_param.name.clone(),
                }
            })
            .collect()
    }

    fn public_param_list(&self) -> CSharpParameterList {
        callback_public_param_list(&self.bridge_params)
    }
}

#[derive(Debug, Clone)]
enum CallbackNativeValuePlan {
    Identity,
    BoolToByte,
    Cast(CSharpType),
    CallbackHandleCreate { bridge: CSharpClassName },
    HandleField,
}

#[derive(Debug, Clone)]
enum CallbackPublicValuePlan {
    Identity,
    ByteToBool,
    Cast(CSharpType),
    CallbackHandleWrap { bridge: CSharpClassName },
}

impl CallbackReturn {
    fn needs_wire_reader(&self) -> bool {
        matches!(
            self,
            Self::Encoded {
                decode_ops: Some(_),
                ..
            }
        )
    }

    fn needs_wire_writer(&self) -> bool {
        matches!(self, Self::Encoded { .. })
    }

    fn public_type(&self) -> CSharpType {
        match self {
            Self::Void => CSharpType::Void,
            Self::Direct { public_type, .. } | Self::Encoded { public_type, .. } => {
                public_type.clone()
            }
        }
    }
}

impl<'a> CSharpLowerer<'a> {
    pub(super) fn lower_callback(&self, callback: &CallbackTraitDef) -> CSharpCallbackPlan {
        let abi_callback = self
            .abi_callback_for(&callback.id)
            .expect("callback abi invocation missing");
        let methods = callback
            .methods
            .iter()
            .map(|method| {
                let abi_method = self
                    .abi_method_for(abi_callback, method)
                    .expect("callback abi method missing");
                self.lower_callback_method(method, abi_method)
            })
            .collect();
        CSharpCallbackPlan {
            public_name: self.callback_public_class_name(&callback.id),
            proxy_name: self.callback_proxy_class_name(&callback.id),
            bridge_name: self.callback_bridge_class_name(&callback.id),
            methods,
            register_fn: CFunctionName::new(abi_callback.register_fn.as_str().to_string()),
            create_fn: CFunctionName::new(abi_callback.create_fn.as_str().to_string()),
            has_async_methods: callback.methods.iter().any(CallbackMethodDef::is_async),
            needs_wire_reader: callback.methods.iter().any(|method| {
                self.callback_method_needs_wire_reader(
                    method,
                    abi_callback,
                    CallbackReturnMode::CallbackVtable,
                )
            }),
            needs_wire_writer: callback.methods.iter().any(|method| {
                self.callback_method_needs_wire_writer(
                    method,
                    abi_callback,
                    CallbackReturnMode::CallbackVtable,
                )
            }),
            needs_ffi_buf: callback.methods.iter().any(|method| {
                self.callback_method_needs_ffi_buf(
                    method,
                    abi_callback,
                    CallbackReturnMode::CallbackVtable,
                )
            }),
        }
    }

    pub(super) fn lower_closure(&self, callback: &CallbackTraitDef) -> CSharpClosurePlan {
        let abi_callback = self
            .abi_callback_for(&callback.id)
            .expect("closure abi invocation missing");
        let method = callback
            .methods
            .first()
            .expect("closure callback must have one method");
        let abi_method = self
            .abi_method_for(abi_callback, method)
            .expect("closure abi method missing");
        CSharpClosurePlan {
            public_name: self.callback_public_class_name(&callback.id),
            bridge_name: self.callback_bridge_class_name(&callback.id),
            method: self.lower_closure_method(method, abi_method),
            needs_wire_reader: self.callback_method_needs_wire_reader(
                method,
                abi_callback,
                CallbackReturnMode::InlineClosure,
            ),
            needs_wire_writer: self.callback_method_needs_wire_writer(
                method,
                abi_callback,
                CallbackReturnMode::InlineClosure,
            ),
            needs_ffi_buf: self.callback_method_needs_ffi_buf(
                method,
                abi_callback,
                CallbackReturnMode::InlineClosure,
            ),
        }
    }

    fn lower_callback_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpCallbackMethodPlan {
        let shape = self.lower_callback_method_shape(
            method,
            abi_method,
            CallbackReturnMode::CallbackVtable,
        );
        self.callback_method_plan_from_shape(&shape)
    }

    fn lower_closure_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpClosureMethodPlan {
        let shape =
            self.lower_callback_method_shape(method, abi_method, CallbackReturnMode::InlineClosure);
        self.closure_method_plan_from_shape(&shape)
    }

    fn callback_method_plan_from_shape(
        &self,
        shape: &CallbackMethodShape,
    ) -> CSharpCallbackMethodPlan {
        CSharpCallbackMethodPlan {
            name: shape.name.clone(),
            vtable_field: shape.vtable_field.clone(),
            return_type: shape.public_return_type(),
            is_async: shape.is_async,
            public_params: shape.public_param_plans(),
            entry: self.lower_callback_entry(shape),
            proxy: self.lower_callback_proxy(shape),
            delegates: self.lower_callback_delegates(shape),
        }
    }

    fn closure_method_plan_from_shape(
        &self,
        shape: &CallbackMethodShape,
    ) -> CSharpClosureMethodPlan {
        let native_return_type = self.inline_callback_native_return_type(&shape.ret);
        let mut native_params = CSharpParameterList::empty();
        native_params.push(CSharpParameter::bare(
            CSharpType::IntPtr,
            CSharpParamName::new("context"),
        ));
        for param in &shape.bridge_params {
            native_params.extend(param.native_params());
        }

        CSharpClosureMethodPlan {
            return_type: shape.public_return_type(),
            public_params: shape.public_param_plans(),
            native_return_type,
            native_params,
            bridge_params: shape.bridge_params.clone(),
            invoke: self.closure_invoke(&shape.ret, &shape.bridge_params),
        }
    }

    pub(super) fn callback_public_class_name(&self, callback_id: &CallbackId) -> CSharpClassName {
        let callback = self.ffi.catalog.resolve_callback(callback_id);
        match callback {
            Some(cb) if matches!(cb.kind, CallbackKind::Closure) => {
                let signature_id = callback_id
                    .as_str()
                    .strip_prefix("__Closure_")
                    .unwrap_or(callback_id.as_str());
                CSharpClassName::new(format!("Closure{signature_id}"))
            }
            _ => CSharpClassName::from_source(callback_id.as_str()),
        }
    }

    pub(super) fn callback_bridge_class_name(&self, callback_id: &CallbackId) -> CSharpClassName {
        let callback = self.ffi.catalog.resolve_callback(callback_id);
        match callback {
            Some(cb) if matches!(cb.kind, CallbackKind::Closure) => {
                let signature_id = callback_id
                    .as_str()
                    .strip_prefix("__Closure_")
                    .unwrap_or(callback_id.as_str());
                CSharpClassName::new(format!("Closure{signature_id}Bridge"))
            }
            _ => CSharpClassName::new(format!(
                "{}Bridge",
                CSharpClassName::from_source(callback_id.as_str())
            )),
        }
    }

    pub(super) fn callback_proxy_class_name(&self, callback_id: &CallbackId) -> CSharpClassName {
        let public_name = self.callback_public_class_name(callback_id);
        CSharpClassName::new(format!("{}Proxy", public_name.as_str()))
    }

    fn lower_callback_method_shape(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
        mode: CallbackReturnMode,
    ) -> CallbackMethodShape {
        CallbackMethodShape {
            name: (&method.id).into(),
            vtable_field: CSharpLocalName::new(abi_method.vtable_field.as_str()),
            is_async: method.is_async(),
            bridge_params: self.bridge_params(method, abi_method),
            ret: self.callback_return(&method.returns, &abi_method.returns, mode),
        }
    }

    fn lower_callback_entry(&self, shape: &CallbackMethodShape) -> CSharpCallbackEntryPlan {
        if shape.is_async {
            CSharpCallbackEntryPlan::Async(Box::new(CSharpAsyncCallbackEntryPlan {
                native_params: self.async_callback_native_params(&shape.name, &shape.bridge_params),
                bridge_params: shape.bridge_params.clone(),
                decoded_args: decoded_arg_list(&shape.bridge_params),
                invalid_handle_completion: self.async_failure_completion(&shape.ret),
                canceled_completion: self.async_failure_completion(&shape.ret),
                faulted_completion: self.async_fault_completion(&shape.ret),
                success_completion: self.async_success_completion(&shape.ret),
                catch_completion: self.async_failure_completion(&shape.ret),
            }))
        } else {
            let success = self.sync_callback_success(&shape.name, &shape.bridge_params, &shape.ret);
            CSharpCallbackEntryPlan::Sync(Box::new(CSharpSyncCallbackEntryPlan {
                native_params: self.sync_callback_native_params(&shape.bridge_params, &shape.ret),
                out_initializer: self.sync_out_initializer(&shape.ret),
                bridge_params: shape.bridge_params.clone(),
                success,
            }))
        }
    }

    fn lower_callback_proxy(&self, shape: &CallbackMethodShape) -> CSharpCallbackProxyPlan {
        let public_params = shape.public_param_list();
        if shape.is_async {
            return CSharpCallbackProxyPlan::AsyncUnsupported {
                public_params,
                result_type: if matches!(shape.ret, CallbackReturn::Void) {
                    None
                } else {
                    Some(shape.public_return_type())
                },
            };
        }

        let has_cleanup = shape
            .bridge_params
            .iter()
            .any(CSharpCallbackBridgeParamPlan::needs_wire_writer);
        let call = self.proxy_call(&shape.bridge_params, &shape.ret);
        CSharpCallbackProxyPlan::Sync(Box::new(CSharpSyncCallbackProxyPlan {
            public_params,
            return_type: shape.ret.public_type(),
            bridge_params: shape.bridge_params.clone(),
            has_cleanup,
            call,
        }))
    }

    fn lower_callback_delegates(&self, shape: &CallbackMethodShape) -> CSharpCallbackDelegatePlan {
        CSharpCallbackDelegatePlan {
            entry_params: if shape.is_async {
                self.async_callback_native_params(&shape.name, &shape.bridge_params)
            } else {
                self.sync_callback_native_params(&shape.bridge_params, &shape.ret)
            },
            completion_params: shape
                .is_async
                .then(|| self.async_completion_params(&shape.ret)),
            proxy_params: (!shape.is_async)
                .then(|| self.sync_callback_native_params(&shape.bridge_params, &shape.ret)),
        }
    }

    fn closure_invoke(
        &self,
        ret: &CallbackReturn,
        params: &[CSharpCallbackBridgeParamPlan],
    ) -> CSharpClosureInvokePlan {
        let decoded_args = decoded_arg_list(params);
        match ret {
            CallbackReturn::Void => CSharpClosureInvokePlan::Void { decoded_args },
            CallbackReturn::Direct {
                native_value,
                marshals_bool,
                ..
            } => {
                let value_expr = local_expr(RETURN_VALUE);
                let native_value_expr = if *marshals_bool {
                    value_expr
                } else {
                    self.native_value_expr(native_value, value_expr)
                };
                CSharpClosureInvokePlan::Direct {
                    decoded_args,
                    native_value_expr,
                }
            }
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                let result_assignment = if *is_result {
                    Some(self.result_assignment_plan(
                        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(
                            "impl_",
                        ))),
                        CSharpMethodName::new("Invoke"),
                        decoded_args.clone(),
                        encode_ops,
                    ))
                } else {
                    None
                };
                let writer = self.return_wire_writer_plan(
                    encode_ops,
                    "_returnWire",
                    "_returnBytes",
                    &root_local_rename(
                        if *is_result { "result" } else { "value" },
                        if *is_result {
                            RESULT_VALUE
                        } else {
                            RETURN_VALUE
                        },
                    ),
                );
                CSharpClosureInvokePlan::Encoded {
                    is_result: *is_result,
                    decoded_args,
                    result_assignment: result_assignment.map(Box::new),
                    writer: Box::new(writer),
                }
            }
        }
    }

    fn bridge_params(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> Vec<CSharpCallbackBridgeParamPlan> {
        method
            .params
            .iter()
            .filter_map(|param| {
                let abi_param = abi_method.params.iter().find(|abi_param| {
                    matches!(&abi_param.role, ParamRole::Input { .. })
                        && abi_param.name == param.name
                })?;
                Some(self.bridge_param(param, abi_param))
            })
            .collect()
    }

    fn bridge_param(
        &self,
        param: &crate::ir::definitions::ParamDef,
        abi_param: &AbiParam,
    ) -> CSharpCallbackBridgeParamPlan {
        let name: CSharpParamName = (&param.name).into();
        let public_type = self
            .lower_type(&param.type_expr)
            .expect("callback param type");
        let public_param = CSharpParameter::bare(public_type, name.clone());
        let ParamRole::Input {
            transport,
            len_param,
            decode_ops,
            encode_ops,
            ..
        } = &abi_param.role
        else {
            panic!("callback bridge param must be input");
        };

        if matches!(transport, Transport::Span(_)) {
            let len_param = len_param
                .as_ref()
                .expect("encoded callback param must have len param");
            let len_name: CSharpParamName = len_param.into();
            let decode_ops = decode_ops
                .as_ref()
                .expect("encoded callback param must have decode ops");
            let reader_name =
                CSharpLocalName::new(format!("__boltffi{}Reader", stripped_name(&name)));
            let decode_expr = self.decode_expr_from_reader(
                &self.normalize_custom_read_seq(decode_ops),
                CSharpExpression::Identity(CSharpIdentity::Local(reader_name.clone())),
            );
            let encode_ops = self.normalize_custom_write_seq(
                encode_ops
                    .as_ref()
                    .expect("encoded callback param must have encode ops"),
            );
            let writer = self.callback_param_wire_writer_plan(&encode_ops, &name);
            let pin_local =
                CSharpLocalName::new(format!("_{}Pin", stripped_name(&writer.param_name)));
            let ptr_local =
                CSharpLocalName::new(format!("_{}Ptr", stripped_name(&writer.param_name)));
            CSharpCallbackBridgeParamPlan::WireEncoded {
                public_param,
                native_ptr_param: CSharpParameter::bare(CSharpType::IntPtr, name),
                native_len_param: CSharpParameter::bare(CSharpType::UIntPtr, len_name),
                reader_local: reader_name,
                decoded_arg: decode_expr,
                writer: Box::new(writer),
                pin_local,
                ptr_local,
            }
        } else {
            let decode_expr = self.direct_decode_expr(&param.type_expr, &name);
            let proxy_expr =
                self.direct_proxy_arg_expr(&param.type_expr, &abi_param.abi_type, &name);
            CSharpCallbackBridgeParamPlan::Direct {
                public_param,
                native_param: self.native_param(&abi_param.abi_type, &name),
                decoded_arg: decode_expr,
                proxy_arg: proxy_expr,
            }
        }
    }

    fn callback_return(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
        mode: CallbackReturnMode,
    ) -> CallbackReturn {
        match returns {
            ReturnDef::Void => CallbackReturn::Void,
            ReturnDef::Value(ty) => self.callback_value_return(ty, ret_shape, mode),
            ReturnDef::Result { ok, err: _ } => {
                let public_type = if matches!(ok, TypeExpr::Void) {
                    CSharpType::Void
                } else {
                    self.lower_type(ok).expect("result ok type")
                };
                CallbackReturn::Encoded {
                    public_type,
                    decode_expr: None,
                    encode_ops: self.normalize_custom_write_seq(
                        ret_shape.encode_ops.as_ref().expect("result encode ops"),
                    ),
                    decode_ops: ret_shape
                        .decode_ops
                        .as_ref()
                        .map(|ops| self.normalize_custom_read_seq(ops)),
                    is_result: true,
                }
            }
        }
    }

    fn callback_value_return(
        &self,
        ty: &TypeExpr,
        ret_shape: &ReturnShape,
        mode: CallbackReturnMode,
    ) -> CallbackReturn {
        let public_type = self.lower_type(ty).expect("callback return type");
        match &ret_shape.transport {
            None => CallbackReturn::Void,
            Some(Transport::Scalar(origin)) => {
                let primitive = origin.primitive();
                let native_type = CSharpType::from(primitive);
                let marshals_bool = primitive == PrimitiveType::Bool;
                let native_out_type = if marshals_bool {
                    CSharpType::Byte
                } else {
                    native_type.clone()
                };
                CallbackReturn::Direct {
                    public_type,
                    native_type,
                    native_out_type,
                    sync_default_value: if marshals_bool {
                        CSharpExpression::Literal(CSharpLiteral::Int(0))
                    } else {
                        CSharpExpression::Literal(CSharpLiteral::Default)
                    },
                    async_default_value: if marshals_bool {
                        CSharpExpression::Literal(CSharpLiteral::Bool(false))
                    } else {
                        CSharpExpression::Literal(CSharpLiteral::Default)
                    },
                    native_value: self.direct_return_native_value_plan(ty, primitive),
                    public_value: self.direct_return_public_value_plan(ty, primitive),
                    marshals_bool,
                }
            }
            Some(Transport::Composite(layout)) if mode == CallbackReturnMode::InlineClosure => {
                let native_type =
                    CSharpType::Record(CSharpClassName::from(&layout.record_id).into());
                CallbackReturn::Direct {
                    public_type,
                    native_type: native_type.clone(),
                    native_out_type: native_type,
                    sync_default_value: CSharpExpression::Literal(CSharpLiteral::Default),
                    async_default_value: CSharpExpression::Literal(CSharpLiteral::Default),
                    native_value: CallbackNativeValuePlan::Identity,
                    public_value: CallbackPublicValuePlan::Identity,
                    marshals_bool: false,
                }
            }
            Some(Transport::Composite(_)) => {
                let decode_ops = ret_shape
                    .decode_ops
                    .as_ref()
                    .map(|ops| self.normalize_custom_read_seq(ops));
                let decode_expr = decode_ops.as_ref().map(|ops| {
                    self.decode_expr_from_reader(
                        ops,
                        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(
                            RETURN_READER,
                        ))),
                    )
                });
                CallbackReturn::Encoded {
                    public_type,
                    decode_expr,
                    encode_ops: self.normalize_custom_write_seq(
                        ret_shape.encode_ops.as_ref().expect("encoded return ops"),
                    ),
                    decode_ops,
                    is_result: false,
                }
            }
            Some(Transport::Callback { callback_id, .. }) => {
                let bridge = self.callback_bridge_class_name(callback_id);
                CallbackReturn::Direct {
                    public_type,
                    native_type: named_type("BoltFFICallbackHandle"),
                    native_out_type: named_type("BoltFFICallbackHandle"),
                    sync_default_value: type_member_expr("BoltFFICallbackHandle", "Null"),
                    async_default_value: type_member_expr("BoltFFICallbackHandle", "Null"),
                    native_value: CallbackNativeValuePlan::CallbackHandleCreate {
                        bridge: bridge.clone(),
                    },
                    public_value: CallbackPublicValuePlan::CallbackHandleWrap { bridge },
                    marshals_bool: false,
                }
            }
            Some(Transport::Handle { .. }) => CallbackReturn::Direct {
                public_type,
                native_type: CSharpType::IntPtr,
                native_out_type: CSharpType::IntPtr,
                sync_default_value: type_member_expr("IntPtr", "Zero"),
                async_default_value: type_member_expr("IntPtr", "Zero"),
                native_value: CallbackNativeValuePlan::HandleField,
                public_value: CallbackPublicValuePlan::Identity,
                marshals_bool: false,
            },
            Some(Transport::Span(_)) => {
                let decode_ops = ret_shape
                    .decode_ops
                    .as_ref()
                    .map(|ops| self.normalize_custom_read_seq(ops));
                let decode_expr = decode_ops.as_ref().map(|ops| {
                    self.decode_expr_from_reader(
                        ops,
                        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(
                            RETURN_READER,
                        ))),
                    )
                });
                CallbackReturn::Encoded {
                    public_type,
                    decode_expr,
                    encode_ops: self.normalize_custom_write_seq(
                        ret_shape.encode_ops.as_ref().expect("encoded return ops"),
                    ),
                    decode_ops,
                    is_result: false,
                }
            }
        }
    }

    fn result_assignment_plan(
        &self,
        receiver: CSharpExpression,
        method_name: CSharpMethodName,
        decoded_args: CSharpArgumentList,
        encode_ops: &WriteSeq,
    ) -> CSharpCallbackResultAssignmentPlan {
        let result_type = self.result_type_for_encode_ops(encode_ops);
        let ok =
            if result_type.ok_type.is_void() || result_type.ok_type == named_type("BoltFFIUnit") {
                CSharpCallbackResultOkPlan::Void {
                    receiver,
                    method_name,
                    args: decoded_args,
                }
            } else {
                CSharpCallbackResultOkPlan::Value {
                    receiver,
                    method_name,
                    args: decoded_args,
                }
            };
        CSharpCallbackResultAssignmentPlan {
            result_type,
            ok,
            catch: self.result_catch_for_encode_ops(encode_ops),
        }
    }

    fn result_type_for_encode_ops(&self, encode_ops: &WriteSeq) -> CSharpResultTypePlan {
        let Some(crate::ir::ops::WriteOp::Result { ok, err, .. }) = encode_ops.ops.first() else {
            return CSharpResultTypePlan {
                ok_type: named_type("object"),
                err_type: named_type("object"),
            };
        };
        let ok_type = self
            .result_branch_type(ok)
            .unwrap_or_else(|| named_type("BoltFFIUnit"));
        let err_type = self
            .result_branch_type(err)
            .unwrap_or_else(|| named_type("object"));
        CSharpResultTypePlan { ok_type, err_type }
    }

    fn result_branch_type(&self, seq: &WriteSeq) -> Option<CSharpType> {
        let op = seq.ops.first()?;
        match op {
            crate::ir::ops::WriteOp::Primitive { primitive, .. } => {
                Some(CSharpType::from(*primitive))
            }
            crate::ir::ops::WriteOp::String { .. } => Some(CSharpType::String),
            crate::ir::ops::WriteOp::Bytes { .. } => {
                Some(CSharpType::Array(Box::new(CSharpType::Byte)))
            }
            crate::ir::ops::WriteOp::Record { id, .. } => {
                Some(CSharpType::Record(CSharpClassName::from(id).into()))
            }
            crate::ir::ops::WriteOp::Enum { id, .. } => {
                Some(CSharpType::DataEnum(CSharpClassName::from(id).into()))
            }
            crate::ir::ops::WriteOp::Option { some, .. } => self
                .result_branch_type(some)
                .map(|inner| CSharpType::Nullable(Box::new(inner))),
            crate::ir::ops::WriteOp::Vec { element_type, .. } => {
                let inner = self.lower_type(element_type)?;
                Some(CSharpType::Array(Box::new(inner)))
            }
            crate::ir::ops::WriteOp::Custom { underlying, .. } => {
                self.result_branch_type(underlying)
            }
            _ => None,
        }
    }

    fn result_catch_for_encode_ops(
        &self,
        encode_ops: &WriteSeq,
    ) -> Option<CSharpCallbackResultCatchPlan> {
        let Some(crate::ir::ops::WriteOp::Result { err, .. }) = encode_ops.ops.first() else {
            return None;
        };
        let err_type = self
            .result_branch_type(err)
            .unwrap_or_else(|| named_type("object"));
        if let Some(exception) = self.error_exception_for_write_seq(err) {
            Some(CSharpCallbackResultCatchPlan::TypedException {
                exception_type: exception,
            })
        } else if err_type == CSharpType::String {
            Some(CSharpCallbackResultCatchPlan::ExceptionMessage)
        } else {
            None
        }
    }

    fn error_exception_for_write_seq(&self, seq: &WriteSeq) -> Option<CSharpType> {
        match seq.ops.first()? {
            crate::ir::ops::WriteOp::Record { id, .. }
                if self
                    .ffi
                    .catalog
                    .resolve_record(id)
                    .is_some_and(|record| record.is_error) =>
            {
                Some(named_type(&format!(
                    "{}Exception",
                    CSharpClassName::from(id)
                )))
            }
            crate::ir::ops::WriteOp::Enum { id, .. }
                if self
                    .ffi
                    .catalog
                    .resolve_enum(id)
                    .is_some_and(|enumeration| enumeration.is_error) =>
            {
                Some(named_type(&format!(
                    "{}Exception",
                    CSharpClassName::from(id)
                )))
            }
            crate::ir::ops::WriteOp::Custom { underlying, .. } => {
                self.error_exception_for_write_seq(underlying)
            }
            _ => None,
        }
    }

    fn callback_param_wire_writer_plan(
        &self,
        encode_ops: &WriteSeq,
        name: &CSharpParamName,
    ) -> CSharpWireWriterPlan {
        self.wire_writer_plan_with_renames(
            encode_ops,
            &format!("_{}Wire", stripped_name(name)),
            &format!("_{}Bytes", stripped_name(name)),
            name,
            &value::Renames::new(),
        )
    }

    fn return_wire_writer_plan(
        &self,
        encode_ops: &WriteSeq,
        writer_name: &str,
        bytes_name: &str,
        renames: &value::Renames,
    ) -> CSharpWireWriterPlan {
        self.wire_writer_plan_with_renames(
            encode_ops,
            writer_name,
            bytes_name,
            &CSharpParamName::new(RETURN_VALUE),
            renames,
        )
    }

    fn wire_writer_plan_with_renames(
        &self,
        encode_ops: &WriteSeq,
        writer_name: &str,
        bytes_name: &str,
        param_name: &CSharpParamName,
        renames: &value::Renames,
    ) -> CSharpWireWriterPlan {
        let writer_name = CSharpLocalName::new(writer_name);
        let bytes_name = CSharpLocalName::new(bytes_name);
        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        let writer = CSharpExpression::Identity(CSharpIdentity::Local(writer_name.clone()));
        CSharpWireWriterPlan {
            binding_name: writer_name,
            bytes_binding_name: bytes_name,
            param_name: param_name.clone(),
            size_expr: size::lower_size_expr(&encode_ops.size, renames, &mut size_locals),
            encode_stmts: encode::lower_encode_expr(
                encode_ops,
                &writer,
                renames,
                &mut encode_locals,
            ),
        }
    }

    fn result_decode_plan(
        &self,
        decode_ops: &ReadSeq,
        reader_name: &str,
    ) -> CSharpCallbackResultDecodePlan {
        let Some(crate::ir::ops::ReadOp::Result { ok, err, .. }) = decode_ops.ops.first() else {
            return CSharpCallbackResultDecodePlan {
                err_expr: CSharpExpression::Literal(CSharpLiteral::Null),
                ok_expr: None,
            };
        };
        let reader =
            CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(reader_name)));
        let mut locals = decode::DecodeLocalCounters::default();
        let err_expr = decode::lower_decode_expr(err, &reader, None, &self.namespace, &mut locals);
        let ok_expr = if ok.ops.is_empty() {
            None
        } else {
            Some(decode::lower_decode_expr(
                ok,
                &reader,
                None,
                &self.namespace,
                &mut locals,
            ))
        };
        CSharpCallbackResultDecodePlan { err_expr, ok_expr }
    }

    fn decode_expr_from_reader(
        &self,
        decode_ops: &ReadSeq,
        reader: CSharpExpression,
    ) -> CSharpExpression {
        let mut locals = decode::DecodeLocalCounters::default();
        decode::lower_decode_expr(decode_ops, &reader, None, &self.namespace, &mut locals)
    }

    fn sync_callback_success(
        &self,
        method_name: &CSharpMethodName,
        params: &[CSharpCallbackBridgeParamPlan],
        ret: &CallbackReturn,
    ) -> CSharpSyncCallbackSuccessPlan {
        let decoded_args = decoded_arg_list(params);
        match ret {
            CallbackReturn::Void => CSharpSyncCallbackSuccessPlan::Void { decoded_args },
            CallbackReturn::Direct { native_value, .. } => CSharpSyncCallbackSuccessPlan::Direct {
                decoded_args,
                native_value_expr: self.native_value_expr(native_value, local_expr(RETURN_VALUE)),
            },
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                let result_assignment = if *is_result {
                    Some(self.result_assignment_plan(
                        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(
                            "impl_",
                        ))),
                        method_name.clone(),
                        decoded_args.clone(),
                        encode_ops,
                    ))
                } else {
                    None
                };
                let writer = self.return_wire_writer_plan(
                    encode_ops,
                    "_returnWire",
                    "_returnBytes",
                    &root_local_rename(
                        if *is_result { "result" } else { "value" },
                        if *is_result {
                            RESULT_VALUE
                        } else {
                            RETURN_VALUE
                        },
                    ),
                );
                CSharpSyncCallbackSuccessPlan::Encoded {
                    is_result: *is_result,
                    decoded_args,
                    result_assignment: result_assignment.map(Box::new),
                    writer: Box::new(writer),
                }
            }
        }
    }

    fn proxy_call(
        &self,
        params: &[CSharpCallbackBridgeParamPlan],
        ret: &CallbackReturn,
    ) -> CSharpCallbackProxyCallPlan {
        let mut args = CSharpArgumentList::empty();
        args.push(CSharpExpression::MemberAccess {
            receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                CSharpLocalName::new("_handle"),
            ))),
            name: CSharpPropertyName::new("handle"),
        });
        for expr in params.iter().flat_map(|p| p.proxy_args()) {
            args.push(expr);
        }
        match ret {
            CallbackReturn::Void => CSharpCallbackProxyCallPlan::Void { args },
            CallbackReturn::Direct {
                native_out_type,
                public_value,
                ..
            } => CSharpCallbackProxyCallPlan::Direct {
                args,
                native_out_type: native_out_type.clone(),
                public_expr: self.public_value_expr(public_value, local_expr(OUT_PTR)),
            },
            CallbackReturn::Encoded {
                decode_expr,
                decode_ops,
                is_result,
                ..
            } => {
                let result_decode = if *is_result {
                    Some(
                        self.result_decode_plan(
                            decode_ops
                                .as_ref()
                                .expect("result callback return decode ops"),
                            RETURN_READER,
                        ),
                    )
                } else {
                    None
                };
                CSharpCallbackProxyCallPlan::Encoded {
                    args,
                    decode_expr: decode_expr.clone(),
                    result_decode,
                }
            }
        }
    }

    fn sync_callback_native_params(
        &self,
        params: &[CSharpCallbackBridgeParamPlan],
        ret: &CallbackReturn,
    ) -> CSharpParameterList {
        let mut decls = CSharpParameterList::empty();
        decls.push(CSharpParameter::bare(
            CSharpType::ULong,
            CSharpParamName::new("handle"),
        ));
        for param in params {
            decls.extend(param.native_params());
        }
        match ret {
            CallbackReturn::Void => {}
            CallbackReturn::Direct {
                native_out_type, ..
            } => {
                decls.push(CSharpParameter::out(
                    native_out_type.clone(),
                    CSharpParamName::new(OUT_PTR),
                ));
            }
            CallbackReturn::Encoded { .. } => {
                decls.push(CSharpParameter::out(
                    CSharpType::IntPtr,
                    CSharpParamName::new(OUT_PTR),
                ));
                decls.push(CSharpParameter::out(
                    CSharpType::UIntPtr,
                    CSharpParamName::new(OUT_LEN),
                ));
            }
        }
        decls.push(CSharpParameter::out(
            named_type("FfiStatus"),
            CSharpParamName::new(STATUS_OUT),
        ));
        decls
    }

    fn async_callback_native_params(
        &self,
        method_name: &CSharpMethodName,
        params: &[CSharpCallbackBridgeParamPlan],
    ) -> CSharpParameterList {
        let mut decls = CSharpParameterList::empty();
        decls.push(CSharpParameter::bare(
            CSharpType::ULong,
            CSharpParamName::new("handle"),
        ));
        for param in params {
            decls.extend(param.native_params());
        }
        decls.push(CSharpParameter::bare(
            named_type(&format!("{method_name}Completion")),
            CSharpParamName::new("callback"),
        ));
        decls.push(CSharpParameter::bare(
            CSharpType::ULong,
            CSharpParamName::new("callbackData"),
        ));
        decls
    }

    fn async_completion_params(&self, ret: &CallbackReturn) -> CSharpParameterList {
        let mut decls = CSharpParameterList::empty();
        decls.push(CSharpParameter::bare(
            CSharpType::ULong,
            CSharpParamName::new("callbackData"),
        ));
        match ret {
            CallbackReturn::Void => {}
            CallbackReturn::Direct {
                native_type,
                marshals_bool,
                ..
            } => {
                if *marshals_bool {
                    decls.push(CSharpParameter {
                        attributes: vec![marshal_as_i1()],
                        modifier: None,
                        csharp_type: CSharpType::Bool,
                        name: CSharpParamName::new("value"),
                    });
                } else {
                    decls.push(CSharpParameter::bare(
                        native_type.clone(),
                        CSharpParamName::new("value"),
                    ));
                }
            }
            CallbackReturn::Encoded { .. } => {
                decls.push(CSharpParameter::bare(
                    CSharpType::IntPtr,
                    CSharpParamName::new("valuePtr"),
                ));
                decls.push(CSharpParameter::bare(
                    CSharpType::UIntPtr,
                    CSharpParamName::new("valueLen"),
                ));
            }
        }
        decls.push(CSharpParameter::bare(
            named_type("FfiStatus"),
            CSharpParamName::new(STATUS_OUT),
        ));
        decls
    }

    fn sync_out_initializer(&self, ret: &CallbackReturn) -> CSharpSyncCallbackOutInitializerPlan {
        match ret {
            CallbackReturn::Void => CSharpSyncCallbackOutInitializerPlan::Void,
            CallbackReturn::Direct {
                sync_default_value, ..
            } => CSharpSyncCallbackOutInitializerPlan::Direct {
                default_value: sync_default_value.clone(),
            },
            CallbackReturn::Encoded { .. } => CSharpSyncCallbackOutInitializerPlan::Encoded,
        }
    }

    fn async_failure_completion(&self, ret: &CallbackReturn) -> CSharpAsyncCallbackFailurePlan {
        match ret {
            CallbackReturn::Void => CSharpAsyncCallbackFailurePlan::Void,
            CallbackReturn::Direct {
                async_default_value,
                ..
            } => CSharpAsyncCallbackFailurePlan::Direct {
                default_value: async_default_value.clone(),
            },
            CallbackReturn::Encoded { .. } => CSharpAsyncCallbackFailurePlan::Encoded,
        }
    }

    fn async_success_completion(&self, ret: &CallbackReturn) -> CSharpAsyncCallbackSuccessPlan {
        match ret {
            CallbackReturn::Void => CSharpAsyncCallbackSuccessPlan::Void,
            CallbackReturn::Direct {
                native_value,
                marshals_bool,
                ..
            } => {
                let task_result = CSharpExpression::MemberAccess {
                    receiver: Box::new(local_expr(ASYNC_COMPLETED)),
                    name: CSharpPropertyName::from_source("result"),
                };
                let native_value_expr = if *marshals_bool {
                    task_result
                } else {
                    self.native_value_expr(native_value, task_result)
                };
                CSharpAsyncCallbackSuccessPlan::Direct { native_value_expr }
            }
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                let writer = self.return_wire_writer_plan(
                    encode_ops,
                    "_returnWire",
                    "_returnBytes",
                    &root_local_rename(
                        if *is_result { "result" } else { "value" },
                        if *is_result {
                            RESULT_VALUE
                        } else {
                            RETURN_VALUE
                        },
                    ),
                );
                CSharpAsyncCallbackSuccessPlan::Encoded {
                    is_result: *is_result,
                    result_type: self.result_type_for_encode_ops(encode_ops),
                    writer: Box::new(writer),
                }
            }
        }
    }

    fn async_fault_completion(&self, ret: &CallbackReturn) -> CSharpAsyncCallbackFaultPlan {
        let CallbackReturn::Encoded {
            encode_ops,
            is_result: true,
            ..
        } = ret
        else {
            return CSharpAsyncCallbackFaultPlan::Failure(self.async_failure_completion(ret));
        };
        let result_ty = self.result_type_for_encode_ops(encode_ops);
        let Some(crate::ir::ops::WriteOp::Result { err, .. }) = encode_ops.ops.first() else {
            return CSharpAsyncCallbackFaultPlan::Failure(self.async_failure_completion(ret));
        };
        let err_type = self
            .result_branch_type(err)
            .unwrap_or_else(|| named_type("object"));
        let (exception_type, error_value_expr, fallback) =
            if let Some(exception_ty) = self.error_exception_for_write_seq(err) {
                (
                    Some(exception_ty),
                    Box::new(CSharpExpression::MemberAccess {
                        receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                            CSharpLocalName::new("__boltffiTypedException"),
                        ))),
                        name: CSharpPropertyName::from_source("error"),
                    }),
                    Some(self.async_failure_completion(ret)),
                )
            } else if err_type == CSharpType::String {
                (
                    None,
                    Box::new(CSharpExpression::MemberAccess {
                        receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                            CSharpLocalName::new(ASYNC_EXCEPTION),
                        ))),
                        name: CSharpPropertyName::from_source("message"),
                    }),
                    None,
                )
            } else {
                return CSharpAsyncCallbackFaultPlan::Failure(self.async_failure_completion(ret));
            };
        let writer = self.return_wire_writer_plan(
            encode_ops,
            "_returnErrorWire",
            "_returnErrorBytes",
            &root_local_rename("result", ERROR_RESULT_VALUE),
        );
        CSharpAsyncCallbackFaultPlan::EncodedResult {
            exception_type,
            error_value_expr,
            result_type: Box::new(result_ty),
            writer: Box::new(writer),
            fallback,
        }
    }

    fn inline_callback_native_return_type(&self, ret: &CallbackReturn) -> CSharpType {
        match ret {
            CallbackReturn::Void => CSharpType::Void,
            CallbackReturn::Direct { native_type, .. } => native_type.clone(),
            CallbackReturn::Encoded { .. } => named_type("FfiBuf"),
        }
    }

    fn native_param(&self, abi_type: &AbiType, name: &CSharpParamName) -> CSharpParameter {
        match abi_type {
            AbiType::Bool => CSharpParameter {
                attributes: vec![marshal_as_i1()],
                modifier: None,
                csharp_type: CSharpType::Bool,
                name: name.clone(),
            },
            _ => {
                CSharpParameter::bare(self.native_csharp_type_for_abi_type(abi_type), name.clone())
            }
        }
    }

    fn native_csharp_type_for_abi_type(&self, abi_type: &AbiType) -> CSharpType {
        match abi_type {
            AbiType::Void => CSharpType::Void,
            AbiType::Bool => CSharpType::Bool,
            AbiType::I8 => CSharpType::SByte,
            AbiType::U8 => CSharpType::Byte,
            AbiType::I16 => CSharpType::Short,
            AbiType::U16 => CSharpType::UShort,
            AbiType::I32 => CSharpType::Int,
            AbiType::U32 => CSharpType::UInt,
            AbiType::I64 => CSharpType::Long,
            AbiType::U64 => CSharpType::ULong,
            AbiType::ISize => CSharpType::NInt,
            AbiType::USize => CSharpType::NUInt,
            AbiType::F32 => CSharpType::Float,
            AbiType::F64 => CSharpType::Double,
            AbiType::Pointer(_) => CSharpType::IntPtr,
            AbiType::OwnedBuffer => named_type("FfiBuf"),
            AbiType::Handle(_) => CSharpType::IntPtr,
            AbiType::CallbackHandle => named_type("BoltFFICallbackHandle"),
            AbiType::Struct(id) => CSharpType::Record(CSharpClassName::from(id).into()),
            AbiType::InlineCallbackFn { .. } => CSharpType::IntPtr,
        }
    }

    fn direct_decode_expr(&self, type_expr: &TypeExpr, name: &CSharpParamName) -> CSharpExpression {
        let param = CSharpExpression::Identity(CSharpIdentity::Param(name.clone()));
        if self.is_c_style_enum_type(type_expr) {
            let ty = self.lower_type(type_expr).expect("enum type");
            CSharpExpression::Cast {
                target: ty,
                inner: Box::new(param),
            }
        } else {
            param
        }
    }

    fn direct_proxy_arg_expr(
        &self,
        type_expr: &TypeExpr,
        abi_type: &AbiType,
        name: &CSharpParamName,
    ) -> CSharpExpression {
        let param = CSharpExpression::Identity(CSharpIdentity::Param(name.clone()));
        if self.is_c_style_enum_type(type_expr) {
            CSharpExpression::Cast {
                target: self.native_csharp_type_for_abi_type(abi_type),
                inner: Box::new(param),
            }
        } else {
            param
        }
    }

    fn direct_return_native_value_plan(
        &self,
        type_expr: &TypeExpr,
        primitive: PrimitiveType,
    ) -> CallbackNativeValuePlan {
        if primitive == PrimitiveType::Bool {
            CallbackNativeValuePlan::BoolToByte
        } else if self.is_c_style_enum_type(type_expr) {
            CallbackNativeValuePlan::Cast(CSharpType::from(primitive))
        } else {
            CallbackNativeValuePlan::Identity
        }
    }

    fn direct_return_public_value_plan(
        &self,
        type_expr: &TypeExpr,
        primitive: PrimitiveType,
    ) -> CallbackPublicValuePlan {
        if primitive == PrimitiveType::Bool {
            CallbackPublicValuePlan::ByteToBool
        } else if self.is_c_style_enum_type(type_expr) {
            let ty = self.lower_type(type_expr).expect("enum type");
            CallbackPublicValuePlan::Cast(ty)
        } else {
            CallbackPublicValuePlan::Identity
        }
    }

    fn native_value_expr(
        &self,
        plan: &CallbackNativeValuePlan,
        value: CSharpExpression,
    ) -> CSharpExpression {
        match plan {
            CallbackNativeValuePlan::Identity => value,
            CallbackNativeValuePlan::BoolToByte => CSharpExpression::Ternary {
                cond: Box::new(value),
                then: Box::new(CSharpExpression::Cast {
                    target: CSharpType::Byte,
                    inner: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(1))),
                }),
                otherwise: Box::new(CSharpExpression::Cast {
                    target: CSharpType::Byte,
                    inner: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(0))),
                }),
            },
            CallbackNativeValuePlan::Cast(target) => CSharpExpression::Cast {
                target: target.clone(),
                inner: Box::new(value),
            },
            CallbackNativeValuePlan::CallbackHandleCreate { bridge } => {
                CSharpExpression::MethodCall {
                    receiver: Box::new(type_ref_expr(bridge.clone())),
                    method: CSharpMethodName::new("Create"),
                    type_args: vec![],
                    args: vec![value].into(),
                }
            }
            CallbackNativeValuePlan::HandleField => CSharpExpression::MemberAccess {
                receiver: Box::new(value),
                name: CSharpPropertyName::from_source("handle"),
            },
        }
    }

    fn public_value_expr(
        &self,
        plan: &CallbackPublicValuePlan,
        value: CSharpExpression,
    ) -> CSharpExpression {
        match plan {
            CallbackPublicValuePlan::Identity => value,
            CallbackPublicValuePlan::ByteToBool => CSharpExpression::Binary {
                op: super::super::ast::CSharpBinaryOp::Ne,
                left: Box::new(value),
                right: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(0))),
            },
            CallbackPublicValuePlan::Cast(target) => CSharpExpression::Cast {
                target: target.clone(),
                inner: Box::new(value),
            },
            CallbackPublicValuePlan::CallbackHandleWrap { bridge } => {
                CSharpExpression::MethodCall {
                    receiver: Box::new(type_ref_expr(bridge.clone())),
                    method: CSharpMethodName::new("Wrap"),
                    type_args: vec![],
                    args: vec![value].into(),
                }
            }
        }
    }

    fn is_c_style_enum_type(&self, type_expr: &TypeExpr) -> bool {
        matches!(
            type_expr,
            TypeExpr::Enum(id)
                if self
                    .ffi
                    .catalog
                    .resolve_enum(id)
                    .is_some_and(|e| matches!(e.repr, crate::ir::definitions::EnumRepr::CStyle { .. }))
        )
    }

    fn abi_callback_for(&self, callback_id: &CallbackId) -> Option<&AbiCallbackInvocation> {
        self.abi
            .callbacks
            .iter()
            .find(|callback| callback.callback_id == *callback_id)
    }

    fn abi_method_for<'b>(
        &self,
        abi_callback: &'b AbiCallbackInvocation,
        method: &CallbackMethodDef,
    ) -> Option<&'b AbiCallbackMethod> {
        abi_callback
            .methods
            .iter()
            .find(|candidate| candidate.id == method.id)
    }

    fn callback_method_needs_wire_reader(
        &self,
        method: &CallbackMethodDef,
        abi_callback: &AbiCallbackInvocation,
        mode: CallbackReturnMode,
    ) -> bool {
        let Some(abi_method) = self.abi_method_for(abi_callback, method) else {
            return false;
        };
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(&method.returns, &abi_method.returns, mode);
        params
            .iter()
            .any(CSharpCallbackBridgeParamPlan::needs_wire_reader)
            || ret.needs_wire_reader()
    }

    fn callback_method_needs_wire_writer(
        &self,
        method: &CallbackMethodDef,
        abi_callback: &AbiCallbackInvocation,
        mode: CallbackReturnMode,
    ) -> bool {
        let Some(abi_method) = self.abi_method_for(abi_callback, method) else {
            return false;
        };
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(&method.returns, &abi_method.returns, mode);
        params
            .iter()
            .any(CSharpCallbackBridgeParamPlan::needs_wire_writer)
            || ret.needs_wire_writer()
    }

    fn callback_method_needs_ffi_buf(
        &self,
        method: &CallbackMethodDef,
        abi_callback: &AbiCallbackInvocation,
        mode: CallbackReturnMode,
    ) -> bool {
        let Some(abi_method) = self.abi_method_for(abi_callback, method) else {
            return false;
        };
        matches!(
            self.callback_return(&method.returns, &abi_method.returns, mode),
            CallbackReturn::Encoded { .. }
        )
    }
}

fn decoded_arg_list(params: &[CSharpCallbackBridgeParamPlan]) -> CSharpArgumentList {
    params
        .iter()
        .map(|param| param.decoded_arg().clone())
        .collect::<Vec<_>>()
        .into()
}

fn callback_public_param_list(params: &[CSharpCallbackBridgeParamPlan]) -> CSharpParameterList {
    params
        .iter()
        .map(|param| param.public_param().clone())
        .collect::<Vec<_>>()
        .into()
}

fn local_expr(name: &str) -> CSharpExpression {
    CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(name)))
}

fn type_ref_expr(name: CSharpClassName) -> CSharpExpression {
    CSharpExpression::TypeRef(CSharpTypeReference::Plain(name))
}

fn type_member_expr(type_name: &str, member_name: &str) -> CSharpExpression {
    CSharpExpression::MemberAccess {
        receiver: Box::new(type_ref_expr(CSharpClassName::new(type_name))),
        name: CSharpPropertyName::new(member_name),
    }
}

fn stripped_name(name: &CSharpParamName) -> &str {
    name.as_str().strip_prefix('@').unwrap_or(name.as_str())
}

fn named_type(name: &str) -> CSharpType {
    CSharpType::Named(CSharpTypeReference::Plain(CSharpClassName::new(name)))
}

fn marshal_as_i1() -> CSharpAttribute {
    CSharpAttribute {
        name: CSharpClassName::new("MarshalAs"),
        args: vec![CSharpAttributeArg::Positional(
            CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                    CSharpClassName::new("UnmanagedType"),
                ))),
                name: CSharpPropertyName::from_source("i1"),
            },
        )],
    }
}

fn root_local_rename(ir_name: &str, csharp_name: &str) -> value::Renames {
    let mut renames = value::Renames::new();
    renames.insert(
        ir_name.to_string(),
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(csharp_name))),
    );
    renames
}
