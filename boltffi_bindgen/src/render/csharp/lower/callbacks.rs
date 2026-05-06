use crate::ir::abi::{AbiCallbackInvocation, AbiCallbackMethod, AbiParam, ParamRole, ReturnShape};
use crate::ir::definitions::{CallbackKind, CallbackMethodDef, CallbackTraitDef, ReturnDef};
use crate::ir::ids::CallbackId;
use crate::ir::ops::{ReadSeq, WriteSeq};
use crate::ir::plan::{AbiType, Transport};
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::super::ast::{
    CSharpArgumentList, CSharpAttribute, CSharpAttributeArg, CSharpClassName, CSharpExpression,
    CSharpIdentity, CSharpLocalName, CSharpMethodName, CSharpParamName, CSharpParameter,
    CSharpParameterList, CSharpPropertyName, CSharpType, CSharpTypeReference,
};
use super::super::plan::{
    CFunctionName, CSharpAsyncCallbackEntryPlan, CSharpAsyncCallbackFailurePlan,
    CSharpAsyncCallbackFaultPlan, CSharpAsyncCallbackSuccessPlan, CSharpCallbackBridgeParamPlan,
    CSharpCallbackDelegatePlan, CSharpCallbackEntryPlan, CSharpCallbackMethodPlan,
    CSharpCallbackParamPlan, CSharpCallbackPlan, CSharpCallbackProxyCallPlan,
    CSharpCallbackProxyPlan, CSharpClosureInvokePlan, CSharpClosureMethodPlan, CSharpClosurePlan,
    CSharpSyncCallbackEntryPlan, CSharpSyncCallbackOutInitializerPlan, CSharpSyncCallbackProxyPlan,
    CSharpSyncCallbackSuccessPlan, CSharpWireWriterPlan,
};
use super::lowerer::CSharpLowerer;
use super::{decode, encode, size, value};

