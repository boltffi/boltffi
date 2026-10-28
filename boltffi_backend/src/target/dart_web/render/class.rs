use super::*;

pub struct Class {
    source: String,
    diagnostics: Vec<Diagnostic>,
}

fn render_class_initializer(
    initializer: &boltffi_binding::InitializerDecl<Wasm32>,
    name: &str,
    class_ref: &str,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let js_name = Name::new(initializer.name()).js_member_name();
    let signature = call_signature(initializer.callable(), context)?;
    let params = signature.dart_params_decl_with_cancellation();
    let arguments = signature.js_call_arguments_with_cancellation();
    let call_js = format!("{class_ref}Methods({class_ref})._call_{js_name}({arguments})");
    let call_js = signature.invoke(
        &call_js,
        &format!("{class_ref}.getProperty('{js_name}'.toJS) as JSFunction"),
        class_ref,
        &arguments,
        context,
    )?;
    Ok(if signature.asynchronous {
        let method_name = Name::new(initializer.name()).dart_identifier();
        let body_statement = format!("return {name}._(({call_js}) as JSObject);");
        format!(
            "  static Future<{name}> {method_name}({params}) async {{\n    {}\n  }}",
            signature.wrap_cancellable_async_call(&signature.call_body(&body_statement))
        )
    } else {
        {
            let method = Name::new(initializer.name()).dart_identifier();
            let constructor = if matches!(method.as_str(), "new" | "$new") {
                name.to_owned()
            } else {
                format!("{name}.{method}")
            };
            format!(
                "  factory {constructor}({params}) {{ {} }}",
                signature.call_body(&format!("return {name}._({call_js} as JSObject);"))
            )
        }
    })
}

fn render_class_method(
    method: &boltffi_binding::ExportedMethodDecl<Wasm32, boltffi_binding::NativeSymbol>,
    class_ref: &str,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let method_name = Name::new(method.name()).dart_identifier();
    let js_name = Name::new(method.name()).js_member_name();
    let signature = call_signature(method.callable(), context)?;
    let params = signature.dart_params_decl_with_cancellation();
    let is_static = method.callable().receiver().is_none();
    let target = if is_static {
        class_ref.to_owned()
    } else {
        "js".to_owned()
    };
    let js_arguments = signature.js_call_arguments_with_cancellation();
    let call_js = format!("{class_ref}Methods({target})._call_{js_name}({js_arguments})");
    let keyword = if is_static { "static " } else { "" };
    let async_keyword = if signature.asynchronous { "async " } else { "" };
    let call_expr = signature.invoke(
        &call_js,
        &format!("({target}).getProperty('{js_name}'.toJS) as JSFunction"),
        &target,
        &js_arguments,
        context,
    )?;
    if signature.asynchronous {
        let body_statement = if signature.return_ty.is_none() {
            format!("{call_expr};")
        } else {
            let decoded = signature.decode_return("__boltffiAwaited", context)?;
            format!("final __boltffiAwaited = {call_expr};\n    return {decoded};")
        };
        return Ok(format!(
            "  {keyword}{} {method_name}({params}) {async_keyword}{{\n    {}\n  }}",
            signature.dart_return_signature(),
            signature.wrap_cancellable_async_call(&signature.call_body(&body_statement))
        ));
    }
    let body = if signature.return_ty.is_none() {
        format!("{{ {} }}", signature.call_body(&format!("{call_expr};")))
    } else {
        let decoded = signature.decode_return(&call_expr, context)?;
        format!(
            "{{ {} }}",
            signature.call_body(&format!("return {decoded};"))
        )
    };
    Ok(format!(
        "  {keyword}{} {method_name}({params}) {async_keyword}{body}",
        signature.dart_return_signature()
    ))
}

impl Class {
    pub fn from_declaration(
        decl: &ClassDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
        namespace: &str,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let class_ref = format!("_boltffi{name}Class");

        let (members, diagnostics) = decl
            .initializers()
            .iter()
            .map(|initializer| {
                (
                    initializer.name(),
                    render_class_initializer(initializer, &name, &class_ref, context),
                )
            })
            .chain(decl.methods().iter().map(|method| {
                (
                    method.name(),
                    render_class_method(method, &class_ref, context),
                )
            }))
            .try_fold(
                (Vec::new(), Vec::new()),
                |(mut rendered, mut diagnostics), (member_name, result)| match result {
                    Ok(member) => {
                        rendered.push(member);
                        Ok((rendered, diagnostics))
                    }
                    Err(Error::UnsupportedTarget { shape, .. })
                        if matches!(context.coverage_mode(), CoverageMode::Partial) =>
                    {
                        diagnostics.push(Diagnostic::new(format!(
                            "{}: {shape}",
                            member_name.as_path_string()
                        )));
                        Ok((rendered, diagnostics))
                    }
                    Err(error) => Err(error),
                },
            )?;
        let dispatch = decl
            .initializers()
            .iter()
            .map(|m| (m.name(), m.callable()))
            .chain(decl.methods().iter().map(|m| (m.name(), m.callable())))
            .filter_map(|(member, callable)| {
                if call_signature(callable, context).is_err() {
                    return None;
                }
                let js_name = Name::new(member).js_member_name();
                let args = (0..callable.params().len())
                    .map(|i| format!("JSAny? arg{i}"))
                    .chain(
                        matches!(callable.execution(), ExecutionDecl::Asynchronous(_))
                            .then(|| "JSAny? options".to_owned()),
                    )
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!(
                    "  @JS('{js_name}')\n  external JSAny? _call_{js_name}({args});"
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let members = members.join("\n");

        let source = format!(
            "@JS('{namespace}.{name}')\nexternal JSObject get {class_ref};\n\nextension type {class_ref}Methods(JSObject _) implements JSObject {{\n{dispatch}\n  external void dispose();\n}}\n\n\
             final class {name} {{\n  final JSObject js;\n  const {name}._(this.js);\n\n  static {name} fromJS(JSObject js) => {name}._(js);\n\n\
             \x20\x20// Releases the Rust handle without waiting for GC.\n\
             \x20\x20void dispose$() {{\n\
             \x20\x20\x20\x20{class_ref}Methods(js).dispose();\n\
             \x20\x20}}\n\n{members}\n}}\n\n",
        );

        Ok(Self {
            source,
            diagnostics,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()).with_diagnostics(self.diagnostics.clone()))
    }
}
