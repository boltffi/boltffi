use boltffi_binding::{
    CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl, CustomTypeDecl, DefaultValue,
    DirectValueType, DirectVectorElementType, Direction, EnumDecl, ExecutionDecl, ExportedCallable,
    FunctionDecl, ImportedCallable, ParamDirection, ParamPlan, RecordDecl, ReturnPlan, StreamDecl,
    StreamItemPlan, TypeRef, Wasm32,
};

use crate::core::{
    Emitted, Error, FileLayout, FilePath, FilePlan, GeneratedOutput, RenderContext,
    RenderedDeclaration, Result,
};

use super::interop;
use super::name_style::Name;

fn unsupported(shape: &'static str) -> Error {
    Error::UnsupportedTarget {
        target: "dart_web",
        shape,
    }
}

struct Boundary {
    dart_type: String,
    ty: TypeRef,
}

fn direct_primitive(ty: &DirectValueType) -> Result<TypeRef> {
    match ty {
        DirectValueType::Primitive(primitive) => Ok(TypeRef::Primitive(*primitive)),
        DirectValueType::Record(id) => Ok(TypeRef::Record(*id)),
        DirectValueType::Enum(id) => Ok(TypeRef::Enum(*id)),
        _ => Err(unsupported("direct value type")),
    }
}

fn direct_vector_type(element: &DirectVectorElementType) -> Result<TypeRef> {
    match element {
        DirectVectorElementType::Primitive(primitive) => Ok(TypeRef::Sequence(Box::new(
            TypeRef::Primitive(primitive.primitive()),
        ))),
        DirectVectorElementType::Record(_) => Err(unsupported("direct-record vector")),
        _ => Err(unsupported("direct vector element type")),
    }
}

fn boundary_for_param<D: Direction>(
    plan: &ParamPlan<Wasm32, D>,
    context: &RenderContext<Wasm32>,
) -> Result<Boundary> {
    match plan {
        ParamPlan::Direct { ty, .. } => {
            let ty = direct_primitive(ty)?;
            Ok(Boundary {
                dart_type: interop::dart_type(&ty, context)?,
                ty,
            })
        }
        ParamPlan::Encoded { ty, .. } => Ok(Boundary {
            dart_type: interop::dart_type(ty, context)?,
            ty: ty.clone(),
        }),
        ParamPlan::Handle { target, .. } => {
            let ty = handle_type_ref(target)?;
            Ok(Boundary {
                dart_type: interop::dart_type(&ty, context)?,
                ty,
            })
        }
        ParamPlan::ScalarOption { primitive } => {
            let ty = TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive)));
            Ok(Boundary {
                dart_type: interop::dart_type(&ty, context)?,
                ty,
            })
        }
        ParamPlan::DirectVec { element, .. } => {
            let ty = direct_vector_type(element)?;
            Ok(Boundary {
                dart_type: interop::dart_type(&ty, context)?,
                ty,
            })
        }
        _ => Err(unsupported("param plan")),
    }
}

fn handle_type_ref(target: &boltffi_binding::HandleTarget) -> Result<TypeRef> {
    match target {
        boltffi_binding::HandleTarget::Class(id) => Ok(TypeRef::Class(*id)),
        boltffi_binding::HandleTarget::Callback(id) => Ok(TypeRef::Callback(*id)),
        boltffi_binding::HandleTarget::Stream(_) => Err(unsupported("stream handle")),
        _ => Err(unsupported("handle target")),
    }
}

struct ParamInfo {
    dart_type: String,
    js_call_expr: String,
    value_ty: Option<TypeRef>,
}

struct CallSignature {
    params: Vec<ParamInfo>,
    return_dart_type: String,
    return_ty: Option<TypeRef>,
    asynchronous: bool,
}

