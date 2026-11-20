use super::*;

pub struct Stream {
    source: String,
}

impl Stream {
    pub fn from_declaration(
        decl: &StreamDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
        namespace: &str,
    ) -> Result<Self> {
        let item_ty = match decl.item() {
            StreamItemPlan::Direct { ty, .. } => direct_primitive(ty)?,
            StreamItemPlan::Encoded { ty, .. } => ty.clone(),
            _ => return Err(unsupported("stream item plan")),
        };
        let dart_item_type = interop::dart_type(&item_ty, context)?;
        let decode_item = interop::from_js("__boltffiItem", &item_ty, context)?;
        let method_name = Name::new(decl.name()).dart_identifier();
        let js_name = Name::new(decl.name()).js_member_name();
        let callback_mode = matches!(decl.mode(), boltffi_binding::StreamMode::Callback);

        let js_item_callback =
            format!("((JSAny? __boltffiItem) {{ __boltffiController.add({decode_item}); }}).toJS");

        let extern_decl = match decl.owner() {
            Some(_) => String::new(),
            None => {
                let extern_name = format!("_boltffiExtern_{js_name}");
                format!(
                    "@JS('{namespace}.{js_name}')\nexternal JSObject {extern_name}([JSAny? callback]);\n\n"
                )
            }
        };

        let session_or_cancellable_expr = match decl.owner() {
            Some(_) if callback_mode => {
                format!(
                    "(js).callMethodVarArgs('{js_name}'.toJS, [{js_item_callback}]) as JSObject"
                )
            }
            Some(_) => format!("(js).callMethodVarArgs('{js_name}'.toJS, []) as JSObject"),
            None if callback_mode => {
                format!("_boltffiExtern_{js_name}({js_item_callback})")
            }
            None => format!("_boltffiExtern_{js_name}()"),
        };
        let cancellable_expr = if callback_mode {
            session_or_cancellable_expr.clone()
        } else {
            format!(
                "({session_or_cancellable_expr}).callMethodVarArgs('consume'.toJS, [{js_item_callback}]) as JSObject"
            )
        };

        let body = match decl.mode() {
            boltffi_binding::StreamMode::Batch => {
                let decoded = interop::from_js("item", &item_ty, context)?;
                format!(
                    "$$BoltStreamPopBatchHandle<{dart_item_type}> {method_name}() {{\n  final session = {session_or_cancellable_expr};\n  return $$BoltStreamPopBatchHandle(\n    popBatch: (maxCount) => (session.callMethodVarArgs('popBatch'.toJS, [maxCount.toJS]) as JSArray<JSAny?>).toDart.map((item) => {decoded}).toList(),\n    cancel: () => session.callMethodVarArgs('dispose'.toJS, []),\n  );\n}}\n"
                )
            }
            _ => {
                let body = format!(
                    "_boltffiStream<{dart_item_type}>((callback) => {cancellable_expr}, (__boltffiItem) => {decode_item})"
                );
                let body = body.replace(&js_item_callback, "callback");
                if callback_mode {
                    format!(
                        "StreamSubscription<{dart_item_type}> {method_name}(void Function({dart_item_type}) callback) => {body}.listen(callback);\n"
                    )
                } else {
                    format!("Stream<{dart_item_type}> {method_name}() => {body};\n")
                }
            }
        };

        let source = match decl.owner() {
            Some(id) => {
                let owner = context
                    .class(id)
                    .ok_or_else(|| unsupported("stream owner without declaration"))?;
                let owner_name = Name::new(owner.name()).dart_type_name();
                format!(
                    "extension {owner_name}${method_name}Stream on {owner_name} {{\n  {body}}}\n\n"
                )
            }
            None => format!("{extern_decl}{body}"),
        };

        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}
