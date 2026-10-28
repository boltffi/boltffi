use super::*;

pub struct Callback {
    source: String,
}

impl Callback {
    pub fn from_declaration(
        decl: &CallbackDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let mut interface_methods = Vec::new();
        let mut adapter_methods = Vec::new();
        let mut wrapper_methods = Vec::new();
        let mut dispatch_methods = Vec::new();

        for method in decl.protocol().methods() {
            let method_name = Name::new(method.name()).dart_identifier();
            let js_name = Name::new(method.name()).js_member_name();
            let signature = callback_method_signature(method.callable(), context)?;
            let dart_params = signature.dart_params_decl();
            let public_return = signature.dart_return_signature();

            interface_methods.push(format!("  {public_return} {method_name}({dart_params});"));

            let adapter_js_params = (0..signature.params.len())
                .map(|i| format!("JSAny? arg{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let decoded_args = signature
                .params
                .iter()
                .enumerate()
                .map(|(i, param)| {
                    let ty = param
                        .value_ty
                        .as_ref()
                        .ok_or_else(|| unsupported("closure parameter on a callback method"))?;
                    interop::from_js(&format!("arg{i}"), ty, context)
                })
                .collect::<Result<Vec<_>>>()?;
            let decoded_args = signature.dart_arguments(&decoded_args);
            let impl_call = format!("_impl.{method_name}({decoded_args})");
            let await_kw = if signature.asynchronous { "await " } else { "" };
            let inner_body = match &signature.error_ty {
                None => {
                    if signature.return_ty.is_none() {
                        format!("{{ {await_kw}{impl_call}; }}")
                    } else {
                        let encoded = signature.encode_return("__boltffiResult", context)?;
                        format!(
                            "{{ final __boltffiResult = {await_kw}{impl_call}; return {encoded}; }}"
                        )
                    }
                }
                Some(error_ty) => {
                    let exception_type = error_exception_type(error_ty, context)?;
                    let success = if signature.return_ty.is_none() {
                        format!("{await_kw}{impl_call};\n      return boltffiWireOk(null);")
                    } else {
                        let encoded = signature.encode_return("__boltffiResult", context)?;
                        format!(
                            "final __boltffiResult = {await_kw}{impl_call};\n      return boltffiWireOk({encoded});"
                        )
                    };
                    let caught_value = error_caught_value(error_ty, "__boltffiError");
                    let encoded_error = interop::to_js(&caught_value, error_ty, context)?;
                    format!(
                        "{{\n    try {{\n      {success}\n    }} on {exception_type} catch (__boltffiError) {{\n      return boltffiWireErr({encoded_error});\n    }}\n  }}"
                    )
                }
            };
            let adapter_method = if signature.asynchronous {
                format!(
                    "  @JSExport('{js_name}')\n  JSPromise<JSAny?> {method_name}({adapter_js_params}) {{\n    return (() async {inner_body})().toJS;\n  }}"
                )
            } else {
                format!(
                    "  @JSExport('{js_name}')\n  JSAny? {method_name}({adapter_js_params}) {inner_body}"
                )
            };
            adapter_methods.push(adapter_method);

            let js_arguments = signature.js_call_arguments();
            let dispatch_params = (0..signature.params.len())
                .map(|i| format!("JSAny? arg{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            dispatch_methods.push(format!(
                "  @JS('{js_name}')\n  external JSAny? _call_{js_name}({dispatch_params});"
            ));
            let raw_call = format!("_{name}JSMethods(js)._call_{js_name}({js_arguments})");
            let (wrapper_async, raw_result) = if signature.asynchronous {
                (
                    "async ",
                    format!("(await ({raw_call} as JSPromise<JSAny?>).toDart)"),
                )
            } else {
                ("", raw_call)
            };
            let wrapper_body = match &signature.error_ty {
                None => {
                    if signature.return_ty.is_none() {
                        format!("{{ {raw_result}; }}")
                    } else if signature.asynchronous {
                        // Same await-in-sync-IIFE hazard as the outbound
                        // call sites: decode a hoisted local instead.
                        let decoded = signature.decode_return("__boltffiAwaited", context)?;
                        format!(
                            "{{ final __boltffiAwaited = {raw_result};\n    return {decoded}; }}"
                        )
                    } else {
                        let decoded = signature.decode_return(&raw_result, context)?;
                        format!("{{ return {decoded}; }}")
                    }
                }
                Some(error_ty) => {
                    let error_value = interop::from_js(
                        "(__boltffiRaw as JSObject).getProperty('error'.toJS)",
                        error_ty,
                        context,
                    )?;
                    let throw_expr = error_throw_expression(error_ty, &error_value)?;
                    let value_decode = if signature.return_ty.is_none() {
                        String::new()
                    } else {
                        let decoded = signature.decode_return("__boltffiValue", context)?;
                        format!("return {decoded};\n    ")
                    };
                    format!(
                        "{{\n    final __boltffiRaw = {raw_result};\n    final __boltffiTag = (__boltffiRaw as JSObject).getProperty('tag'.toJS);\n    if (__boltffiTag != null && (__boltffiTag as JSString).toDart == 'err') {{\n      throw {throw_expr};\n    }}\n    final __boltffiValue = __boltffiTag != null && (__boltffiTag as JSString).toDart == 'ok'\n        ? (__boltffiRaw as JSObject).getProperty('value'.toJS)\n        : __boltffiRaw;\n    {value_decode}}}"
                    )
                }
            };
            let wrapper_body = if signature.params.iter().any(|p| p.setup.is_some()) {
                format!(
                    "{{ {} }}",
                    signature.call_body(wrapper_body.trim_start_matches('{').trim_end_matches('}'))
                )
            } else {
                wrapper_body
            };
            wrapper_methods.push(format!(
                "  @override\n  {public_return} {method_name}({dart_params}) {wrapper_async}{wrapper_body}"
            ));
        }

        let source = format!(
            "extension type _{name}JSMethods(JSObject _) implements JSObject {{\n{dispatch}\n}}\n\nabstract interface class {name} {{\n{interface}\n}}\n\n\
             @JSExport()\nclass _{name}JSAdapter {{\n  final {name} _impl;\n  _{name}JSAdapter(this._impl);\n\n{adapter}\n}}\n\n\
             final class {name}JsWrapper implements {name} {{\n  final JSObject js;\n  const {name}JsWrapper(this.js);\n\n{wrapper}\n}}\n\n\
             final _{name}JSAdapters = Expando<JSObject>();\nJSObject boltffiCallbackToJS{name}({name} callback) {{\n  if (callback is {name}JsWrapper) return callback.js;\n  return _{name}JSAdapters[callback] ??= createJSInteropWrapper(_{name}JSAdapter(callback));\n}}\n\n",
            dispatch = dispatch_methods.join("\n"),
            interface = interface_methods.join("\n"),
            adapter = adapter_methods.join("\n\n"),
            wrapper = wrapper_methods.join("\n\n"),
        );

        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}