impl CallSignature {
    fn dart_params_decl(&self) -> String {
        self.params
            .iter()
            .map(|param| param.dart_type.clone())
            .zip(0..)
            .map(|(ty, index)| format!("{ty} arg{index}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn js_call_arguments(&self) -> String {
        self.params
            .iter()
            .map(|param| param.js_call_expr.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn dart_return_signature(&self) -> String {
        if self.asynchronous {
            format!("Future<{}>", self.return_dart_type)
        } else {
            self.return_dart_type.clone()
        }
    }

    fn decode_return(&self, raw_expr: &str, context: &RenderContext<Wasm32>) -> Result<String> {
        match &self.return_ty {
            None => Ok(String::new()),
            Some(ty) => interop::from_js(raw_expr, ty, context),
        }
    }

    fn encode_return(&self, dart_expr: &str, context: &RenderContext<Wasm32>) -> Result<String> {
        match &self.return_ty {
            None => Ok(String::new()),
            Some(ty) => interop::to_js(dart_expr, ty, context),
        }
    }
}

fn call_signature(
    callable: &ExportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<CallSignature> {
    let mut params = Vec::new();

    for (index, param) in callable.params().iter().enumerate() {
        let dart_name = format!("arg{index}");
        match param.payload() {
            boltffi_binding::IncomingParam::Value(plan) => {
                let boundary = boundary_for_param(plan, context)?;
                let js_call_expr = interop::to_js(&dart_name, &boundary.ty, context)?;
                params.push(ParamInfo {
                    dart_type: boundary.dart_type,
                    js_call_expr,
                    value_ty: Some(boundary.ty),
                });
            }
            boltffi_binding::IncomingParam::Closure(closure) => {
                params.push(closure_param_info(closure, &dart_name, context)?);
            }
        }
    }

    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    let (return_dart_type, return_ty) = return_boundary(callable.returns().plan(), context)?;

    Ok(CallSignature {
        params,
        return_dart_type,
        return_ty,
        asynchronous,
    })
}

fn closure_param_info(
    closure: &boltffi_binding::ClosureParameter<Wasm32, boltffi_binding::IntoRust>,
    dart_name: &str,
    context: &RenderContext<Wasm32>,
) -> Result<ParamInfo> {
    let inner = callback_method_signature(closure.invoke(), context)?;
    let dart_type = format!(
        "{} Function({})",
        inner.return_dart_type,
        inner
            .params
            .iter()
            .map(|param| param.dart_type.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let js_call_expr = wrap_dart_callable_as_js_function(dart_name, &inner, context)?;
    Ok(ParamInfo {
        dart_type,
        js_call_expr,
        value_ty: None,
    })
}

fn wrap_dart_callable_as_js_function(
    dart_callable_expr: &str,
    signature: &CallSignature,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let js_params = (0..signature.params.len())
        .map(|i| format!("JSAny? __jsArg{i}"))
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
                .ok_or_else(|| unsupported("nested closure parameter"))?;
            interop::from_js(&format!("__jsArg{i}"), ty, context)
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let call_expr = format!("{dart_callable_expr}({decoded_args})");
    let body = if signature.return_ty.is_none() {
        format!("{{ {call_expr}; }}")
    } else {
        let encoded = signature.encode_return("__boltffiResult", context)?;
        format!("{{ final __boltffiResult = {call_expr}; return {encoded}; }}")
    };
    Ok(format!("(({js_params}) {body}).toJS"))
}

fn callback_method_signature(
    callable: &ImportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<CallSignature> {
    let mut params = Vec::new();

    for (index, param) in callable.params().iter().enumerate() {
        let dart_name = format!("arg{index}");
        let value = param
            .payload()
            .as_value()
            .ok_or_else(|| unsupported("closure parameter on a callback/closure body"))?;
        let boundary = boundary_for_param(value, context)?;
        let js_call_expr = interop::to_js(&dart_name, &boundary.ty, context)?;
        params.push(ParamInfo {
            dart_type: boundary.dart_type,
            js_call_expr,
            value_ty: Some(boundary.ty),
        });
    }

    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    let (return_dart_type, return_ty) = return_boundary(callable.returns().plan(), context)?;

    Ok(CallSignature {
        params,
        return_dart_type,
        return_ty,
        asynchronous,
    })
}

fn return_boundary<D: Direction>(
    plan: &ReturnPlan<Wasm32, D>,
    context: &RenderContext<Wasm32>,
) -> Result<(String, Option<TypeRef>)>
where
    D::Opposite: ParamDirection<Wasm32>,
{
    Ok(match plan {
        ReturnPlan::Void => ("void".to_owned(), None),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            let ty = direct_primitive(ty)?;
            let dart_type = interop::dart_type(&ty, context)?;
            (dart_type, Some(ty))
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => {
            let dart_type = interop::dart_type(ty, context)?;
            (dart_type, Some(ty.clone()))
        }
        ReturnPlan::HandleViaReturnSlot { target, .. }
        | ReturnPlan::HandleViaOutPointer { target, .. } => {
            let ty = handle_type_ref(target)?;
            let dart_type = interop::dart_type(&ty, context)?;
            (dart_type, Some(ty))
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
            let ty = TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive)));
            let dart_type = interop::dart_type(&ty, context)?;
            (dart_type, Some(ty))
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let ty = direct_vector_type(element)?;
            let dart_type = interop::dart_type(&ty, context)?;
            (dart_type, Some(ty))
        }
        ReturnPlan::ClosureViaOutPointer(_) => return Err(unsupported("closure return")),
        _ => return Err(unsupported("return plan")),
    })
}

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
    let params = signature.dart_params_decl();
    let arguments = signature.js_call_arguments();
    let extern_name = format!("_boltffiExtern_{js_name}");

    let js_return_type = if signature.asynchronous {
        "JSPromise<JSAny?>".to_owned()
    } else {
        "JSAny?".to_owned()
    };
    let extern_params = (0..signature.params.len())
        .map(|i| format!("JSAny? arg{i}"))
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
    let awaited_expr = if signature.asynchronous {
        format!("(await ({call_expr}).toDart)")
    } else {
        call_expr
    };

    if signature.return_ty.is_none() {
        out.push_str(&format!("  {awaited_expr};\n}}\n\n"));
    } else {
        let decoded = signature.decode_return(&awaited_expr, context)?;
        out.push_str(&format!("  return {decoded};\n}}\n\n"));
    }

    Ok(out)
}

pub struct Record {
    source: String,
}

impl Record {
    pub fn from_declaration(
        decl: &RecordDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let fields: Vec<(String, TypeRef)> = match decl {
            RecordDecl::Direct(record) => record
                .fields()
                .iter()
                .map(|field| {
                    let key = field_key_name(field.key());
                    let ty = TypeRef::Primitive(field.ty().primitive());
                    (key, ty)
                })
                .collect(),
            RecordDecl::Encoded(record) => record
                .fields()
                .iter()
                .map(|field| (field_key_name(field.key()), field.ty().clone()))
                .collect(),
            _ => return Err(unsupported("record declaration")),
        };

        let source = render_data_class(&name, None, &fields, context)?;
        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

fn render_data_class(
    name: &str,
    extends: Option<(&str, &str)>,
    fields: &[(String, TypeRef)],
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let mut field_decls = Vec::new();
    let mut ctor_params = Vec::new();
    let mut to_js_entries = Vec::new();
    let mut from_js_args = Vec::new();

    if let Some((_, tag)) = extends {
        to_js_entries.push(format!("    result.setProperty('tag'.toJS, '{tag}'.toJS);"));
    }

    for (field_name, ty) in fields {
        let dart_type = interop::dart_type(ty, context)?;
        field_decls.push(format!("  final {dart_type} {field_name};"));
        ctor_params.push(format!("required this.{field_name}"));
        let to_js = interop::to_js(field_name, ty, context)?;
        to_js_entries.push(format!(
            "    result.setProperty('{field_name}'.toJS, {to_js});"
        ));
        let from_js =
            interop::from_js(&format!("js.getProperty('{field_name}'.toJS)"), ty, context)?;
        from_js_args.push(format!("{field_name}: {from_js}"));
    }

    let (header, override_kw, extra_ctor) = match extends {
        Some((base, _)) => (
            format!("class {name} extends {base}"),
            "@override\n  ",
            " : super._()".to_owned(),
        ),
        None => (format!("class {name}"), "", String::new()),
    };

    Ok(format!(
        "{header} {{\n{fields}\n\n  const {name}({{{ctor}}}){extra_ctor};\n\n  {override_kw}JSObject toJS() {{\n    final result = JSObject();\n{to_js}\n    return result;\n  }}\n\n  static {name} fromJS(JSObject js) {{\n    return {name}({from_js});\n  }}\n}}\n\n",
        fields = field_decls.join("\n"),
        ctor = ctor_params.join(", "),
        to_js = to_js_entries.join("\n"),
        from_js = from_js_args.join(", "),
    ))
}

fn field_key_name(key: &boltffi_binding::FieldKey) -> String {
    match key {
        boltffi_binding::FieldKey::Named(name) => Name::new(name).dart_identifier(),
        boltffi_binding::FieldKey::Position(position) => format!("field{position}"),
        _ => "field".to_owned(),
    }
}

pub struct Enumeration {
    source: String,
}

impl Enumeration {
    pub fn from_declaration(
        decl: &EnumDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let source = match decl {
            EnumDecl::CStyle(cstyle) => Self::c_style(cstyle)?,
            EnumDecl::Data(data) => Self::data(data, context)?,
            _ => return Err(unsupported("enum declaration")),
        };
        Ok(Self { source })
    }

    fn c_style(decl: &boltffi_binding::CStyleEnumDecl<Wasm32>) -> Result<String> {
        let name = Name::new(decl.name()).dart_type_name();
        let mut variant_entries = Vec::new();
        let mut from_raw_cases = Vec::new();
        for variant in decl.variants() {
            let variant_name = Name::new(variant.name()).dart_type_name();
            let value = variant.discriminant().get();
            variant_entries.push(format!(
                "  static const {variant_name} = {name}._({value});"
            ));
            from_raw_cases.push(format!("      case {value}: return {name}.{variant_name};"));
        }

        Ok(format!(
            "class {name} {{\n  final int value;\n  const {name}._(this.value);\n\n{variants}\n\n  JSAny toJS() => value.toJS;\n\n  static {name} fromJS(JSAny js) => _fromRaw((js as JSNumber).toDartInt);\n\n  static {name} _fromRaw(int value) {{\n    switch (value) {{\n{cases}\n      default: throw StateError('Unknown {name} value: \\$value');\n    }}\n  }}\n}}\n\n",
            variants = variant_entries.join("\n"),
            cases = from_raw_cases.join("\n"),
        ))
    }

    fn data(
        decl: &boltffi_binding::DataEnumDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        let name = Name::new(decl.name()).dart_type_name();
        let mut variant_classes = Vec::new();
        let mut from_js_cases = Vec::new();

        for variant in decl.variants() {
            let variant_dart_name = Name::new(variant.name()).dart_type_name();
            // Must match target::typescript's wire tag spelling exactly.
            let tag = variant_dart_name.clone();
            let variant_type = format!("{name}${variant_dart_name}");
            let fields: Vec<(String, TypeRef)> = variant
                .payload()
                .fields()
                .iter()
                .map(|field| (field_key_name(field.key()), field.ty().clone()))
                .collect();

            variant_classes.push(render_data_class(
                &variant_type,
                Some((&name, &tag)),
                &fields,
                context,
            )?);

            let from_js_args = fields
                .iter()
                .map(|(field_name, ty)| {
                    let from_js = interop::from_js(
                        &format!("js.getProperty('{field_name}'.toJS)"),
                        ty,
                        context,
                    )?;
                    Ok(format!("{field_name}: {from_js}"))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ");

            from_js_cases.push(format!(
                "      case '{tag}': return {variant_type}({from_js_args});"
            ));
        }

        Ok(format!(
            "abstract class {name} {{\n  const {name}._();\n\n  JSObject toJS();\n\n  static {name} fromJS(JSObject js) {{\n    final tag = (js.getProperty('tag'.toJS) as JSString).toDart;\n    switch (tag) {{\n{cases}\n      default: throw StateError('Unknown {name} tag: \\$tag');\n    }}\n  }}\n}}\n\n{variants}",
            cases = from_js_cases.join("\n"),
            variants = variant_classes.join(""),
        ))
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

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

        for method in decl.protocol().methods() {
            let method_name = Name::new(method.name()).dart_identifier();
            let js_name = Name::new(method.name()).js_member_name();
            let signature = callback_method_signature(method.callable(), context)?;
            let dart_params = signature.dart_params_decl();
            let public_return = signature.dart_return_signature();

            interface_methods.push(format!("  {public_return} {method_name}({dart_params});"));

            // `@JSExport` doesn't turn an async method's Future into a
            // real Promise (verified in-browser: no `.then`), so this
            // stays synchronous and converts the inner Future via `.toJS`.
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
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            let impl_call = format!("_impl.{method_name}({decoded_args})");
            let adapter_method = if signature.asynchronous {
                let inner_body = if signature.return_ty.is_none() {
                    format!("{{ await {impl_call}; }}")
                } else {
                    let encoded = signature.encode_return("__boltffiResult", context)?;
                    format!("{{ final __boltffiResult = await {impl_call}; return {encoded}; }}")
                };
                format!(
                    "  JSPromise<JSAny?> {method_name}({adapter_js_params}) {{\n    return (() async {inner_body})().toJS;\n  }}"
                )
            } else {
                let body = if signature.return_ty.is_none() {
                    format!("{{ {impl_call}; }}")
                } else {
                    let encoded = signature.encode_return("__boltffiResult", context)?;
                    format!("{{ final __boltffiResult = {impl_call}; return {encoded}; }}")
                };
                format!("  JSAny? {method_name}({adapter_js_params}) {body}")
            };
            adapter_methods.push(adapter_method);

            let js_arguments = signature.js_call_arguments();
            let raw_call = format!("_js.callMethodVarArgs('{js_name}'.toJS, [{js_arguments}])");
            let (wrapper_async, raw_result) = if signature.asynchronous {
                (
                    "async ",
                    format!("(await ({raw_call} as JSPromise<JSAny?>).toDart)"),
                )
            } else {
                ("", raw_call)
            };
            if signature.return_ty.is_none() {
                wrapper_methods.push(format!(
                    "  @override\n  {public_return} {method_name}({dart_params}) {wrapper_async}{{ {raw_result}; }}"
                ));
            } else {
                let decoded = signature.decode_return(&raw_result, context)?;
                wrapper_methods.push(format!(
                    "  @override\n  {public_return} {method_name}({dart_params}) {wrapper_async}{{ return {decoded}; }}"
                ));
            }
        }

        let source = format!(
            "abstract interface class {name} {{\n{interface}\n}}\n\n\
             @JSExport()\nclass _{name}JSAdapter {{\n  final {name} _impl;\n  _{name}JSAdapter(this._impl);\n\n{adapter}\n}}\n\n\
             /// Wraps a raw JS object that already speaks {name}'s wire contract so\n\
             /// it can be handed to Rust without routing calls through the\n\
             /// dart2js/dart2wasm module at all. Passing an instance of this class\n\
             /// anywhere a {name} is expected skips the usual `rust -> js -> dart\n\
             /// -> js -> rust` round trip in favor of a plain `rust -> js -> rust`\n\
             /// call.\n\
             final class {name}JsWrapper implements {name} {{\n  final JSObject js;\n  const {name}JsWrapper(this.js);\n  JSObject get _js => js;\n\n{wrapper}\n}}\n\n\
             JSObject boltffiCallbackToJS{name}({name} callback) {{\n  if (callback is {name}JsWrapper) return callback.js;\n  return createJSInteropWrapper(_{name}JSAdapter(callback));\n}}\n\n",
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

// Accessor constants need a module-init hook this target doesn't have yet.
pub struct Constant {
    source: String,
}

impl Constant {
    pub fn from_declaration(
        decl: &ConstantDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        if decl.owner().is_some() {
            return Ok(Self {
                source: String::new(),
            });
        }
        let name = Name::new(decl.name()).dart_constant_name();
        let source = match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                let dart_type = interop::dart_type(ty, context)?;
                let literal = render_default_value(value)?;
                format!("final {dart_type} {name} = {literal};\n\n")
            }
            ConstantValueDecl::Accessor { .. } => return Err(unsupported("accessor constant")),
            _ => return Err(unsupported("constant value shape")),
        };
        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

fn render_default_value(value: &DefaultValue) -> Result<String> {
    Ok(match value {
        DefaultValue::Bool(value) => value.to_string(),
        DefaultValue::Integer(value) => value.get().to_string(),
        DefaultValue::Float(value) => format!("{:?}", value.to_f64()),
        DefaultValue::String(value) => super::syntax::dart_string_literal(value),
        DefaultValue::EnumVariant {
            enum_name,
            variant_name,
        } => format!(
            "{}.{}",
            Name::new(enum_name).dart_type_name(),
            Name::new(variant_name).dart_type_name()
        ),
        DefaultValue::Null => "null".to_owned(),
        _ => return Err(unsupported("constant literal shape")),
    })
}

pub struct CustomType {
    source: String,
}

impl CustomType {
    pub fn from_declaration(
        decl: &CustomTypeDecl,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let representation = interop::dart_type(decl.representation(), context)?;
        Ok(Self {
            source: format!("typedef {name} = {representation};\n\n"),
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

pub struct Class {
    source: String,
}

impl Class {
    pub fn from_declaration(
        decl: &ClassDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
        namespace: &str,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let class_ref = format!("_boltffi{name}Class");
        let mut members = Vec::new();

        for initializer in decl.initializers() {
            let method_name = Name::new(initializer.name()).dart_identifier();
            let js_name = Name::new(initializer.name()).js_member_name();
            let signature = call_signature(initializer.callable(), context)?;
            let params = signature.dart_params_decl();
            let arguments = signature.js_call_arguments();
            members.push(format!(
                "  static {name} {method_name}({params}) => {name}._({class_ref}.callMethodVarArgs('{js_name}'.toJS, [{arguments}]) as JSObject);",
            ));
        }

        for method in decl.methods() {
            let method_name = Name::new(method.name()).dart_identifier();
            let js_name = Name::new(method.name()).js_member_name();
            let signature = call_signature(method.callable(), context)?;
            let params = signature.dart_params_decl();
            let is_static = method.callable().receiver().is_none();
            let target = if is_static {
                class_ref.clone()
            } else {
                "js".to_owned()
            };
            let js_arguments = signature.js_call_arguments();
            let call_js =
                format!("({target}).callMethodVarArgs('{js_name}'.toJS, [{js_arguments}])");
            let keyword = if is_static { "static " } else { "" };
            let async_keyword = if signature.asynchronous { "async " } else { "" };
            let call_expr = if signature.asynchronous {
                format!("(await ({call_js} as JSPromise<JSAny?>).toDart)")
            } else {
                call_js
            };
            let body = if signature.return_ty.is_none() {
                format!("{{ {call_expr}; }}")
            } else {
                let decoded = signature.decode_return(&call_expr, context)?;
                format!("=> {decoded};")
            };
            members.push(format!(
                "  {keyword}{} {async_keyword}{method_name}({params}) {body}",
                signature.dart_return_signature()
            ));
        }

        let source = format!(
            "@JS('{namespace}.{name}')\nexternal JSObject get {class_ref};\n\n\
             class {name} {{\n  final JSObject js;\n  const {name}._(this.js);\n\n  static {name} fromJS(JSObject js) => {name}._(js);\n\n{members}\n}}\n\n",
            members = members.join("\n"),
        );

        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

// Every Rust-side stream mode (Async/Batch/Callback) unifies to the same
// Dart Stream<T>: target::typescript's StreamSession.consume(callback) /
// StreamCancellable are public JS methods regardless of mode, so this
// never touches the poll/wake protocol directly.
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

        let (call_target, extern_decl) = match decl.owner() {
            Some(_) => ("(js)".to_owned(), String::new()),
            None => {
                let extern_name = format!("_boltffiExtern_{js_name}");
                (
                    extern_name.clone(),
                    format!(
                        "@JS('{namespace}.{js_name}')\nexternal JSObject {extern_name}([JSAny? callback]);\n\n"
                    ),
                )
            }
        };

        let cancellable_expr = if callback_mode {
            format!(
                "{call_target}.callMethodVarArgs('{js_name}'.toJS, [{js_item_callback}]) as JSObject"
            )
        } else {
            format!(
                "({call_target}.callMethodVarArgs('{js_name}'.toJS, []) as JSObject).callMethodVarArgs('consume'.toJS, [{js_item_callback}]) as JSObject"
            )
        };

        let body = format!(
            "Stream<{dart_item_type}> {method_name}() {{\n\
             \x20\x20late final StreamController<{dart_item_type}> __boltffiController;\n\
             \x20\x20JSObject? __boltffiCancellable;\n\
             \x20\x20__boltffiController = StreamController<{dart_item_type}>(\n\
             \x20\x20\x20\x20onListen: () {{\n\
             \x20\x20\x20\x20\x20\x20__boltffiCancellable = {cancellable_expr};\n\
             \x20\x20\x20\x20\x20\x20(__boltffiCancellable!.getProperty('done'.toJS) as JSPromise).toDart\n\
             \x20\x20\x20\x20\x20\x20\x20\x20.then((_) {{ __boltffiController.close(); }});\n\
             \x20\x20\x20\x20}},\n\
             \x20\x20\x20\x20onCancel: () {{\n\
             \x20\x20\x20\x20\x20\x20__boltffiCancellable?.callMethodVarArgs('cancel'.toJS, []);\n\
             \x20\x20\x20\x20}},\n\
             \x20\x20);\n\
             \x20\x20return __boltffiController.stream;\n\
             }}\n\n"
        );

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

pub struct Module<'m> {
    name: &'m str,
    namespace: &'m str,
}

impl<'m> Module<'m> {
    pub fn new(name: &'m str, namespace: &'m str) -> Self {
        Self { name, namespace }
    }

    // Kept as one function so the loader script and this renderer can't
    // drift on the global name independently.
    pub fn ready_global(namespace: &str) -> String {
        format!("{namespace}_ready")
    }

    pub fn render<'decl>(
        &self,
        declarations: Vec<RenderedDeclaration<'decl, Wasm32>>,
    ) -> Result<GeneratedOutput> {
        let ready_global = Self::ready_global(self.namespace);
        let preamble = format!(
            "// Generated by boltffi (target: dart_web). Do not edit by hand.\n\
             import 'dart:async';\n\
             import 'dart:js_interop';\n\
             import 'dart:js_interop_unsafe';\n\
             import 'dart:typed_data';\n\n\
             @JS('{ready_global}')\n\
             external JSPromise<JSAny?> get _boltffiReady;\n\n\
             /// Waits for the wrapped wasm module to finish instantiating.\n\
             /// Must be awaited before calling anything else this package\n\
             /// exports.\n\
             Future<void> init() => _boltffiReady.toDart.then((_) {{}});\n\n"
        );
        FileLayout::new()
            .with_file(
                FilePlan::all(FilePath::new(format!("{}.dart", self.name))?)
                    .with_preamble(preamble),
            )
            .assemble_declarations(declarations)
    }
}