const STATUS_OK: &str = "new FfiStatus { code = 0 }";
const STATUS_INTERNAL_ERROR: &str = "new FfiStatus { code = 100 }";
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
        public_type: String,
        native_type: CSharpType,
        native_out_type: CSharpType,
        default_value: String,
        native_expr: String,
        public_expr: String,
        marshals_bool: bool,
    },
    Encoded {
        public_type: String,
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

    fn public_type(&self) -> String {
        match self {
            Self::Void => "void".to_string(),
            Self::Direct { public_type, .. } | Self::Encoded { public_type, .. } => {
                public_type.clone()
            }
        }
    }

    fn async_task_type(&self) -> String {
        match self {
            Self::Void => "global::System.Threading.Tasks.Task".to_string(),
            other => format!(
                "global::System.Threading.Tasks.Task<{}>",
                other.public_type()
            ),
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
        CSharpCallbackMethodPlan {
            name: (&method.id).into(),
            vtable_field: CSharpLocalName::new(abi_method.vtable_field.as_str()),
            return_type: self.callback_public_return_type(&method.returns),
            is_async: method.is_async(),
            public_params: self.public_param_plans(&method.params),
            entry: self.lower_callback_entry(method, abi_method),
            proxy: self.lower_callback_proxy(method, abi_method),
            delegates: self.lower_callback_delegates(method, abi_method),
        }
    }

    fn lower_closure_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpClosureMethodPlan {
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::InlineClosure,
        );
        let native_return_type = self.inline_callback_native_return_type(&ret);
        let mut native_params = CSharpParameterList::empty();
        native_params.push(CSharpParameter::bare(
            CSharpType::IntPtr,
            CSharpParamName::new("context"),
        ));
        for param in &params {
            native_params.extend(param.native_params());
        }
        let marshals_return_bool = matches!(
            ret,
            CallbackReturn::Direct {
                marshals_bool: true,
                ..
            }
        );

        CSharpClosureMethodPlan {
            return_type: self.callback_public_return_type(&method.returns),
            public_params: self.public_param_plans(&method.params),
            native_return_type,
            native_params,
            marshals_return_bool,
            bridge_params: params.clone(),
            invoke: self.closure_invoke(&ret, &params),
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

    fn lower_callback_entry(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpCallbackEntryPlan {
        let method_name: CSharpMethodName = (&method.id).into();
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        if method.is_async() {
            CSharpCallbackEntryPlan::Async(CSharpAsyncCallbackEntryPlan {
                native_params: self.async_callback_native_params(&method_name, &params),
                bridge_params: params.clone(),
                decoded_args: decoded_arg_list(&params),
                invalid_handle_completion: self.async_failure_completion(&ret),
                canceled_completion: self.async_failure_completion(&ret),
                faulted_completion: self.async_fault_completion(&ret),
                success_completion: self.async_success_completion(&ret),
                catch_completion: self.async_failure_completion(&ret),
            })
        } else {
            let success = self.sync_callback_success(&method_name, &params, &ret);
            CSharpCallbackEntryPlan::Sync(CSharpSyncCallbackEntryPlan {
                native_params: self.sync_callback_native_params(&params, &ret),
                out_initializer: self.sync_out_initializer(&ret),
                bridge_params: params,
                success,
            })
        }
    }

    fn lower_callback_proxy(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpCallbackProxyPlan {
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        let public_params = callback_public_param_list(&params);
        if method.is_async() {
            return CSharpCallbackProxyPlan::AsyncUnsupported {
                public_params,
                return_type: ret.async_task_type(),
                result_type: if matches!(ret, CallbackReturn::Void) {
                    None
                } else {
                    Some(self.callback_public_return_type(&method.returns))
                },
                not_supported_expr:
                    "new NotSupportedException(\"async callback proxies are not implemented for C# yet\")"
                        .to_string(),
            };
        }

        let has_cleanup = params
            .iter()
            .any(CSharpCallbackBridgeParamPlan::needs_wire_writer);
        let call = self.proxy_call(&params, &ret);
        CSharpCallbackProxyPlan::Sync(CSharpSyncCallbackProxyPlan {
            public_params,
            return_type: ret.public_type(),
            bridge_params: params,
            has_cleanup,
            call,
        })
    }

    fn lower_callback_delegates(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpCallbackDelegatePlan {
        let method_name: CSharpMethodName = (&method.id).into();
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        CSharpCallbackDelegatePlan {
            entry_params: if method.is_async() {
                self.async_callback_native_params(&method_name, &params)
            } else {
                self.sync_callback_native_params(&params, &ret)
            },
            completion_params: method
                .is_async()
                .then(|| self.async_completion_params(&ret)),
            proxy_params: (!method.is_async())
                .then(|| self.sync_callback_native_params(&params, &ret)),
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
                native_expr,
                marshals_bool,
                ..
            } => {
                let native_value_expr = if *marshals_bool {
                    RETURN_VALUE.to_string()
                } else {
                    native_expr.replace("value", RETURN_VALUE)
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
                let result_assignment_lines = if *is_result {
                    unindented_lines(
                        &self.render_result_assignment(
                            "impl_",
                            "Invoke",
                            &decoded_args.to_string(),
                            encode_ops,
                        ),
                        "            ",
                    )
                } else {
                    vec![]
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
                    result_assignment_lines,
                    writer,
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
            CSharpCallbackBridgeParamPlan::WireEncoded {
                public_param,
                native_ptr_param: CSharpParameter::bare(CSharpType::IntPtr, name),
                native_len_param: CSharpParameter::bare(CSharpType::UIntPtr, len_name),
                reader_local: reader_name,
                decoded_arg: decode_expr,
                pin_local: CSharpLocalName::new(format!(
                    "_{}Pin",
                    stripped_name(&writer.param_name)
                )),
                ptr_local: CSharpLocalName::new(format!(
                    "_{}Ptr",
                    stripped_name(&writer.param_name)
                )),
                writer,
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
                    "void".to_string()
                } else {
                    self.lower_type(ok).expect("result ok type").to_string()
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

    fn callback_public_return_type(&self, returns: &ReturnDef) -> CSharpType {
        match returns {
            ReturnDef::Void => CSharpType::Void,
            ReturnDef::Value(ty) => self.lower_type(ty).expect("callback return type"),
            ReturnDef::Result { ok, .. } if matches!(ok, TypeExpr::Void) => CSharpType::Void,
            ReturnDef::Result { ok, .. } => self.lower_type(ok).expect("result ok type"),
        }
    }

    fn callback_value_return(
        &self,
        ty: &TypeExpr,
        ret_shape: &ReturnShape,
        mode: CallbackReturnMode,
    ) -> CallbackReturn {
        let public_type = self
            .lower_type(ty)
            .expect("callback return type")
            .to_string();
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
                let native_expr = self.direct_return_native_expr(ty, primitive, "value");
                let public_expr = self.direct_return_public_expr(ty, primitive, OUT_PTR);
                CallbackReturn::Direct {
                    public_type,
                    native_type,
                    native_out_type,
                    default_value: if marshals_bool {
                        "0".to_string()
                    } else {
                        "default".to_string()
                    },
                    native_expr,
                    public_expr,
                    marshals_bool,
                }
            }
            Some(Transport::Composite(layout)) if mode == CallbackReturnMode::InlineClosure => {
                let native_type = CSharpClassName::from(&layout.record_id).to_string();
                CallbackReturn::Direct {
                    public_type,
                    native_type: named_type(&native_type),
                    native_out_type: named_type(&native_type),
                    default_value: "default".to_string(),
                    native_expr: "value".to_string(),
                    public_expr: OUT_PTR.to_string(),
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
                    default_value: "BoltFFICallbackHandle.Null".to_string(),
                    native_expr: format!("{bridge}.Create(value)"),
                    public_expr: format!("{bridge}.Wrap({OUT_PTR})"),
                    marshals_bool: false,
                }
            }
            Some(Transport::Handle { .. }) => CallbackReturn::Direct {
                public_type,
                native_type: CSharpType::IntPtr,
                native_out_type: CSharpType::IntPtr,
                default_value: "IntPtr.Zero".to_string(),
                native_expr: "value.Handle".to_string(),
                public_expr: OUT_PTR.to_string(),
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

    fn public_param_plans(
        &self,
        params: &[crate::ir::definitions::ParamDef],
    ) -> Vec<CSharpCallbackParamPlan> {
        params
            .iter()
            .map(|param| CSharpCallbackParamPlan {
                csharp_type: self
                    .lower_type(&param.type_expr)
                    .expect("callback param type"),
                name: (&param.name).into(),
            })
            .collect()
    }

    fn render_result_assignment(
        &self,
        receiver: &str,
        method_name: &str,
        decoded_args: &str,
        encode_ops: &WriteSeq,
    ) -> String {
        let mut out = String::new();
        let result_ty = self.result_type_for_encode_ops(encode_ops);
        out.push_str(&format!("            {result_ty} {RESULT_VALUE};\n"));
        out.push_str("            try\n            {\n");
        if result_ty.starts_with("BoltFFIResult<BoltFFIUnit,") {
            out.push_str(&format!(
                "                {receiver}.{method_name}({decoded_args});\n"
            ));
            out.push_str(&format!(
                "                {RESULT_VALUE} = BoltFFIResult<BoltFFIUnit, "
            ));
            out.push_str(
                result_ty
                    .split_once(", ")
                    .map(|(_, tail)| tail.trim_end_matches('>'))
                    .unwrap_or("object"),
            );
            out.push_str(">.Ok(default);\n");
        } else {
            out.push_str(&format!(
                "                {RESULT_VALUE} = {result_ty}.Ok({receiver}.{method_name}({decoded_args}));\n"
            ));
        }
        out.push_str("            }\n");
        out.push_str(&self.result_expected_catches_from_encode_ops(
            encode_ops,
            RESULT_VALUE,
            "            ",
        ));
        out
    }

    fn result_type_for_encode_ops(&self, encode_ops: &WriteSeq) -> String {
        let Some(crate::ir::ops::WriteOp::Result { ok, err, .. }) = encode_ops.ops.first() else {
            return "BoltFFIResult<object, object>".to_string();
        };
        let ok_type = self
            .result_branch_type(ok)
            .unwrap_or_else(|| "BoltFFIUnit".to_string());
        let err_type = self
            .result_branch_type(err)
            .unwrap_or_else(|| "object".to_string());
        format!("BoltFFIResult<{ok_type}, {err_type}>")
    }

    fn result_branch_type(&self, seq: &WriteSeq) -> Option<String> {
        let op = seq.ops.first()?;
        match op {
            crate::ir::ops::WriteOp::Primitive { primitive, .. } => {
                Some(CSharpType::from(*primitive).to_string())
            }
            crate::ir::ops::WriteOp::String { .. } => Some("string".to_string()),
            crate::ir::ops::WriteOp::Bytes { .. } => Some("byte[]".to_string()),
            crate::ir::ops::WriteOp::Record { id, .. } => {
                Some(CSharpClassName::from(id).to_string())
            }
            crate::ir::ops::WriteOp::Enum { id, .. } => Some(CSharpClassName::from(id).to_string()),
            crate::ir::ops::WriteOp::Option { some, .. } => self
                .result_branch_type(some)
                .map(|inner| format!("{inner}?")),
            crate::ir::ops::WriteOp::Vec { element_type, .. } => {
                let inner = self.lower_type(element_type)?.to_string();
                Some(format!("{inner}[]"))
            }
            crate::ir::ops::WriteOp::Custom { underlying, .. } => {
                self.result_branch_type(underlying)
            }
            _ => None,
        }
    }

    fn result_expected_catches_from_encode_ops(
        &self,
        encode_ops: &WriteSeq,
        result_name: &str,
        indent: &str,
    ) -> String {
        let Some(crate::ir::ops::WriteOp::Result { err, .. }) = encode_ops.ops.first() else {
            return String::new();
        };
        let err_type = self
            .result_branch_type(err)
            .unwrap_or_else(|| "object".to_string());
        let mut out = String::new();
        if let Some(exception) = self.error_exception_for_write_seq(err) {
            out.push_str(&format!(
                "{indent}catch ({exception} {ASYNC_EXCEPTION})\n{indent}{{\n"
            ));
            out.push_str(&format!(
                "{indent}    {result_name} = BoltFFIResult<{}, {err_type}>.Err({ASYNC_EXCEPTION}.Error);\n",
                self.result_type_for_encode_ops(encode_ops)
                    .trim_start_matches("BoltFFIResult<")
                    .split_once(", ")
                    .map(|(ok, _)| ok)
                    .unwrap_or("object")
            ));
            out.push_str(&format!("{indent}}}\n"));
        } else if err_type == "string" {
            out.push_str(&format!(
                "{indent}catch (Exception {ASYNC_EXCEPTION})\n{indent}{{\n"
            ));
            out.push_str(&format!(
                "{indent}    {result_name} = BoltFFIResult<{}, string>.Err({ASYNC_EXCEPTION}.Message);\n",
                self.result_type_for_encode_ops(encode_ops)
                    .trim_start_matches("BoltFFIResult<")
                    .split_once(", ")
                    .map(|(ok, _)| ok)
                    .unwrap_or("object")
            ));
            out.push_str(&format!("{indent}}}\n"));
        }
        out
    }

    fn error_exception_for_write_seq(&self, seq: &WriteSeq) -> Option<String> {
        match seq.ops.first()? {
            crate::ir::ops::WriteOp::Record { id, .. }
                if self
                    .ffi
                    .catalog
                    .resolve_record(id)
                    .is_some_and(|record| record.is_error) =>
            {
                Some(format!("{}Exception", CSharpClassName::from(id)))
            }
            crate::ir::ops::WriteOp::Enum { id, .. }
                if self
                    .ffi
                    .catalog
                    .resolve_enum(id)
                    .is_some_and(|enumeration| enumeration.is_error) =>
            {
                Some(format!("{}Exception", CSharpClassName::from(id)))
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

    fn render_result_decode_from_ops(
        &self,
        decode_ops: &ReadSeq,
        reader_name: &str,
        indent: &str,
    ) -> String {
        let Some(crate::ir::ops::ReadOp::Result { ok, err, .. }) = decode_ops.ops.first() else {
            return String::new();
        };
        let reader =
            CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(reader_name)));
        let mut locals = decode::DecodeLocalCounters::default();
        let err_expr =
            decode::lower_decode_expr(err, &reader, None, &self.namespace, &mut locals).to_string();
        let ok_expr = if matches!(ok.ops.first(), None) {
            None
        } else {
            Some(
                decode::lower_decode_expr(ok, &reader, None, &self.namespace, &mut locals)
                    .to_string(),
            )
        };
        let mut out = String::new();
        out.push_str(&format!(
            "{indent}if ({reader_name}.ReadU8() != 0)\n{indent}{{\n"
        ));
        out.push_str(&format!(
            "{indent}    throw new InvalidOperationException({err_expr}.ToString());\n"
        ));
        out.push_str(&format!("{indent}}}\n"));
        if let Some(ok_expr) = ok_expr {
            out.push_str(&format!("{indent}return {ok_expr};\n"));
        }
        out
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
            CallbackReturn::Direct { native_expr, .. } => CSharpSyncCallbackSuccessPlan::Direct {
                decoded_args,
                native_value_expr: native_expr.replace("value", RETURN_VALUE),
            },
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                let result_assignment_lines = if *is_result {
                    unindented_lines(
                        &self.render_result_assignment(
                            "impl_",
                            &method_name.to_string(),
                            &decoded_args.to_string(),
                            encode_ops,
                        ),
                        "            ",
                    )
                } else {
                    vec![]
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
                    result_assignment_lines,
                    writer,
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
                public_expr,
                ..
            } => CSharpCallbackProxyCallPlan::Direct {
                args,
                native_out_type: native_out_type.clone(),
                public_expr: public_expr.clone(),
            },
            CallbackReturn::Encoded {
                decode_expr,
                decode_ops,
                is_result,
                ..
            } => {
                let result_decode_lines = if *is_result {
                    source_lines(
                        &self.render_result_decode_from_ops(
                            decode_ops
                                .as_ref()
                                .expect("result callback return decode ops"),
                            RETURN_READER,
                            "",
                        ),
                    )
                } else {
                    vec![]
                };
                CSharpCallbackProxyCallPlan::Encoded {
                    args,
                    decode_expr: decode_expr.clone(),
                    result_decode_lines,
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
            CallbackReturn::Direct { default_value, .. } => {
                CSharpSyncCallbackOutInitializerPlan::Direct {
                    default_value: default_value.clone(),
                }
            }
            CallbackReturn::Encoded { .. } => CSharpSyncCallbackOutInitializerPlan::Encoded,
        }
    }

    fn async_failure_completion(&self, ret: &CallbackReturn) -> CSharpAsyncCallbackFailurePlan {
        match ret {
            CallbackReturn::Void => CSharpAsyncCallbackFailurePlan::Void,
            CallbackReturn::Direct { default_value, .. } => {
                CSharpAsyncCallbackFailurePlan::Direct {
                    default_value: default_value.clone(),
                }
            }
            CallbackReturn::Encoded { .. } => CSharpAsyncCallbackFailurePlan::Encoded,
        }
    }

    fn async_success_completion(&self, ret: &CallbackReturn) -> CSharpAsyncCallbackSuccessPlan {
        match ret {
            CallbackReturn::Void => CSharpAsyncCallbackSuccessPlan::Void,
            CallbackReturn::Direct {
                native_expr,
                marshals_bool,
                ..
            } => {
                let native_expr = if *marshals_bool {
                    format!("{ASYNC_COMPLETED}.Result")
                } else {
                    native_expr.replace("value", &format!("{ASYNC_COMPLETED}.Result"))
                };
                CSharpAsyncCallbackSuccessPlan::Direct {
                    native_value_expr: native_expr,
                }
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
                    writer,
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
            .unwrap_or_else(|| "object".to_string());
        let (exception_type, error_value_expr, fallback) =
            if let Some(exception_ty) = self.error_exception_for_write_seq(err) {
                (
                    Some(exception_ty),
                    CSharpExpression::MemberAccess {
                        receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                            CSharpLocalName::new("__boltffiTypedException"),
                        ))),
                        name: CSharpPropertyName::from_source("error"),
                    },
                    Some(self.async_failure_completion(ret)),
                )
            } else if err_type == "string" {
                (
                    None,
                    CSharpExpression::MemberAccess {
                        receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(
                            CSharpLocalName::new(ASYNC_EXCEPTION),
                        ))),
                        name: CSharpPropertyName::from_source("message"),
                    },
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
            result_type: result_ty,
            writer,
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

    fn native_type_for_primitive(&self, primitive: PrimitiveType) -> &'static str {
        match primitive {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "sbyte",
            PrimitiveType::U8 => "byte",
            PrimitiveType::I16 => "short",
            PrimitiveType::U16 => "ushort",
            PrimitiveType::I32 => "int",
            PrimitiveType::U32 => "uint",
            PrimitiveType::I64 => "long",
            PrimitiveType::U64 => "ulong",
            PrimitiveType::ISize => "nint",
            PrimitiveType::USize => "nuint",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
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

    fn direct_return_native_expr(
        &self,
        type_expr: &TypeExpr,
        primitive: PrimitiveType,
        value_name: &str,
    ) -> String {
        if primitive == PrimitiveType::Bool {
            format!("{value_name} ? (byte)1 : (byte)0")
        } else if self.is_c_style_enum_type(type_expr) {
            format!(
                "({}){value_name}",
                self.native_type_for_primitive(primitive)
            )
        } else {
            value_name.to_string()
        }
    }

    fn direct_return_public_expr(
        &self,
        type_expr: &TypeExpr,
        primitive: PrimitiveType,
        value_name: &str,
    ) -> String {
        if primitive == PrimitiveType::Bool {
            format!("{value_name} != 0")
        } else if self.is_c_style_enum_type(type_expr) {
            let ty = self.lower_type(type_expr).expect("enum type");
            format!("({ty}){value_name}")
        } else {
            value_name.to_string()
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

fn source_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn unindented_lines(text: &str, prefix: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.strip_prefix(prefix).unwrap_or(line).to_string())
        .collect()
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
