use crate::ir::abi::{AbiCallbackInvocation, AbiCallbackMethod, AbiParam, ParamRole, ReturnShape};
use crate::ir::definitions::{CallbackKind, CallbackMethodDef, CallbackTraitDef, ReturnDef};
use crate::ir::ids::CallbackId;
use crate::ir::ops::{ReadSeq, WriteSeq};
use crate::ir::plan::{AbiType, Transport};
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::super::ast::{
    CSharpArgumentList, CSharpAttribute, CSharpAttributeArg, CSharpClassName, CSharpExpression,
    CSharpIdentity, CSharpLocalName, CSharpMethodName, CSharpParamName, CSharpParameter,
    CSharpPropertyName, CSharpType, CSharpTypeReference,
};
use super::super::plan::{
    CFunctionName, CSharpCallbackBridgeParamPlan, CSharpCallbackMethodPlan,
    CSharpCallbackParamPlan, CSharpCallbackPlan, CSharpClosureMethodPlan, CSharpClosurePlan,
    CSharpWireWriterPlan,
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
const ASYNC_TASK: &str = "__boltffiTask";
const ASYNC_COMPLETED: &str = "__boltffiCompleted";
const ASYNC_EXCEPTION: &str = "__boltffiException";
const ERROR_OUT_PTR: &str = "__boltffiErrorOutPtr";
const ERROR_OUT_LEN: &str = "__boltffiErrorOutLen";

#[derive(Debug, Clone)]
enum CallbackReturn {
    Void,
    Direct {
        public_type: String,
        native_type: String,
        native_out_type: String,
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
                self.lower_callback_method(callback, method, abi_method)
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
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> CSharpCallbackMethodPlan {
        let entry_source = if method.is_async() {
            self.render_async_callback_entry(callback, method, abi_method)
        } else {
            self.render_sync_callback_entry(callback, method, abi_method)
        };

        CSharpCallbackMethodPlan {
            name: (&method.id).into(),
            vtable_field: CSharpLocalName::new(abi_method.vtable_field.as_str()),
            return_type: self.callback_public_return_type(&method.returns),
            is_async: method.is_async(),
            public_params: self.public_param_plans(&method.params),
            entry_source,
            proxy_source: self.render_proxy_method(method, abi_method),
            delegate_source: self.render_callback_delegate_types(method, abi_method),
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
        let native_decls = std::iter::once("IntPtr context".to_string())
            .chain(
                params
                    .iter()
                    .flat_map(|p| p.native_params().into_iter().map(|param| param.to_string())),
            )
            .collect::<Vec<_>>();
        let decoded_args = decoded_arg_list(&params).to_string();
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
            native_decls,
            marshals_return_bool,
            decode_setup: params
                .iter()
                .flat_map(CSharpCallbackBridgeParamPlan::decode_setup_lines)
                .collect(),
            invoke_body: self.render_closure_invoke_return(&ret, &decoded_args),
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

    fn render_closure_invoke_return(&self, ret: &CallbackReturn, decoded_args: &str) -> String {
        match ret {
            CallbackReturn::Void => {
                format!("            impl_({decoded_args});\n")
            }
            CallbackReturn::Direct {
                native_expr,
                marshals_bool,
                ..
            } => {
                let return_expr = if *marshals_bool {
                    RETURN_VALUE.to_string()
                } else {
                    native_expr.replace("value", RETURN_VALUE)
                };
                format!(
                    "            var {RETURN_VALUE} = impl_({decoded_args});\n            return {return_expr};\n"
                )
            }
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                let mut out = String::new();
                if *is_result {
                    out.push_str(&self.render_result_assignment(
                        "impl_",
                        "Invoke",
                        decoded_args,
                        encode_ops,
                    ));
                } else {
                    out.push_str(&format!(
                        "            var {RETURN_VALUE} = impl_({decoded_args});\n"
                    ));
                }
                out.push_str(&indent_block(
                    &self.render_encode_to_bytes_with_renames(
                        encode_ops,
                        "_returnWire",
                        "_returnBytes",
                        "            ",
                        &root_local_rename(
                            if *is_result { "result" } else { "value" },
                            if *is_result {
                                RESULT_VALUE
                            } else {
                                RETURN_VALUE
                            },
                        ),
                    ),
                    "",
                ));
                out.push_str("            return FfiBuf.FromBytes(_returnBytes);\n");
                out
            }
        }
    }

    fn render_sync_callback_entry(
        &self,
        _callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> String {
        let method_name: CSharpMethodName = (&method.id).into();
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        let native_decls = self.sync_callback_native_decls(&params, &ret);
        let decoded_args = decoded_arg_list(&params).to_string();
        let mut out =
            format!("        private static void {method_name}({native_decls})\n        {{\n");
        for line in self.sync_out_initializers(&ret) {
            out.push_str(&format!("            {line}\n"));
        }
        out.push_str(&format!(
            "            {STATUS_OUT} = {STATUS_INTERNAL_ERROR};\n"
        ));
        out.push_str("            try\n            {\n");
        out.push_str("                if (!Handles.TryGetValue(handle, out var impl_)) throw new InvalidOperationException(\"invalid callback handle\");\n");
        for param in &params {
            for line in param.decode_setup_lines() {
                out.push_str(&format!("                {line}\n"));
            }
        }
        match &ret {
            CallbackReturn::Void => {
                out.push_str(&format!(
                    "                impl_.{method_name}({decoded_args});\n"
                ));
                out.push_str(&format!("                {STATUS_OUT} = {STATUS_OK};\n"));
            }
            CallbackReturn::Direct { native_expr, .. } => {
                out.push_str(&format!(
                    "                var {RETURN_VALUE} = impl_.{method_name}({decoded_args});\n"
                ));
                out.push_str(&format!(
                    "                {OUT_PTR} = {};\n",
                    native_expr.replace("value", RETURN_VALUE)
                ));
                out.push_str(&format!("                {STATUS_OUT} = {STATUS_OK};\n"));
            }
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                if *is_result {
                    out.push_str(&indent_block(
                        &self.render_result_assignment(
                            "impl_",
                            &method_name.to_string(),
                            &decoded_args,
                            encode_ops,
                        ),
                        "                ",
                    ));
                } else {
                    out.push_str(&format!(
                        "                var {RETURN_VALUE} = impl_.{method_name}({decoded_args});\n"
                    ));
                }
                out.push_str(&indent_block(
                    &self.render_encode_to_bytes_with_renames(
                        encode_ops,
                        "_returnWire",
                        "_returnBytes",
                        "                ",
                        &root_local_rename(
                            if *is_result { "result" } else { "value" },
                            if *is_result {
                                RESULT_VALUE
                            } else {
                                RETURN_VALUE
                            },
                        ),
                    ),
                    "",
                ));
                out.push_str(&format!(
                    "                BoltFFICallbackReturn.Store(_returnBytes, out {OUT_PTR}, out {OUT_LEN});\n"
                ));
                out.push_str(&format!("                {STATUS_OUT} = {STATUS_OK};\n"));
            }
        }
        out.push_str("            }\n");
        out.push_str("            catch\n            {\n");
        for line in self.sync_out_initializers(&ret) {
            out.push_str(&format!("                {line}\n"));
        }
        out.push_str(&format!(
            "                {STATUS_OUT} = {STATUS_INTERNAL_ERROR};\n"
        ));
        out.push_str("            }\n");
        out.push_str("        }\n");
        out
    }

    fn render_async_callback_entry(
        &self,
        _callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> String {
        let method_name: CSharpMethodName = (&method.id).into();
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        let mut native_decls = vec!["ulong handle".to_string()];
        native_decls.extend(
            params
                .iter()
                .flat_map(|p| p.native_params().into_iter().map(|param| param.to_string())),
        );
        native_decls.push(format!("{method_name}Completion callback"));
        native_decls.push("ulong callbackData".to_string());
        let decoded_args = decoded_arg_list(&params).to_string();
        let mut out = format!(
            "        private static void {method_name}({})\n        {{\n",
            native_decls.join(", ")
        );
        out.push_str(
            "            if (!Handles.TryGetValue(handle, out var impl_))\n            {\n",
        );
        out.push_str(&self.async_complete_failure(
            &ret,
            "callback",
            "callbackData",
            "                ",
        ));
        out.push_str("                return;\n            }\n");
        out.push_str("            try\n            {\n");
        for param in &params {
            for line in param.decode_setup_lines() {
                out.push_str(&format!("                {line}\n"));
            }
        }
        out.push_str(&format!(
            "                var {ASYNC_TASK} = impl_.{method_name}({decoded_args});\n"
        ));
        out.push_str(&format!(
            "                _ = {ASYNC_TASK}.ContinueWith({ASYNC_COMPLETED} =>\n                {{\n"
        ));
        out.push_str(&format!(
            "                    if ({ASYNC_COMPLETED}.IsCanceled)\n                    {{\n"
        ));
        out.push_str(&self.async_complete_failure(
            &ret,
            "callback",
            "callbackData",
            "                        ",
        ));
        out.push_str("                        return;\n                    }\n");
        out.push_str(&format!(
            "                    if ({ASYNC_COMPLETED}.IsFaulted)\n                    {{\n"
        ));
        out.push_str(&format!(
            "                        Exception {ASYNC_EXCEPTION} = UnwrapTaskException({ASYNC_COMPLETED}.Exception!);\n"
        ));
        out.push_str(&self.async_complete_exception(
            &ret,
            "callback",
            "callbackData",
            "                        ",
        ));
        out.push_str("                        return;\n                    }\n");
        out.push_str(&self.async_complete_success(
            &ret,
            "callback",
            "callbackData",
            ASYNC_COMPLETED,
            "                    ",
        ));
        out.push_str("                }, global::System.Threading.Tasks.TaskScheduler.Default);\n");
        out.push_str("            }\n");
        out.push_str("            catch\n            {\n");
        out.push_str(&self.async_complete_failure(
            &ret,
            "callback",
            "callbackData",
            "                ",
        ));
        out.push_str("            }\n");
        out.push_str("        }\n");
        out
    }

    fn render_proxy_method(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> String {
        let method_name: CSharpMethodName = (&method.id).into();
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        let public_params = params
            .iter()
            .map(|p| p.public_param().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if method.is_async() {
            let return_type = ret.async_task_type();
            let not_supported = "new NotSupportedException(\"async callback proxies are not implemented for C# yet\")";
            return match ret {
                CallbackReturn::Void => format!(
                    "            public {return_type} {method_name}({public_params}) => global::System.Threading.Tasks.Task.FromException({not_supported});\n"
                ),
                _ => format!(
                    "            public {return_type} {method_name}({public_params}) => global::System.Threading.Tasks.Task.FromException<{}>({not_supported});\n",
                    ret.public_type()
                ),
            };
        }

        let mut out = format!(
            "            public {} {method_name}({public_params})\n            {{\n",
            ret.public_type()
        );
        out.push_str("                if (_handle.IsNull) throw new ObjectDisposedException(nameof(Proxy));\n");
        out.push_str(
            "                VTable __boltffiTable = Marshal.PtrToStructure<VTable>(_handle.vtable);\n",
        );
        out.push_str(&format!(
            "                var {INVOKE_LOCAL} = Marshal.GetDelegateForFunctionPointer<{method_name}ProxyFn>(__boltffiTable.{});\n",
            abi_method.vtable_field.as_str()
        ));
        for param in &params {
            for line in param.proxy_setup_lines() {
                out.push_str(&format!("                {line}\n"));
            }
        }
        let has_cleanup = params
            .iter()
            .any(|param| !param.proxy_cleanup_lines().is_empty());
        if has_cleanup {
            out.push_str("                try\n                {\n");
        }
        for param in &params {
            for line in param.proxy_pin_lines() {
                out.push_str(&format!(
                    "{}{line}\n",
                    if has_cleanup {
                        "                    "
                    } else {
                        "                "
                    }
                ));
            }
        }
        let call_indent = if has_cleanup {
            "                    "
        } else {
            "                "
        };
        let args = std::iter::once("_handle.handle".to_string())
            .chain(
                params
                    .iter()
                    .flat_map(|p| p.proxy_args().into_iter().map(|expr| expr.to_string())),
            )
            .collect::<Vec<_>>()
            .join(", ");
        match &ret {
            CallbackReturn::Void => {
                out.push_str(&format!("{call_indent}FfiStatus {STATUS_OUT};\n"));
                out.push_str(&format!(
                    "{call_indent}{INVOKE_LOCAL}({args}, out {STATUS_OUT});\n"
                ));
                out.push_str(&format!(
                    "{call_indent}ThrowIfCallbackStatus({STATUS_OUT});\n"
                ));
            }
            CallbackReturn::Direct {
                native_out_type,
                public_expr,
                ..
            } => {
                out.push_str(&format!("{call_indent}{native_out_type} {OUT_PTR};\n"));
                out.push_str(&format!("{call_indent}FfiStatus {STATUS_OUT};\n"));
                out.push_str(&format!(
                    "{call_indent}{INVOKE_LOCAL}({args}, out {OUT_PTR}, out {STATUS_OUT});\n"
                ));
                out.push_str(&format!(
                    "{call_indent}ThrowIfCallbackStatus({STATUS_OUT});\n"
                ));
                out.push_str(&format!("{call_indent}return {public_expr};\n"));
            }
            CallbackReturn::Encoded {
                decode_expr,
                decode_ops,
                is_result,
                ..
            } => {
                out.push_str(&format!("{call_indent}IntPtr {OUT_PTR};\n"));
                out.push_str(&format!("{call_indent}UIntPtr {OUT_LEN};\n"));
                out.push_str(&format!("{call_indent}FfiStatus {STATUS_OUT};\n"));
                out.push_str(&format!(
                    "{call_indent}{INVOKE_LOCAL}({args}, out {OUT_PTR}, out {OUT_LEN}, out {STATUS_OUT});\n"
                ));
                out.push_str(&format!("{call_indent}try\n{call_indent}{{\n"));
                out.push_str(&format!(
                    "{call_indent}    ThrowIfCallbackStatus({STATUS_OUT});\n"
                ));
                out.push_str(&format!(
                    "{call_indent}    var {RETURN_READER} = new WireReader({OUT_PTR}, {OUT_LEN});\n"
                ));
                if *is_result {
                    let result_body = self.render_result_decode_from_ops(
                        decode_ops
                            .as_ref()
                            .expect("result callback return decode ops"),
                        RETURN_READER,
                        &(call_indent.to_string() + "    "),
                    );
                    out.push_str(&result_body);
                } else if let Some(expr) = decode_expr {
                    out.push_str(&format!("{call_indent}    return {expr};\n"));
                }
                out.push_str(&format!("{call_indent}}}\n"));
                out.push_str(&format!("{call_indent}finally\n{call_indent}{{\n"));
                out.push_str(&format!(
                    "{call_indent}    BoltFFICallbackReturn.Free({OUT_PTR});\n"
                ));
                out.push_str(&format!("{call_indent}}}\n"));
            }
        }
        if has_cleanup {
            out.push_str("                }\n                finally\n                {\n");
            for param in &params {
                for line in param.proxy_cleanup_lines() {
                    out.push_str(&format!("                    {line}\n"));
                }
            }
            out.push_str("                }\n");
        }
        out.push_str("            }\n");
        out
    }

    fn render_callback_delegate_types(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> String {
        let method_name: CSharpMethodName = (&method.id).into();
        let params = self.bridge_params(method, abi_method);
        let ret = self.callback_return(
            &method.returns,
            &abi_method.returns,
            CallbackReturnMode::CallbackVtable,
        );
        let mut out = String::new();
        out.push_str("        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]\n");
        out.push_str(&format!(
            "        private delegate void {method_name}Fn({});\n",
            if method.is_async() {
                let mut decls = vec!["ulong handle".to_string()];
                decls.extend(
                    params
                        .iter()
                        .flat_map(|p| p.native_params().into_iter().map(|param| param.to_string())),
                );
                decls.push(format!("{method_name}Completion callback"));
                decls.push("ulong callbackData".to_string());
                decls.join(", ")
            } else {
                self.sync_callback_native_decls(&params, &ret)
            }
        ));
        out.push_str(&format!(
            "        private static readonly {method_name}Fn {method_name}Delegate = {method_name};\n\n"
        ));
        if method.is_async() {
            out.push_str("        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]\n");
            out.push_str(&format!(
                "        private delegate void {method_name}Completion({});\n\n",
                self.async_completion_decls(&ret)
            ));
        } else {
            out.push_str("        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]\n");
            out.push_str(&format!(
                "        private delegate void {method_name}ProxyFn({});\n\n",
                self.sync_proxy_native_decls(&params, &ret)
            ));
        }
        out
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
                let native_type = self.native_type_for_primitive(primitive).to_string();
                let marshals_bool = primitive == PrimitiveType::Bool;
                let native_out_type = if marshals_bool {
                    "byte".to_string()
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
                    native_type: native_type.clone(),
                    native_out_type: native_type,
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
                    native_type: "BoltFFICallbackHandle".to_string(),
                    native_out_type: "BoltFFICallbackHandle".to_string(),
                    default_value: "BoltFFICallbackHandle.Null".to_string(),
                    native_expr: format!("{bridge}.Create(value)"),
                    public_expr: format!("{bridge}.Wrap({OUT_PTR})"),
                    marshals_bool: false,
                }
            }
            Some(Transport::Handle { .. }) => CallbackReturn::Direct {
                public_type,
                native_type: "IntPtr".to_string(),
                native_out_type: "IntPtr".to_string(),
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
        let writer_name = CSharpLocalName::new(format!("_{}Wire", stripped_name(name)));
        let bytes_name = CSharpLocalName::new(format!("_{}Bytes", stripped_name(name)));
        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        let writer = CSharpExpression::Identity(CSharpIdentity::Local(writer_name.clone()));
        CSharpWireWriterPlan {
            binding_name: writer_name,
            bytes_binding_name: bytes_name,
            param_name: name.clone(),
            size_expr: size::lower_size_expr(
                &encode_ops.size,
                &value::Renames::new(),
                &mut size_locals,
            ),
            encode_stmts: encode::lower_encode_expr(
                encode_ops,
                &writer,
                &value::Renames::new(),
                &mut encode_locals,
            ),
        }
    }

    fn render_encode_to_bytes_with_renames(
        &self,
        encode_ops: &WriteSeq,
        writer_name: &str,
        bytes_name: &str,
        indent: &str,
        renames: &value::Renames,
    ) -> String {
        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        let writer =
            CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(writer_name)));
        let size_expr = size::lower_size_expr(&encode_ops.size, renames, &mut size_locals);
        let stmts = encode::lower_encode_expr(encode_ops, &writer, renames, &mut encode_locals);
        let mut out = String::new();
        out.push_str(&format!("{indent}byte[] {bytes_name};\n"));
        out.push_str(&format!(
            "{indent}using (var {writer_name} = new WireWriter({size_expr}))\n"
        ));
        out.push_str(&format!("{indent}{{\n"));
        for stmt in stmts {
            out.push_str(&format!("{indent}    {stmt};\n"));
        }
        out.push_str(&format!(
            "{indent}    {bytes_name} = {writer_name}.ToArray();\n"
        ));
        out.push_str(&format!("{indent}}}\n"));
        out
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

    fn sync_callback_native_decls(
        &self,
        params: &[CSharpCallbackBridgeParamPlan],
        ret: &CallbackReturn,
    ) -> String {
        let mut decls = vec!["ulong handle".to_string()];
        decls.extend(
            params
                .iter()
                .flat_map(|p| p.native_params().into_iter().map(|param| param.to_string())),
        );
        match ret {
            CallbackReturn::Void => {}
            CallbackReturn::Direct {
                native_out_type, ..
            } => {
                decls.push(format!("out {native_out_type} {OUT_PTR}"));
            }
            CallbackReturn::Encoded { .. } => {
                decls.push(format!("out IntPtr {OUT_PTR}"));
                decls.push(format!("out UIntPtr {OUT_LEN}"));
            }
        }
        decls.push(format!("out FfiStatus {STATUS_OUT}"));
        decls.join(", ")
    }

    fn sync_proxy_native_decls(
        &self,
        params: &[CSharpCallbackBridgeParamPlan],
        ret: &CallbackReturn,
    ) -> String {
        self.sync_callback_native_decls(params, ret)
    }

    fn sync_out_initializers(&self, ret: &CallbackReturn) -> Vec<String> {
        match ret {
            CallbackReturn::Void => vec![],
            CallbackReturn::Direct { default_value, .. } => {
                vec![format!("{OUT_PTR} = {default_value};")]
            }
            CallbackReturn::Encoded { .. } => vec![
                format!("{OUT_PTR} = IntPtr.Zero;"),
                format!("{OUT_LEN} = UIntPtr.Zero;"),
            ],
        }
    }

    fn async_completion_decls(&self, ret: &CallbackReturn) -> String {
        let mut decls = vec!["ulong callbackData".to_string()];
        match ret {
            CallbackReturn::Void => {}
            CallbackReturn::Direct {
                native_type,
                marshals_bool,
                ..
            } => {
                if *marshals_bool {
                    decls.push("[MarshalAs(UnmanagedType.I1)] bool value".to_string());
                } else {
                    decls.push(format!("{native_type} value"));
                }
            }
            CallbackReturn::Encoded { .. } => {
                decls.push("IntPtr valuePtr".to_string());
                decls.push("UIntPtr valueLen".to_string());
            }
        }
        decls.push(format!("FfiStatus {STATUS_OUT}"));
        decls.join(", ")
    }

    fn async_complete_failure(
        &self,
        ret: &CallbackReturn,
        callback_name: &str,
        callback_data: &str,
        indent: &str,
    ) -> String {
        match ret {
            CallbackReturn::Void => {
                format!("{indent}{callback_name}({callback_data}, {STATUS_INTERNAL_ERROR});\n")
            }
            CallbackReturn::Direct { default_value, .. } => format!(
                "{indent}{callback_name}({callback_data}, {default_value}, {STATUS_INTERNAL_ERROR});\n"
            ),
            CallbackReturn::Encoded { .. } => format!(
                "{indent}{callback_name}({callback_data}, IntPtr.Zero, UIntPtr.Zero, {STATUS_INTERNAL_ERROR});\n"
            ),
        }
    }

    fn async_complete_success(
        &self,
        ret: &CallbackReturn,
        callback_name: &str,
        callback_data: &str,
        task_name: &str,
        indent: &str,
    ) -> String {
        match ret {
            CallbackReturn::Void => {
                format!("{indent}{callback_name}({callback_data}, {STATUS_OK});\n")
            }
            CallbackReturn::Direct {
                native_expr,
                marshals_bool,
                ..
            } => {
                let native_expr = if *marshals_bool {
                    format!("{task_name}.Result")
                } else {
                    native_expr.replace("value", &format!("{task_name}.Result"))
                };
                format!("{indent}{callback_name}({callback_data}, {native_expr}, {STATUS_OK});\n")
            }
            CallbackReturn::Encoded {
                encode_ops,
                is_result,
                ..
            } => {
                let mut out = String::new();
                if *is_result {
                    out.push_str(&format!(
                        "{indent}var {RESULT_VALUE} = {}.Ok({task_name}.Result);\n",
                        self.result_type_for_encode_ops(encode_ops)
                    ));
                } else {
                    out.push_str(&format!(
                        "{indent}var {RETURN_VALUE} = {task_name}.Result;\n"
                    ));
                }
                out.push_str(&self.render_encode_to_bytes_with_renames(
                    encode_ops,
                    "_returnWire",
                    "_returnBytes",
                    indent,
                    &root_local_rename(
                        if *is_result { "result" } else { "value" },
                        if *is_result {
                            RESULT_VALUE
                        } else {
                            RETURN_VALUE
                        },
                    ),
                ));
                out.push_str(&format!(
                    "{indent}BoltFFICallbackReturn.Store(_returnBytes, out var {OUT_PTR}, out var {OUT_LEN});\n"
                ));
                out.push_str(&format!("{indent}try\n{indent}{{\n"));
                out.push_str(&format!(
                    "{indent}    {callback_name}({callback_data}, {OUT_PTR}, {OUT_LEN}, {STATUS_OK});\n"
                ));
                out.push_str(&format!("{indent}}}\n{indent}finally\n{indent}{{\n"));
                out.push_str(&format!(
                    "{indent}    BoltFFICallbackReturn.Free({OUT_PTR});\n"
                ));
                out.push_str(&format!("{indent}}}\n"));
                out
            }
        }
    }

    fn async_complete_exception(
        &self,
        ret: &CallbackReturn,
        callback_name: &str,
        callback_data: &str,
        indent: &str,
    ) -> String {
        let CallbackReturn::Encoded {
            encode_ops,
            is_result: true,
            ..
        } = ret
        else {
            return self.async_complete_failure(ret, callback_name, callback_data, indent);
        };
        let mut out = String::new();
        let result_ty = self.result_type_for_encode_ops(encode_ops);
        let Some(crate::ir::ops::WriteOp::Result { err, .. }) = encode_ops.ops.first() else {
            return self.async_complete_failure(ret, callback_name, callback_data, indent);
        };
        let err_type = self
            .result_branch_type(err)
            .unwrap_or_else(|| "object".to_string());
        if let Some(exception_ty) = self.error_exception_for_write_seq(err) {
            out.push_str(&format!(
                "{indent}if ({ASYNC_EXCEPTION} is {exception_ty} __boltffiTypedException)\n{indent}{{\n"
            ));
            out.push_str(&format!(
                "{indent}    var {ERROR_RESULT_VALUE} = {result_ty}.Err(__boltffiTypedException.Error);\n"
            ));
        } else if err_type == "string" {
            out.push_str(&format!("{indent}{{\n"));
            out.push_str(&format!(
                "{indent}    var {ERROR_RESULT_VALUE} = {result_ty}.Err({ASYNC_EXCEPTION}.Message);\n"
            ));
        } else {
            return self.async_complete_failure(ret, callback_name, callback_data, indent);
        }
        out.push_str(&self.render_encode_to_bytes_with_renames(
            encode_ops,
            "_returnErrorWire",
            "_returnErrorBytes",
            &(indent.to_string() + "    "),
            &root_local_rename("result", ERROR_RESULT_VALUE),
        ));
        out.push_str(&format!(
            "{indent}    BoltFFICallbackReturn.Store(_returnErrorBytes, out var {ERROR_OUT_PTR}, out var {ERROR_OUT_LEN});\n"
        ));
        out.push_str(&format!("{indent}    try\n{indent}    {{\n"));
        out.push_str(&format!(
            "{indent}        {callback_name}({callback_data}, {ERROR_OUT_PTR}, {ERROR_OUT_LEN}, {STATUS_OK});\n"
        ));
        out.push_str(&format!(
            "{indent}    }}\n{indent}    finally\n{indent}    {{\n"
        ));
        out.push_str(&format!(
            "{indent}        BoltFFICallbackReturn.Free({ERROR_OUT_PTR});\n"
        ));
        out.push_str(&format!("{indent}    }}\n"));
        out.push_str(&format!("{indent}    return;\n"));
        out.push_str(&format!("{indent}}}\n"));
        if self.error_exception_for_write_seq(err).is_some() {
            out.push_str(&self.async_complete_failure(ret, callback_name, callback_data, indent));
        }
        out
    }

    fn inline_callback_native_return_type(&self, ret: &CallbackReturn) -> String {
        match ret {
            CallbackReturn::Void => "void".to_string(),
            CallbackReturn::Direct { native_type, .. } => native_type.clone(),
            CallbackReturn::Encoded { .. } => "FfiBuf".to_string(),
        }
    }

    fn native_param(&self, abi_type: &AbiType, name: &CSharpParamName) -> CSharpParameter {
        match abi_type {
            AbiType::Bool => CSharpParameter {
                attributes: vec![marshal_as_i1()],
                csharp_type: CSharpType::Bool,
                name: name.clone(),
            },
            _ => {
                CSharpParameter::bare(self.native_csharp_type_for_abi_type(abi_type), name.clone())
            }
        }
    }

    fn native_type_for_abi_type(&self, abi_type: &AbiType) -> String {
        self.native_csharp_type_for_abi_type(abi_type).to_string()
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

fn indent_block(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}\n"))
        .collect::<String>()
}

fn decoded_arg_list(params: &[CSharpCallbackBridgeParamPlan]) -> CSharpArgumentList {
    params
        .iter()
        .map(|param| param.decoded_arg().clone())
        .collect::<Vec<_>>()
        .into()
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
