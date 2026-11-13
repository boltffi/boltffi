use super::*;

pub struct Function {
    source: String,
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
        namespace: &str,
    ) -> Result<Self> {
        let js_name = Name::new(decl.name()).js_export_name();
        let dart_name = Name::new(decl.name()).dart_identifier();
        let signature = call_signature(decl.callable(), context)?;
        Ok(Self {
            source: render_free_function(&js_name, &dart_name, &signature, context, namespace)?,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

// `namespace` must match `DartWebHost::js_namespace` exactly, or every
// `@JS()` extern in the file binds to nothing.
fn render_free_function(
    js_name: &str,
    dart_name: &str,
    signature: &CallSignature,
    context: &RenderContext<Wasm32>,
    namespace: &str,
) -> Result<String> {
    let params = signature.dart_params_decl_with_cancellation();
    let arguments = signature.js_call_arguments_with_cancellation();
    let extern_name = format!("_boltffiExtern_{js_name}");

    let js_return_type = if signature.asynchronous {
        "JSPromise<JSAny?>".to_owned()
    } else {
        "JSAny?".to_owned()
    };
    let extern_params = (0..signature.params.len())
        .map(|i| format!("JSAny? arg{i}"))
        .chain(
            signature
                .asynchronous
                .then(|| ["JSAny? options".to_owned()])
                .into_iter()
                .flatten(),
        )
        .collect::<Vec<_>>()
        .join(", ");

    let mut out = format!(
        "@JS('{namespace}.{js_name}')\nexternal {js_return_type} {extern_name}({extern_params});\n\n"
    );

    let async_keyword = if signature.asynchronous { "async " } else { "" };
    out.push_str(&format!(
        "{} {dart_name}({params}) {async_keyword}{{\n",
        signature.dart_return_signature()
    ));

    let call_expr = format!("{extern_name}({arguments})");
    if signature.error_ty.is_some() && !signature.asynchronous {
        out.insert_str(
            0,
            &format!("@JS('{namespace}.{js_name}')\nexternal JSFunction get {extern_name}Ref;\n\n"),
        );
    }
    let awaited_expr = signature.invoke(
        &call_expr,
        &format!("{extern_name}Ref"),
        "null",
        &arguments,
        context,
    )?;

    let body_statement = if signature.return_ty.is_none() {
        format!("{awaited_expr};")
    } else if signature.asynchronous {
        let decoded = signature.decode_return("__boltffiAwaited", context)?;
        format!("final __boltffiAwaited = {awaited_expr};\n    return {decoded};")
    } else {
        let decoded = signature.decode_return(&awaited_expr, context)?;
        format!("return {decoded};")
    };
    if signature.asynchronous {
        out.push_str(&format!(
            "  {}\n}}\n\n",
            signature.wrap_cancellable_async_call(&signature.call_body(&body_statement))
        ));
    } else {
        out.push_str(&format!(
            "  {}\n}}\n\n",
            signature.call_body(&body_statement)
        ));
    }

    Ok(out)
}
