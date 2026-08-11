//! Builds Dart source text for each declaration kind. Every declaration
//! renders as `dart:js_interop` glue calling straight into the
//! `target::typescript` module this target wraps — see `interop.rs` for
//! the per-`TypeRef` conversion rules this leans on.

use boltffi_binding::{
    CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl, CustomTypeDecl, DefaultValue,
    DirectValueType, DirectVectorElementType, Direction, EnumDecl, ExecutionDecl, ExportedCallable,
    FunctionDecl, ImportedCallable, ParamDirection, ParamPlan, RecordDecl, ReturnPlan, TypeRef,
    Wasm32,
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

/// The JS-facing type and Dart↔JS conversion for one function parameter
/// or return value, derived from the shared `ParamPlan`/`ReturnPlan` the
/// wasm surface already computed. This target does not remake any of
/// `target::typescript`'s marshalling decisions — it only needs to know
/// which JS-boundary shape a plan settled on (scalar / wire-encoded /
/// handle / etc.) so it can call the already-generated JS the same way.
struct Boundary {
    dart_type: String,
    ty: TypeRef,
}

/// `Direct` transport is a wasm-side ABI optimization (the value occupies
/// a native call slot instead of going through the wire); it does not
/// change the JS-facing declared type at all, so a direct record/enum
/// maps to the exact same `TypeRef` its encoded counterpart would.
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

/// A function/method/initializer body, shared across free functions,
/// class initializers, and class/callback methods — they only differ in
/// how the JS call target is named and whether a receiver is prepended.
struct CallSignature {
    dart_params: Vec<String>,
    js_arguments: Vec<String>,
    return_dart_type: String,
    return_expr_wrapper: Box<dyn Fn(&str) -> String>,
    asynchronous: bool,
}

fn call_signature(
    callable: &ExportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<CallSignature> {
    let mut dart_params = Vec::new();
    let mut js_arguments = Vec::new();

    for (index, param) in callable.params().iter().enumerate() {
        let value = param
            .payload()
            .as_value()
            .ok_or_else(|| unsupported("closure parameter"))?;
        let boundary = boundary_for_param(value, context)?;
        let dart_name = format!("arg{index}");
        dart_params.push(format!("{} {dart_name}", boundary.dart_type));
        js_arguments.push(interop::to_js(&dart_name, &boundary.ty, context)?);
    }

    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    let (return_dart_type, return_expr_wrapper) =
        return_boundary(callable.returns().plan(), context)?;

    Ok(CallSignature {
        dart_params,
        js_arguments,
        return_dart_type,
        return_expr_wrapper,
        asynchronous,
    })
}

/// Same idea as `call_signature`, but for a callback *method*: Rust is
/// the caller here, so parameters flow `OutOfRust` (Rust -> foreign) and
/// the return flows `IntoRust` (foreign -> Rust) — the opposite of a
/// free function/initializer/class method. Only synchronous methods are
/// handled; async callback methods need a distinct completion protocol
/// this target does not implement yet (see `Callback::from_declaration`).
fn callback_method_signature(
    callable: &ImportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<CallSignature> {
    let mut dart_params = Vec::new();
    let mut js_arguments = Vec::new();

    for (index, param) in callable.params().iter().enumerate() {
        let value = param
            .payload()
            .as_value()
            .ok_or_else(|| unsupported("closure parameter"))?;
        let boundary = boundary_for_param(value, context)?;
        let dart_name = format!("arg{index}");
        dart_params.push(format!("{} {dart_name}", boundary.dart_type));
        js_arguments.push(interop::to_js(&dart_name, &boundary.ty, context)?);
    }

    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    let (return_dart_type, return_expr_wrapper) =
        return_boundary(callable.returns().plan(), context)?;

    Ok(CallSignature {
        dart_params,
        js_arguments,
        return_dart_type,
        return_expr_wrapper,
        asynchronous,
    })
}

#[allow(clippy::type_complexity)]
fn return_boundary<D: Direction>(
    plan: &ReturnPlan<Wasm32, D>,
    context: &RenderContext<Wasm32>,
) -> Result<(String, Box<dyn Fn(&str) -> String>)>
where
    D::Opposite: ParamDirection<Wasm32>,
{
    Ok(match plan {
        ReturnPlan::Void => ("void".to_owned(), Box::new(|_: &str| String::new())),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            let ty = direct_primitive(ty)?;
            let dart_type = interop::dart_type(&ty, context)?;
            let from = interop::from_js("__boltffiRaw", &ty, context)?;
            (
                dart_type,
                Box::new(move |raw| from.replace("__boltffiRaw", raw)),
            )
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => {
            let dart_type = interop::dart_type(ty, context)?;
            let from = interop::from_js("__boltffiRaw", ty, context)?;
            (
                dart_type,
                Box::new(move |raw| from.replace("__boltffiRaw", raw)),
            )
        }
        ReturnPlan::HandleViaReturnSlot { target, .. }
        | ReturnPlan::HandleViaOutPointer { target, .. } => {
            let ty = handle_type_ref(target)?;
            let dart_type = interop::dart_type(&ty, context)?;
            let from = interop::from_js("__boltffiRaw", &ty, context)?;
            (
                dart_type,
                Box::new(move |raw| from.replace("__boltffiRaw", raw)),
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
            let ty = TypeRef::Optional(Box::new(TypeRef::Primitive(*primitive)));
            let dart_type = interop::dart_type(&ty, context)?;
            let from = interop::from_js("__boltffiRaw", &ty, context)?;
            (
                dart_type,
                Box::new(move |raw| from.replace("__boltffiRaw", raw)),
            )
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let ty = direct_vector_type(element)?;
            let dart_type = interop::dart_type(&ty, context)?;
            let from = interop::from_js("__boltffiRaw", &ty, context)?;
            (
                dart_type,
                Box::new(move |raw| from.replace("__boltffiRaw", raw)),
            )
        }
        ReturnPlan::ClosureViaOutPointer(_) => return Err(unsupported("closure return")),
        _ => return Err(unsupported("return plan")),
    })
}

/// Renders one free function.
pub struct Function {
    source: String,
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let js_name = Name::new(decl.name()).js_export_name();
        let dart_name = Name::new(decl.name()).dart_identifier();
        let signature = call_signature(decl.callable(), context)?;
        Ok(Self {
            source: render_free_function(&js_name, &dart_name, &signature),
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

/// Renders a free function: an `@JS()` extern bound to
/// `boltffiPoc.<jsName>` plus a public Dart wrapper that converts
/// arguments/return value at the boundary.
/// The declared Dart return type for a call signature: `Future<T>` (or
/// `Future<void>`) when asynchronous, `T` otherwise.
fn dart_return_signature(signature: &CallSignature) -> String {
    if signature.asynchronous {
        format!("Future<{}>", signature.return_dart_type)
    } else {
        signature.return_dart_type.clone()
    }
}

fn render_free_function(js_name: &str, dart_name: &str, signature: &CallSignature) -> String {
    let params = signature.dart_params.join(", ");
    let arguments = signature.js_arguments.join(", ");
    let extern_name = format!("_boltffiExtern_{js_name}");

    let js_return_type = if signature.asynchronous {
        "JSPromise<JSAny?>".to_owned()
    } else {
        "JSAny?".to_owned()
    };
    let extern_params = (0..signature.dart_params.len())
        .map(|i| format!("JSAny? arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut out = format!(
        "@JS('boltffiPoc.{js_name}')\nexternal {js_return_type} {extern_name}({extern_params});\n\n"
    );

    let async_keyword = if signature.asynchronous { "async " } else { "" };
    out.push_str(&format!(
        "{} {dart_name}({params}) {async_keyword}{{\n",
        dart_return_signature(signature)
    ));

    let call_expr = format!("{extern_name}({arguments})");
    let awaited_expr = if signature.asynchronous {
        format!("(await ({call_expr}).toDart)")
    } else {
        call_expr
    };

    if signature.return_dart_type == "void" {
        out.push_str(&format!("  {awaited_expr};\n}}\n\n"));
    } else {
        let wrapped = (signature.return_expr_wrapper)(&awaited_expr);
        out.push_str(&format!("  return {wrapped};\n}}\n\n"));
    }

    out
}

/// Renders a record as a plain Dart data class matching the JS object
/// shape `target::typescript` emits for it (`interface { field: T }`).
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

/// Shared body for both plain records and data-enum variant payloads:
/// both cross as a plain JS object with one property per field.
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

    let (header, footer, override_kw, extra_ctor) = match extends {
        Some((base, _)) => (
            format!("class {name} extends {base}"),
            String::new(),
            "@override\n  ",
            " : super._()".to_owned(),
        ),
        None => (format!("class {name}"), String::new(), "", String::new()),
    };
    let _ = footer;

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

/// Renders a C-style or data enum matching the JS shape
/// `target::typescript` emits (a raw number, or a `{ tag, ...fields }`
/// object respectively).
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
            // Same spelling `target::typescript` uses for the wire tag
            // string (`Name::variant_identifier`, upper-camel of the
            // variant's canonical name) — must match exactly, since
            // that's the literal `.tag` property value on the JS side.
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

/// Renders a callback trait as a Dart interface plus the machinery to
/// cross it to JS: a `@JSExport` adapter (for a real Dart
/// implementation) and a `{Name}JsWrapper` escape hatch that skips the
/// dart2js/dart2wasm hop entirely when the caller already has a raw JS
/// object satisfying the same shape.
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
            if signature.asynchronous {
                // Async callback methods need a distinct completion
                // protocol (requestId + explicit `_complete` call) not
                // yet implemented by this target.
                return Err(unsupported("async callback method"));
            }
            let params = signature.dart_params.join(", ");
            let param_names = (0..signature.dart_params.len())
                .map(|i| format!("arg{i}"))
                .collect::<Vec<_>>()
                .join(", ");

            interface_methods.push(format!(
                "  {} {method_name}({params});",
                signature.return_dart_type
            ));

            adapter_methods.push(format!(
                "  {} {method_name}({params}) => _impl.{method_name}({param_names});",
                signature.return_dart_type
            ));

            let js_arguments = signature.js_arguments.join(", ");
            let call_js = format!("_js.callMethodVarArgs('{js_name}'.toJS, [{js_arguments}])");
            if signature.return_dart_type == "void" {
                wrapper_methods.push(format!(
                    "  @override\n  {} {method_name}({params}) {{ {call_js}; }}",
                    signature.return_dart_type
                ));
            } else {
                let wrapped = (signature.return_expr_wrapper)(&call_js);
                wrapper_methods.push(format!(
                    "  @override\n  {} {method_name}({params}) => {wrapped};",
                    signature.return_dart_type
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
            adapter = adapter_methods.join("\n"),
            wrapper = wrapper_methods.join("\n\n"),
        );

        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

/// Renders a constant. Only `ConstantValueDecl::Inline` is supported —
/// `Accessor` constants need a module-init hook this target does not
/// have yet (see module docs).
pub struct Constant {
    source: String,
}

impl Constant {
    pub fn from_declaration(
        decl: &ConstantDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        if decl.owner().is_some() {
            // Associated constants render alongside their owner; skip
            // here the same way `target::typescript` does.
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

/// Renders a custom type as a bare typedef over its wire representation.
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

/// Renders a class: static/initializer methods call the exported JS
/// class constructor object directly, instance methods forward to the
/// wrapped JS instance.
pub struct Class {
    source: String,
}

impl Class {
    pub fn from_declaration(
        decl: &ClassDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let class_ref = format!("_boltffi{name}Class");
        let mut members = Vec::new();

        for initializer in decl.initializers() {
            let method_name = Name::new(initializer.name()).dart_identifier();
            let js_name = Name::new(initializer.name()).js_member_name();
            let signature = call_signature(initializer.callable(), context)?;
            let params = signature.dart_params.join(", ");
            let arguments = signature.js_arguments.join(", ");
            members.push(format!(
                "  static {name} {method_name}({params}) => {name}._({class_ref}.callMethodVarArgs('{js_name}'.toJS, [{arguments}]) as JSObject);",
            ));
        }

        for method in decl.methods() {
            let method_name = Name::new(method.name()).dart_identifier();
            let js_name = Name::new(method.name()).js_member_name();
            let signature = call_signature(method.callable(), context)?;
            let params = signature.dart_params.join(", ");
            let is_static = method.callable().receiver().is_none();
            let target = if is_static {
                class_ref.clone()
            } else {
                "js".to_owned()
            };
            let js_arguments = signature.js_arguments.join(", ");
            let call_js =
                format!("({target}).callMethodVarArgs('{js_name}'.toJS, [{js_arguments}])");
            let keyword = if is_static { "static " } else { "" };
            let async_keyword = if signature.asynchronous { "async " } else { "" };
            let call_expr = if signature.asynchronous {
                format!("(await ({call_js}).toDart)")
            } else {
                call_js
            };
            let body = if signature.return_dart_type == "void" {
                format!("{{ {call_expr}; }}")
            } else {
                let wrapped = (signature.return_expr_wrapper)(&call_expr);
                format!("=> {wrapped};")
            };
            members.push(format!(
                "  {keyword}{} {async_keyword}{method_name}({params}) {body}",
                dart_return_signature(&signature)
            ));
        }

        let source = format!(
            "@JS('boltffiPoc.{name}')\nexternal JSObject get {class_ref};\n\n\
             class {name} {{\n  final JSObject js;\n  const {name}._(this.js);\n\n  static {name} fromJS(JSObject js) => {name}._(js);\n\n{members}\n}}\n\n",
            members = members.join("\n"),
        );

        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

/// Assembles every rendered declaration into a single `.dart` file. This
/// target only ever produces one file per package (unlike
/// `target::typescript`'s browser/node split) — the Dart side always goes
/// through `dart:js_interop`, so there is no separate "node" surface.
pub struct Module<'m> {
    name: &'m str,
}

impl<'m> Module<'m> {
    pub fn new(name: &'m str) -> Self {
        Self { name }
    }

    pub fn render<'decl>(
        &self,
        declarations: Vec<RenderedDeclaration<'decl, Wasm32>>,
    ) -> Result<GeneratedOutput> {
        let preamble = "// Generated by boltffi (target: dart_web). Do not edit by hand.\n\
                         import 'dart:js_interop';\n\
                         import 'dart:js_interop_unsafe';\n\
                         import 'dart:typed_data';\n\n"
            .to_owned();
        FileLayout::new()
            .with_file(
                FilePlan::all(FilePath::new(format!("{}.dart", self.name))?)
                    .with_preamble(preamble),
            )
            .assemble_declarations(declarations)
    }
}
