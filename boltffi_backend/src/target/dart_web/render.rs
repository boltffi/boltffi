use boltffi_binding::{
    CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl, CustomTypeDecl, Decl, DefaultValue,
    DirectValueType, DirectVectorElementType, Direction, EnumDecl, ExecutionDecl, ExportedCallable,
    FunctionDecl, HandlePresence, ImportedCallable, ParamDirection, ParamPlan, Primitive,
    RecordDecl, ReturnPlan, StreamDecl, StreamItemPlan, TypeRef, Wasm32,
};

use crate::core::{
    CoverageMode, Diagnostic, Emitted, Error, FileLayout, FilePath, FilePlan, GeneratedOutput,
    RenderContext, RenderedDeclaration, Result,
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

// target::typescript's direct-vector boundary (render/direct_vector.rs) crosses
// non-bool primitives as a typed array (Int32Array, Float64Array, ...), not a
// plain JS array -- dart_web doesn't yet emit typed-array conversions, so only
// bool (which TS itself represents as a plain boolean array) is supported here.
fn direct_vector_type(element: &DirectVectorElementType) -> Result<TypeRef> {
    match element {
        DirectVectorElementType::Primitive(primitive)
            if primitive.primitive() == Primitive::Bool =>
        {
            Ok(TypeRef::Sequence(Box::new(TypeRef::Primitive(
                Primitive::Bool,
            ))))
        }
        DirectVectorElementType::Primitive(_) => {
            Err(unsupported("direct vector of non-bool primitives"))
        }
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
        ParamPlan::Handle {
            target, presence, ..
        } => {
            let ty = apply_handle_presence(handle_type_ref(target)?, *presence);
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

// An `Option<Class>`/`Option<&Class>` boundary crosses as a JS handle that
// may be `null`, so its Dart type must stay nullable instead of assuming
// every handle is present.
fn apply_handle_presence(ty: TypeRef, presence: HandlePresence) -> TypeRef {
    match presence {
        HandlePresence::Nullable => TypeRef::Optional(Box::new(ty)),
        _ => ty,
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
    // Only ever set by callback_method_signature, for a callback method
    // whose Rust trait method returns `Result<T, E>`. Dart calls into
    // Rust (functions/class methods) go through call_signature instead,
    // which never populates this.
    error_ty: Option<TypeRef>,
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

    // Only for an outbound call (function/initializer/method calling into
    // Rust) -- callback_method_signature also builds a CallSignature, for
    // the opposite direction (Rust calling into Dart), which has no
    // cancellation concept.
    fn dart_params_decl_with_cancellation(&self) -> String {
        let positional = self.dart_params_decl();
        if !self.asynchronous {
            return positional;
        }
        if positional.is_empty() {
            "{ $$BoltCancellationToken? cancellationToken }".to_owned()
        } else {
            format!("{positional}, {{ $$BoltCancellationToken? cancellationToken }}")
        }
    }

    // The shared wasm/JS bridge's async calls take a trailing
    // `(options?: { signal?: AbortSignal }, __boltffiCancelId?: number)` --
    // dart_web never has a `signal` to offer (constructing a real JS
    // AbortController from Dart would cost a JS interop round trip on every
    // call), so `options` is always `null` here and cancellation instead
    // goes through the plain int `__boltffiCallId` this call registers
    // below, free to pass unlike a JS object.
    fn js_call_arguments_with_cancellation(&self) -> String {
        let arguments = self.js_call_arguments();
        if !self.asynchronous {
            return arguments;
        }
        if arguments.is_empty() {
            "null, __boltffiCallId?.toJS".to_owned()
        } else {
            format!("{arguments}, null, __boltffiCallId?.toJS")
        }
    }

    // Wraps `body_statement` (a single already-cancelled-checked statement,
    // e.g. `return decoded;` or `awaited;`) with the already-cancelled
    // pre-check and the register/unregister bracket around the call,
    // matching native Dart's own token semantics. Returns the statements to
    // place inside the function's own braces, not a nested block.
    fn wrap_cancellable_async_call(&self, body_statement: &str) -> String {
        format!(
            "if (cancellationToken?.isCancelled ?? false) {{\n    \
             throw const $$BoltCancelledException();\n  \
             }}\n  \
             final __boltffiCallId = cancellationToken?._registerCall();\n  \
             try {{\n    {body_statement}\n  }} finally {{\n    \
             if (__boltffiCallId != null) cancellationToken!._unregisterCall(__boltffiCallId);\n  \
             }}"
        )
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
        error_ty: None,
        asynchronous,
    })
}

fn closure_param_info(
    closure: &boltffi_binding::ClosureParameter<Wasm32, boltffi_binding::IntoRust>,
    dart_name: &str,
    context: &RenderContext<Wasm32>,
) -> Result<ParamInfo> {
    // An `Option<Box<dyn Fn(...)>>` parameter's `None` case has nowhere to
    // go: the generated Dart type stays non-nullable and every value gets
    // wrapped/called unconditionally, so a caller can never actually pass
    // an absent closure through.
    if matches!(closure.presence(), HandlePresence::Nullable) {
        return Err(unsupported("nullable closure parameter"));
    }
    let inner = callback_method_signature(closure.invoke(), context)?;
    // wrap_dart_callable_as_js_function has no WireResult encoding (that's
    // only implemented for the Callback interface adapter/wrapper below)
    // -- a fallible bare closure would have its thrown Dart exception
    // escape `.toJS` unencoded instead of reaching Rust as a decodable
    // error.
    if inner.error_ty.is_some() {
        return Err(unsupported("fallible closure parameter"));
    }
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
    // target::typescript's wasm callback adapter expects the JS callback to
    // return either a plain success value or a tagged WireResult
    // (`{tag: 'ok', value}` / `{tag: 'err', error}`, see
    // runtime/typescript/src/wire.ts's wireOk/wireErr) -- Callback's own
    // adapter/wrapper rendering builds that shape from a thrown/caught
    // Dart exception, matching target::dart's own error-catch-binding
    // convention (only String, Record, and Enum error payloads are
    // supported there, so the same restriction applies here).
    let error_ty = match callable.error() {
        boltffi_binding::ErrorDecl::None(_) => None,
        boltffi_binding::ErrorDecl::EncodedViaReturnSlot {
            ty: ty @ (TypeRef::String | TypeRef::Record(_) | TypeRef::Enum(_)),
            ..
        } => Some(ty.clone()),
        _ => return Err(unsupported("callback method error channel")),
    };
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
        error_ty,
        asynchronous,
    })
}

// The Dart exception type a callback method's error is caught/thrown as.
// A `String` error can't itself `implements Exception` (it's a builtin,
// not a declared type), so it's wrapped in BoltFFIStringException instead
// -- matching target::dart's own `$$BoltException` convention for the
// same case.
fn error_exception_type(ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    match ty {
        TypeRef::String => Ok("BoltFFIStringException".to_owned()),
        TypeRef::Record(_) | TypeRef::Enum(_) => interop::dart_type(ty, context),
        _ => Err(unsupported("callback error payload type")),
    }
}

// Builds the expression thrown on the Dart-implementation side once the
// wire error value (already decoded to `decoded_expr`) is known.
fn error_throw_expression(ty: &TypeRef, decoded_expr: &str) -> Result<String> {
    match ty {
        TypeRef::String => Ok(format!("BoltFFIStringException({decoded_expr})")),
        TypeRef::Record(_) | TypeRef::Enum(_) => Ok(decoded_expr.to_owned()),
        _ => Err(unsupported("callback error payload type")),
    }
}

// The expression that recovers the wire-encodable error value from a
// caught Dart exception of `error_exception_type(ty, ..)`.
fn error_caught_value(ty: &TypeRef, caught_expr: &str) -> String {
    match ty {
        TypeRef::String => format!("{caught_expr}.message"),
        _ => caught_expr.to_owned(),
    }
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
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => {
            let ty = apply_handle_presence(handle_type_ref(target)?, *presence);
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
                .then(|| ["JSAny? options".to_owned(), "JSAny? cancelId".to_owned()])
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
    let awaited_expr = if signature.asynchronous {
        format!("(await ({call_expr}).toDart)")
    } else {
        call_expr
    };

    let body_statement = if signature.return_ty.is_none() {
        format!("{awaited_expr};")
    } else {
        let decoded = signature.decode_return(&awaited_expr, context)?;
        format!("return {decoded};")
    };
    if signature.asynchronous {
        out.push_str(&format!(
            "  {}\n}}\n\n",
            signature.wrap_cancellable_async_call(&body_statement)
        ));
    } else {
        out.push_str(&format!("  {body_statement}\n}}\n\n"));
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
        // Records render as plain data classes with no binding to a JS
        // object of their own (only the encoded fields cross the
        // boundary), so an inherent initializer/method exported on the
        // record's impl block has nowhere to be called from.
        if !decl.initializers().is_empty() || !decl.methods().is_empty() {
            return Err(unsupported("record with inherent initializers/methods"));
        }
        let name = Name::new(decl.name()).dart_type_name();
        let fields: Vec<DataField> = match decl {
            RecordDecl::Direct(record) => record
                .fields()
                .iter()
                .map(|field| {
                    let ty = TypeRef::Primitive(field.ty().primitive());
                    DataField::new(field.key(), ty)
                })
                .collect(),
            RecordDecl::Encoded(record) => record
                .fields()
                .iter()
                .map(|field| DataField::new(field.key(), field.ty().clone()))
                .collect(),
            _ => return Err(unsupported("record declaration")),
        };

        let source = render_data_class(&name, None, &fields, decl.is_error_payload(), context)?;
        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

// The Dart field identifier and the JS wire property key are separate
// namespaces with separate escaping rules (target::dart's keyword list
// vs. target::typescript's -- e.g. a field named `extension` needs Dart
// escaping but not JS, while `import` needs JS escaping but not Dart),
// so a field carries both instead of one name serving both purposes.
struct DataField {
    dart_name: String,
    wire_key: String,
    ty: TypeRef,
}

impl DataField {
    fn new(key: &boltffi_binding::FieldKey, ty: TypeRef) -> Self {
        Self {
            dart_name: field_dart_name(key),
            wire_key: field_wire_key(key),
            ty,
        }
    }
}

fn render_data_class(
    name: &str,
    extends: Option<(&str, &str)>,
    fields: &[DataField],
    implements_exception: bool,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let mut field_decls = Vec::new();
    let mut ctor_params = Vec::new();
    let mut to_js_entries = Vec::new();
    let mut from_js_args = Vec::new();

    if let Some((_, tag)) = extends {
        to_js_entries.push(format!("    result.setProperty('tag'.toJS, '{tag}'.toJS);"));
    }

    // An enum variant class is immutable (`final` fields, `const`
    // constructor) matching target::dart's own data-enum variant classes;
    // a plain record is mutable (no `final`, no `const`) matching
    // target::dart's own Record -- app code that assigns to a record field
    // after construction must keep working on the web half too.
    let immutable = extends.is_some();
    let field_keyword = if immutable { "final " } else { "" };
    let ctor_keyword = if immutable { "const " } else { "" };

    for field in fields {
        let DataField {
            dart_name,
            wire_key,
            ty,
        } = field;
        let dart_type = interop::dart_type(ty, context)?;
        field_decls.push(format!("  {field_keyword}{dart_type} {dart_name};"));
        ctor_params.push(format!("required this.{dart_name}"));
        let to_js = interop::to_js(dart_name, ty, context)?;
        to_js_entries.push(format!(
            "    result.setProperty('{wire_key}'.toJS, {to_js});"
        ));
        let from_js = interop::from_js(&format!("js.getProperty('{wire_key}'.toJS)"), ty, context)?;
        from_js_args.push(format!("{dart_name}: {from_js}"));
    }

    let (header, override_kw, extra_ctor) = match extends {
        Some((base, _)) => (
            format!("class {name} extends {base}"),
            "@override\n  ",
            " : super._()".to_owned(),
        ),
        None => {
            // A data-enum variant class inherits "implements Exception"
            // through `extends` instead of declaring it again here.
            let exception_clause = if implements_exception {
                " implements Exception"
            } else {
                ""
            };
            (format!("class {name}{exception_clause}"), "", String::new())
        }
    };
    let ctor_params_decl = if ctor_params.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", ctor_params.join(", "))
    };

    Ok(format!(
        "{header} {{\n{fields}\n\n  {ctor_keyword}{name}({ctor_params_decl}){extra_ctor};\n\n  {override_kw}JSObject toJS() {{\n    final result = JSObject();\n{to_js}\n    return result;\n  }}\n\n  static {name} fromJS(JSObject js) {{\n    return {name}({from_js});\n  }}\n}}\n\n",
        fields = field_decls.join("\n"),
        to_js = to_js_entries.join("\n"),
        from_js = from_js_args.join(", "),
    ))
}

fn field_dart_name(key: &boltffi_binding::FieldKey) -> String {
    match key {
        boltffi_binding::FieldKey::Named(name) => Name::new(name).dart_identifier(),
        boltffi_binding::FieldKey::Position(position) => format!("value{position}"),
        _ => "field".to_owned(),
    }
}

// Must match target::typescript's PropertyKey::from_field/Display exactly,
// or these bindings read/write a JS object property the wire side never
// uses.
fn field_wire_key(key: &boltffi_binding::FieldKey) -> String {
    match key {
        boltffi_binding::FieldKey::Named(name) => Name::new(name).js_export_name(),
        boltffi_binding::FieldKey::Position(position) => format!("value{position}"),
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
        // Enums render as either a wrapped int (C-style) or a sealed class
        // hierarchy over plain data classes (data enums) -- neither has a
        // binding to a JS object of its own, so an inherent
        // initializer/method exported on the enum's impl block has
        // nowhere to be called from (same reasoning as Record).
        let (initializers, methods) = match decl {
            EnumDecl::CStyle(cstyle) => (cstyle.initializers(), cstyle.methods()),
            EnumDecl::Data(data) => (data.initializers(), data.methods()),
            _ => return Err(unsupported("enum declaration")),
        };
        if !initializers.is_empty() || !methods.is_empty() {
            return Err(unsupported("enum with inherent initializers/methods"));
        }
        let source = match decl {
            EnumDecl::CStyle(cstyle) => Self::c_style(cstyle)?,
            EnumDecl::Data(data) => Self::data(data, context)?,
            _ => return Err(unsupported("enum declaration")),
        };
        Ok(Self { source })
    }

    // A real Dart `enum` (not a hand-rolled wrapped-int class), with the
    // same lowerCamelCase variant names target::dart's native C-style enum
    // uses -- the unified package's whole premise is that app code (switch
    // patterns, `.name`, equality) sees one Dart API regardless of which
    // half (native or web) it's actually running against.
    fn c_style(decl: &boltffi_binding::CStyleEnumDecl<Wasm32>) -> Result<String> {
        let name = Name::new(decl.name()).dart_type_name();
        let variant_entries = decl
            .variants()
            .iter()
            .map(|variant| {
                let variant_name = Name::new(variant.name()).dart_identifier();
                let value = variant.discriminant().get();
                format!("  {variant_name}({value})")
            })
            .collect::<Vec<_>>()
            .join(",\n");

        let exception_clause = if decl.is_error_payload() {
            " implements Exception"
        } else {
            ""
        };
        Ok(format!(
            "enum {name}{exception_clause} {{\n{variant_entries};\n\n  final int value;\n  const {name}(this.value);\n\n  JSAny toJS() => value.toJS;\n\n  static {name} fromJS(JSAny js) => _fromRaw((js as JSNumber).toDartInt);\n\n  static {name} _fromRaw(int value) => values.firstWhere(\n    (variant) => variant.value == value,\n    orElse: () => throw ArgumentError.value(value, 'value', 'unknown {name} value'),\n  );\n}}\n\n",
        ))
    }

    // A `sealed class` with a named factory constructor per variant --
    // matching target::dart's native data-enum shape exactly (`sealed`,
    // not `abstract`, plus `factory Name.variantName({required fields}) =
    // VariantClass;`) -- so app code written against the native half's
    // pattern-matching/construction API works unchanged here.
    fn data(
        decl: &boltffi_binding::DataEnumDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        let name = Name::new(decl.name()).dart_type_name();
        let mut variant_classes = Vec::new();
        let mut from_js_cases = Vec::new();
        let mut factory_constructors = Vec::new();

        for variant in decl.variants() {
            let variant_name = Name::new(variant.name()).dart_identifier();
            let variant_dart_name = Name::new(variant.name()).dart_type_name();
            // Must match target::typescript's wire tag spelling exactly.
            let tag = variant_dart_name.clone();
            let variant_type = format!("{name}${variant_dart_name}");
            let fields: Vec<DataField> = variant
                .payload()
                .fields()
                .iter()
                .map(|field| DataField::new(field.key(), field.ty().clone()))
                .collect();

            variant_classes.push(render_data_class(
                &variant_type,
                Some((&name, &tag)),
                &fields,
                false,
                context,
            )?);

            let from_js_args = fields
                .iter()
                .map(|field| {
                    let from_js = interop::from_js(
                        &format!("js.getProperty('{}'.toJS)", field.wire_key),
                        &field.ty,
                        context,
                    )?;
                    Ok(format!("{}: {from_js}", field.dart_name))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ");

            from_js_cases.push(format!(
                "      case '{tag}': return {variant_type}({from_js_args});"
            ));

            let params = if fields.is_empty() {
                String::new()
            } else {
                let required = fields
                    .iter()
                    .map(|field| {
                        let dart_type = interop::dart_type(&field.ty, context)?;
                        Ok(format!("required {dart_type} {}", field.dart_name))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                format!("{{{required}}}")
            };
            // `const factory` (not just `factory`) -- the redirect target
            // is a const constructor (see render_data_class's `immutable`
            // branch), and target::dart's own enumeration.dart template
            // marks its redirecting factories const too, so app code that
            // does `const Enum.variant(...)` must keep compiling here.
            factory_constructors.push(format!(
                "  const factory {name}.{variant_name}({params}) = {variant_type};"
            ));
        }

        let exception_clause = if decl.is_error_payload() {
            " implements Exception"
        } else {
            ""
        };
        Ok(format!(
            "sealed class {name}{exception_clause} {{\n  const {name}._();\n\n{factories}\n\n  JSObject toJS();\n\n  static {name} fromJS(JSObject js) {{\n    final tag = (js.getProperty('tag'.toJS) as JSString).toDart;\n    switch (tag) {{\n{cases}\n      default: throw StateError('Unknown {name} tag: \\$tag');\n    }}\n  }}\n}}\n\n{variants}",
            factories = factory_constructors.join("\n"),
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
                // Wraps the Dart implementation's success/thrown-exception
                // outcome as the `{tag: 'ok'|'err', ...}` wire shape
                // target::typescript's wasm callback adapter expects (see
                // runtime/typescript/src/wire.ts's WireResult) -- Rust
                // decodes this back into its own `Result<T, E>` on the
                // other side of the boundary.
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
            let raw_call = format!("_js.callMethodVarArgs('{js_name}'.toJS, [{js_arguments}])");
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
                    } else {
                        let decoded = signature.decode_return(&raw_result, context)?;
                        format!("{{ return {decoded}; }}")
                    }
                }
                // Unwraps the same `{tag: 'ok'|'err', ...}` wire shape the
                // adapter above builds, for the case where a raw JS object
                // (not a Dart implementation routed through the adapter)
                // is speaking the wire contract directly.
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
            wrapper_methods.push(format!(
                "  @override\n  {public_return} {method_name}({dart_params}) {wrapper_async}{wrapper_body}"
            ));
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
            return Err(unsupported("associated constant"));
        }
        // Matches target::dart's own Constant::from_declaration: lowerCamel
        // name (not SCREAMING_SNAKE_CASE) and a `const` (not `final`)
        // declaration.
        let name = Name::new(decl.name()).dart_constant_name();
        let source = match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                let dart_type = interop::dart_type(ty, context)?;
                let literal = render_default_value(value, context)?;
                format!("const {dart_type} {name} = {literal};\n\n")
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

// `{:?}` on f64::NAN/INFINITY/NEG_INFINITY produces "NaN"/"inf"/"-inf",
// none of which are valid Dart expressions -- Dart spells these
// double.nan/double.infinity/double.negativeInfinity.
fn render_float_literal(value: f64) -> String {
    if value.is_nan() {
        "double.nan".to_owned()
    } else if value == f64::INFINITY {
        "double.infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "double.negativeInfinity".to_owned()
    } else {
        format!("{value:?}")
    }
}

fn render_default_value(value: &DefaultValue, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match value {
        DefaultValue::Bool(value) => value.to_string(),
        DefaultValue::Integer(value) => value.get().to_string(),
        DefaultValue::Float(value) => render_float_literal(value.to_f64()),
        DefaultValue::String(value) => super::syntax::dart_string_literal(value),
        DefaultValue::EnumVariant {
            enum_name,
            variant_name,
        } => {
            let enum_dart_name = Name::new(enum_name).dart_type_name();
            // Data enums render each unit variant as its own subclass
            // (`Enum$Variant`), not a static member on `Enum` itself.
            let is_data_enum = context
                .bindings()
                .decls()
                .iter()
                .find_map(|decl| match decl {
                    Decl::Enum(enumeration) if enumeration.name() == enum_name => {
                        Some(matches!(enumeration.as_ref(), EnumDecl::Data(_)))
                    }
                    _ => None,
                })
                .ok_or_else(|| unsupported("enum default referencing an unknown enum"))?;
            if is_data_enum {
                let variant_dart_name = Name::new(variant_name).dart_type_name();
                format!("{enum_dart_name}${variant_dart_name}()")
            } else {
                // A real Dart enum's values are lowerCamelCase, matching
                // target::dart's own C-style enum variants.
                let variant_dart_name = Name::new(variant_name).dart_identifier();
                format!("{enum_dart_name}.{variant_dart_name}")
            }
        }
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
    diagnostics: Vec<Diagnostic>,
}

// Every declaration `decl.initializers()` yields already returns exactly
// `Self` (that's the IR's own is_initializer test), so a sync initializer
// is always eligible to be *the* unnamed constructor -- matches
// target::dart's Placement::Initializer rule (Factory when
// `!asynchronous`), which is what lets app code call `ClassName(args)` on
// native. An async initializer can't be a Dart constructor (constructors
// can't be `async`), so it keeps rendering as a named static method,
// exactly like target::dart does for its own async initializers.
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
    let call_js = format!("{class_ref}.callMethodVarArgs('{js_name}'.toJS, [{arguments}])");
    Ok(if signature.asynchronous {
        let method_name = Name::new(initializer.name()).dart_identifier();
        let body_statement = format!(
            "return {name}._((await ({call_js} as JSPromise<JSAny?>).toDart) as JSObject);"
        );
        format!(
            "  static Future<{name}> {method_name}({params}) async {{\n    {}\n  }}",
            signature.wrap_cancellable_async_call(&body_statement)
        )
    } else {
        format!("  factory {name}({params}) => {name}._({call_js} as JSObject);",)
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
    let call_js = format!("({target}).callMethodVarArgs('{js_name}'.toJS, [{js_arguments}])");
    let keyword = if is_static { "static " } else { "" };
    let async_keyword = if signature.asynchronous { "async " } else { "" };
    let call_expr = if signature.asynchronous {
        format!("(await ({call_js} as JSPromise<JSAny?>).toDart)")
    } else {
        call_js
    };
    if signature.asynchronous {
        let body_statement = if signature.return_ty.is_none() {
            format!("{call_expr};")
        } else {
            let decoded = signature.decode_return(&call_expr, context)?;
            format!("return {decoded};")
        };
        return Ok(format!(
            "  {keyword}{} {method_name}({params}) {async_keyword}{{\n    {}\n  }}",
            signature.dart_return_signature(),
            signature.wrap_cancellable_async_call(&body_statement)
        ));
    }
    let body = if signature.return_ty.is_none() {
        format!("{{ {call_expr}; }}")
    } else {
        let decoded = signature.decode_return(&call_expr, context)?;
        format!("=> {decoded};")
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

        // One unsupported member (e.g. a Vec<i32> direct-vector parameter)
        // must not drop the whole class -- matches target::typescript's
        // Class::from_declaration, which keeps every other successfully
        // rendered initializer/method and records a diagnostic instead.
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
        let members = members.join("\n");

        let source = format!(
            "@JS('{namespace}.{name}')\nexternal JSObject get {class_ref};\n\n\
             class {name} {{\n  final JSObject js;\n  const {name}._(this.js);\n\n  static {name} fromJS(JSObject js) => {name}._(js);\n\n\
             \x20\x20// Releases the underlying Rust handle. The JS wrapper (BoltFFIHandle)\n\
             \x20\x20// also finalizes it automatically if this is never called, but that's\n\
             \x20\x20// nondeterministic GC timing -- call this to release deterministically.\n\
             \x20\x20void dispose$() {{\n\
             \x20\x20\x20\x20js.callMethodVarArgs('dispose'.toJS, []);\n\
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

        let extern_decl = match decl.owner() {
            Some(_) => String::new(),
            None => {
                let extern_name = format!("_boltffiExtern_{js_name}");
                format!(
                    "@JS('{namespace}.{js_name}')\nexternal JSObject {extern_name}([JSAny? callback]);\n\n"
                )
            }
        };

        // An owned stream is reached as a method on the instance's JS
        // object (like any other method); a free stream's extern binds
        // directly to the wrapped module's function, so it's called, not
        // looked up as a property on something else.
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
            session_or_cancellable_expr
        } else {
            format!(
                "({session_or_cancellable_expr}).callMethodVarArgs('consume'.toJS, [{js_item_callback}]) as JSObject"
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
             \x20\x20\x20\x20\x20\x20\x20\x20.then((_) {{ __boltffiController.close(); }}, onError: (Object error, StackTrace stackTrace) {{\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20__boltffiController.addError(error, stackTrace);\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20__boltffiController.close();\n\
             \x20\x20\x20\x20\x20\x20}});\n\
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
        let namespace = self.namespace;
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
             Future<void> boltffiInit() => _boltffiReady.toDart.then((_) {{}});\n\n\
             @JS('{namespace}.__boltffiCancelById')\n\
             external void _boltffiCancelById(JSAny? callId);\n\n\
             // dart:js_interop's JSBigInt has no int/BigInt conversion members
             // (no BigInt.toJS, no JSBigInt.toDartInt) -- round-trip through the
             // JS BigInt constructor and its decimal string representation instead.\n\
             JSAny boltffiInt64ToJS(int value) {{\n\
             \x20\x20return (globalContext.getProperty('BigInt'.toJS) as JSFunction)\n\
             \x20\x20\x20\x20\x20\x20.callAsConstructor<JSAny>(BigInt.from(value).toString().toJS);\n\
             }}\n\n\
             int boltffiInt64FromJS(JSAny value) {{\n\
             \x20\x20final text = (value as JSObject).callMethodVarArgs('toString'.toJS, []) as JSString;\n\
             \x20\x20return BigInt.parse(text.toDart).toInt();\n\
             }}\n\n\
             // The wire format for `std::time::Duration` is `{{ secs: bigint,\n\
             // nanos: number }}` (see runtime/typescript/src/wire.ts).\n\
             JSObject boltffiDurationToJS(Duration value) {{\n\
             \x20\x20final result = JSObject();\n\
             \x20\x20final wholeSeconds = value.inSeconds;\n\
             \x20\x20final remainderMicros = value.inMicroseconds - wholeSeconds * 1000000;\n\
             \x20\x20result.setProperty('secs'.toJS, boltffiInt64ToJS(wholeSeconds));\n\
             \x20\x20result.setProperty('nanos'.toJS, (remainderMicros * 1000).toJS);\n\
             \x20\x20return result;\n\
             }}\n\n\
             Duration boltffiDurationFromJS(JSObject value) {{\n\
             \x20\x20final secs = boltffiInt64FromJS(value.getProperty('secs'.toJS) as JSAny);\n\
             \x20\x20final nanos = (value.getProperty('nanos'.toJS) as JSNumber).toDartInt;\n\
             \x20\x20return Duration(seconds: secs, microseconds: nanos ~/ 1000);\n\
             }}\n\n\
             // A Rust `Result<T, E>` crossing through a callback method
             // wraps its outcome as `{{tag: 'ok', value}}` /
             // `{{tag: 'err', error}}` (see runtime/typescript/src/wire.ts's
             // wireOk/wireErr) -- the callback adapter builds this from a
             // thrown/caught Dart exception, and the JsWrapper escape hatch
             // unwraps it back into one.\n\
             JSObject boltffiWireOk(JSAny? value) {{\n\
             \x20\x20final result = JSObject();\n\
             \x20\x20result.setProperty('tag'.toJS, 'ok'.toJS);\n\
             \x20\x20result.setProperty('value'.toJS, value);\n\
             \x20\x20return result;\n\
             }}\n\n\
             JSObject boltffiWireErr(JSAny? error) {{\n\
             \x20\x20final result = JSObject();\n\
             \x20\x20result.setProperty('tag'.toJS, 'err'.toJS);\n\
             \x20\x20result.setProperty('error'.toJS, error);\n\
             \x20\x20return result;\n\
             }}\n\n\
             // A `String` Rust error can't itself `implements Exception`
             // (it's a builtin, not a declared type), so a fallible
             // callback method with a String error throws/catches this
             // wrapper instead -- matches target::dart's own
             // `$$BoltException` convention for the same case.\n\
             class BoltFFIStringException implements Exception {{\n\
             \x20\x20final String message;\n\
             \x20\x20const BoltFFIStringException(this.message);\n\n\
             \x20\x20@override\n\
             \x20\x20String toString() => message;\n\
             }}\n\n\
             // Named to match target::dart's own `$$BoltCancellationToken`.\n\
             // Tracks plain ints instead of wrapping a real JS\n\
             // AbortController -- constructing one from Dart would cost a JS\n\
             // interop round trip on every call, unlike a bare int.\n\
             final class $$BoltCancellationToken {{\n\
             \x20\x20static int _nextCallId = 0;\n\n\
             \x20\x20bool _isCancelled = false;\n\
             \x20\x20final Set<int> _activeCallIds = {{}};\n\n\
             \x20\x20bool get isCancelled => _isCancelled;\n\n\
             \x20\x20void cancel() {{\n\
             \x20\x20\x20\x20if (_isCancelled) return;\n\
             \x20\x20\x20\x20_isCancelled = true;\n\
             \x20\x20\x20\x20for (final id in _activeCallIds) {{\n\
             \x20\x20\x20\x20\x20\x20_boltffiCancelById(id.toJS);\n\
             \x20\x20\x20\x20}}\n\
             \x20\x20\x20\x20_activeCallIds.clear();\n\
             \x20\x20}}\n\n\
             \x20\x20int _registerCall() {{\n\
             \x20\x20\x20\x20final id = _nextCallId++;\n\
             \x20\x20\x20\x20_activeCallIds.add(id);\n\
             \x20\x20\x20\x20return id;\n\
             \x20\x20}}\n\n\
             \x20\x20void _unregisterCall(int id) {{\n\
             \x20\x20\x20\x20_activeCallIds.remove(id);\n\
             \x20\x20}}\n\
             }}\n\n\
             final class $$BoltCancelledException implements Exception {{\n\
             \x20\x20const $$BoltCancelledException();\n\n\
             \x20\x20@override\n\
             \x20\x20String toString() => 'BoltFFI call was cancelled';\n\
             }}\n\n"
        );
        FileLayout::new()
            .with_file(
                FilePlan::all(FilePath::new(format!("{}.dart", self.name))?)
                    .with_preamble(preamble),
            )
            .assemble_declarations(declarations)
    }
}

#[cfg(test)]
mod tests {
    use super::render_float_literal;

    // Rust has no NaN/infinity float *literal* syntax (only the associated
    // consts f64::NAN/INFINITY, which the classifier treats as accessor
    // constants, not inline defaults), so these values aren't currently
    // reachable through a #[export] const default -- this is a direct unit
    // test of the pure formatting logic rather than an end-to-end fixture.
    #[test]
    fn renders_finite_floats_with_debug_formatting() {
        assert_eq!(render_float_literal(1.5), "1.5");
        assert_eq!(render_float_literal(0.0), "0.0");
    }

    #[test]
    fn renders_nan_and_infinity_as_dart_double_constants() {
        assert_eq!(render_float_literal(f64::NAN), "double.nan");
        assert_eq!(render_float_literal(f64::INFINITY), "double.infinity");
        assert_eq!(
            render_float_literal(f64::NEG_INFINITY),
            "double.negativeInfinity"
        );
    }
}
