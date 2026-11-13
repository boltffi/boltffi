use super::*;

struct Boundary {
    dart_type: String,
    ty: TypeRef,
}

pub(super) fn web_default(
    ty: &TypeRef,
    value: &DefaultValue,
    context: &RenderContext<Wasm32>,
) -> Result<crate::target::dart::default_value::DefaultExpression> {
    use crate::core::default_value::{Field, Representation};
    use crate::target::dart::{
        default_value::DefaultExpression,
        syntax::{Expression, Literal},
    };
    match ty {
        TypeRef::Optional(inner) if !matches!(value, DefaultValue::Null) => {
            return match web_default(inner, value, context)? {
                value @ DefaultExpression::Constant(_) => Ok(value),
                _ => Err(unsupported("optional constructed default")),
            };
        }
        TypeRef::Custom(id) => {
            return match Representation::resolve(*id, context)? {
                Representation::Transparent(ty) => web_default(ty, value, context),
                Representation::Record(record) => {
                    let ty = match record.field() {
                        Field::Direct(field) => TypeRef::Primitive(field.ty().primitive()),
                        Field::Encoded(field) => field.ty().clone(),
                    };
                    let value = match web_default(&ty, value, context)? {
                        DefaultExpression::Constant(value) => value.to_string(),
                        DefaultExpression::Runtime(value) => value.to_string(),
                    };
                    Ok(DefaultExpression::Runtime(Expression::new(format!(
                        "{}({}: {value})",
                        Name::new(record.name()).dart_type_name(),
                        field_dart_name(record.field().key())
                    ))))
                }
            };
        }
        _ => {}
    }
    if let (TypeRef::Builtin(boltffi_binding::BuiltinType::Uuid), DefaultValue::String(value)) =
        (ty, value)
    {
        let uuid = crate::core::default_value::UuidLiteral::parse(value)
            .ok_or_else(|| unsupported("invalid UUID default"))?;
        let hex = format!("{:016x}{:016x}", uuid.high_bits(), uuid.low_bits());
        let text = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        return Ok(DefaultExpression::Constant(Literal::new(format!(
            "const $$BoltUUIDValue._text('{text}')"
        ))));
    }
    if let DefaultValue::Integer(value) = value
        && (value.get() < -9007199254740991 || value.get() > 9007199254740991)
    {
        return Err(unsupported(
            "integer default outside the exact web integer range",
        ));
    }
    DefaultExpression::render(ty, value, context).map_err(|error| match error {
        Error::UnsupportedTarget { shape, .. } => unsupported(shape),
        error => error,
    })
}

pub(super) fn direct_primitive(ty: &DirectValueType) -> Result<TypeRef> {
    match ty {
        DirectValueType::Primitive(primitive) => Ok(TypeRef::Primitive(*primitive)),
        DirectValueType::Record(id) => Ok(TypeRef::Record(*id)),
        DirectValueType::Enum(id) => Ok(TypeRef::Enum(*id)),
        _ => Err(unsupported("direct value type")),
    }
}

fn direct_vector_type(element: &DirectVectorElementType) -> Result<TypeRef> {
    let ty = match element {
        DirectVectorElementType::Primitive(primitive) => TypeRef::Primitive(primitive.primitive()),
        DirectVectorElementType::Record(id) => TypeRef::Record(*id),
        _ => return Err(unsupported("direct vector element type")),
    };
    Ok(TypeRef::Sequence(Box::new(ty)))
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

fn apply_handle_presence(ty: TypeRef, presence: HandlePresence) -> TypeRef {
    match presence {
        HandlePresence::Nullable => TypeRef::Optional(Box::new(ty)),
        _ => ty,
    }
}

pub(super) struct ParamInfo {
    pub(super) name: String,
    pub(super) default: Option<String>,
    pub(super) setup: Option<String>,
    pub(super) writeback: Option<String>,
    pub(super) dart_type: String,
    pub(super) js_call_expr: String,
    pub(super) value_ty: Option<TypeRef>,
}

pub(super) struct CallSignature {
    pub(super) cancellation_name: String,
    pub(super) params: Vec<ParamInfo>,
    pub(super) return_dart_type: String,
    pub(super) return_ty: Option<TypeRef>,
    pub(super) error_ty: Option<TypeRef>,
    pub(super) asynchronous: bool,
}

impl CallSignature {
    pub(super) fn parameter_declarations(&self, cancellation: bool) -> String {
        let positional = self
            .params
            .iter()
            .position(|p| p.default.is_some())
            .unwrap_or(self.params.len());
        let mut parts = self.params[..positional]
            .iter()
            .map(|p| format!("{} {}", p.dart_type, p.name))
            .collect::<Vec<_>>();
        let mut named = self.params[positional..]
            .iter()
            .map(|p| match &p.default {
                Some(value) => format!("{} {} = {value}", p.dart_type, p.name),
                None => format!("required {} {}", p.dart_type, p.name),
            })
            .collect::<Vec<_>>();
        if cancellation {
            named.push(format!(
                "$$BoltCancellationToken? {}",
                self.cancellation_name
            ));
        }
        if !named.is_empty() {
            parts.push(format!("{{ {} }}", named.join(", ")));
        }
        parts.join(", ")
    }

    pub(super) fn dart_params_decl(&self) -> String {
        self.parameter_declarations(false)
    }

    pub(super) fn call_body(&self, body: &str) -> String {
        let setup = self
            .params
            .iter()
            .filter_map(|p| p.setup.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        let writeback = self
            .params
            .iter()
            .filter_map(|p| p.writeback.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        if writeback.is_empty() {
            return format!("{setup}\n{body}");
        }
        format!("{setup}\ntry {{\n{body}\n}} finally {{\n{writeback}\n}}")
    }

    pub(super) fn invoke(
        &self,
        direct: &str,
        reference: &str,
        receiver: &str,
        arguments: &str,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        if !self.asynchronous && self.error_ty.is_none() {
            return Ok(direct.to_owned());
        }
        let decoder = match &self.error_ty {
            None => "null".to_owned(),
            Some(ty) => {
                let exception = error_exception_type(ty, context)?;
                let name = if matches!(ty, TypeRef::String) { "Error".to_owned() } else { format!("{exception}Exception") };
                let property = if matches!(ty, TypeRef::String) { "message" } else { "value" };
                let value = interop::from_js(&format!("error.getProperty('{property}'.toJS)"), ty, context)?;
                let value = if matches!(ty, TypeRef::String) { format!("$$BoltException({value})") } else { value };
                format!("(error) => (error.getProperty('name'.toJS) as JSString?).toDart == '{name}' ? {value} : error")
            }
        }.replace("as JSString?).toDart", "as JSString?)?.toDart");
        let result = if self.asynchronous {
            format!("await _boltffiCaptureAsync({direct} as JSPromise<JSAny?>).toDart")
        } else {
            format!("_boltffiCapture({reference}, {receiver}, <JSAny?>[{arguments}].toJS)")
        };
        let decoded = format!("_boltffiUnwrap({result}, {decoder})");
        if self.asynchronous && self.error_ty.is_none() {
            return Ok(format!(
                "{} == null ? await ({direct} as JSPromise<JSAny?>).toDart : {decoded}",
                self.cancellation_name
            ));
        }
        Ok(decoded)
    }

    pub(super) fn dart_arguments(&self, arguments: &[String]) -> String {
        let positional = self
            .params
            .iter()
            .position(|p| p.default.is_some())
            .unwrap_or(self.params.len());
        arguments
            .iter()
            .enumerate()
            .map(|(i, value)| {
                if i < positional {
                    value.clone()
                } else {
                    format!("{}: {value}", self.params[i].name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(super) fn js_call_arguments(&self) -> String {
        self.params
            .iter()
            .map(|param| param.js_call_expr.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(super) fn dart_return_signature(&self) -> String {
        if self.asynchronous {
            format!("Future<{}>", self.return_dart_type)
        } else {
            self.return_dart_type.clone()
        }
    }

    pub(super) fn decode_return(
        &self,
        raw_expr: &str,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        match &self.return_ty {
            None => Ok(String::new()),
            Some(ty) => interop::from_js(raw_expr, ty, context),
        }
    }

    pub(super) fn encode_return(
        &self,
        dart_expr: &str,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        match &self.return_ty {
            None => Ok(String::new()),
            Some(ty) => interop::to_js(dart_expr, ty, context),
        }
    }

    pub(super) fn dart_params_decl_with_cancellation(&self) -> String {
        self.parameter_declarations(self.asynchronous)
    }

    // Options remain absent for calls without cancellation.
    pub(super) fn js_call_arguments_with_cancellation(&self) -> String {
        let arguments = self.js_call_arguments();
        if !self.asynchronous {
            return arguments;
        }
        if arguments.is_empty() {
            format!("_boltffiCancellationOptions({})", self.cancellation_name)
        } else {
            format!(
                "{arguments}, _boltffiCancellationOptions({})",
                self.cancellation_name
            )
        }
    }

    pub(super) fn wrap_cancellable_async_call(&self, body_statement: &str) -> String {
        format!(
            "if ({}?.isCancelled ?? false) {{\n    throw const $$BoltCancelledException();\n  }}\n  {body_statement}",
            self.cancellation_name
        )
    }
}
fn apply_parameter_default(
    info: &mut ParamInfo,
    value: Option<&DefaultValue>,
    context: &RenderContext<Wasm32>,
) -> Result<()> {
    if let (Some(value), Some(ty)) = (value, info.value_ty.as_ref()) {
        use crate::target::dart::default_value::DefaultExpression;
        match web_default(ty, value, context)? {
            DefaultExpression::Constant(value) => info.default = Some(value.to_string()),
            DefaultExpression::Runtime(value) => {
                if !info.dart_type.ends_with('?') {
                    info.dart_type.push('?');
                }
                info.default = Some("null".to_owned());
                info.setup = Some(format!("{} ??= {value};", info.name));
            }
        }
    }
    Ok(())
}

pub(super) fn call_signature(
    callable: &ExportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<CallSignature> {
    let mut params = Vec::new();

    for (index, param) in callable.params().iter().enumerate() {
        let dart_name = Name::new(param.name()).dart_identifier();
        match param.payload() {
            boltffi_binding::IncomingParam::Value(plan) => {
                let boundary = boundary_for_param(plan, context)?;
                let js_call_expr = interop::to_js(&dart_name, &boundary.ty, context)?;
                params.push(ParamInfo {
                    name: dart_name.clone(),
                    default: None,
                    setup: None,
                    writeback: None,
                    dart_type: boundary.dart_type,
                    js_call_expr,
                    value_ty: Some(boundary.ty),
                });
            }
            boltffi_binding::IncomingParam::Closure(closure) => {
                params.push(closure_param_info(closure, &dart_name, context)?);
            }
        }
        let info = params.last_mut().unwrap();
        if matches!(
            param.payload(),
            boltffi_binding::IncomingParam::Value(ParamPlan::Encoded {
                receive: boltffi_binding::Receive::ByMutRef,
                ..
            })
        ) {
            return Err(unsupported("mutable encoded parameter"));
        }
        apply_parameter_default(info, param.meta().default(), context)?;
        if let boltffi_binding::IncomingParam::Value(
            ParamPlan::Direct {
                receive: boltffi_binding::Receive::ByMutRef,
                ..
            }
            | ParamPlan::Encoded {
                receive: boltffi_binding::Receive::ByMutRef,
                ..
            }
            | ParamPlan::DirectVec {
                receive: boltffi_binding::Receive::ByMutRef,
                ..
            },
        ) = param.payload()
        {
            let raw = format!("__boltffiArg{index}");
            let conversion = format!("final {raw} = {};", info.js_call_expr);
            info.setup = Some(format!(
                "{}\n{conversion}",
                info.setup.take().unwrap_or_default()
            ));
            info.js_call_expr = raw.clone();
            match info.value_ty.as_ref().unwrap() {
                TypeRef::Record(_)
                    if matches!(callable.execution(), ExecutionDecl::Asynchronous(_)) =>
                {
                    return Err(unsupported("asynchronous mutable record parameter"));
                }
                TypeRef::Record(_) => {
                    info.writeback = Some(format!("{dart_name}._updateFromJS({raw});"))
                }
                TypeRef::Sequence(_) | TypeRef::Bytes => {
                    let decoded = interop::from_js(&raw, info.value_ty.as_ref().unwrap(), context)?;
                    info.writeback = Some(format!("{dart_name}.setAll(0, {decoded});"));
                }
                _ => return Err(unsupported("mutable value parameter")),
            }
        }
    }

    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    let (return_dart_type, return_ty) = return_boundary(callable.returns().plan(), context)?;

    Ok(CallSignature {
        cancellation_name: crate::target::dart::name_style::cancellation_token_name(
            params.iter().map(|p| p.name.as_str()),
        )
        .to_owned(),
        params,
        return_dart_type,
        return_ty,
        error_ty: match callable.error() {
            boltffi_binding::ErrorDecl::EncodedViaReturnSlot { ty, .. }
            | boltffi_binding::ErrorDecl::EncodedViaOutPointer { ty, .. } => Some(ty.clone()),
            boltffi_binding::ErrorDecl::None(_) => None,
            _ => return Err(unsupported("outbound error channel")),
        },
        asynchronous,
    })
}

fn closure_param_info(
    closure: &boltffi_binding::ClosureParameter<Wasm32, boltffi_binding::IntoRust>,
    dart_name: &str,
    context: &RenderContext<Wasm32>,
) -> Result<ParamInfo> {
    if matches!(closure.presence(), HandlePresence::Nullable) {
        return Err(unsupported("nullable closure parameter"));
    }
    let inner = callback_method_signature(closure.invoke(), context)?;
    if inner.error_ty.is_some() {
        return Err(unsupported("fallible closure parameter"));
    }
    let dart_type = format!(
        "{} Function({})",
        inner.dart_return_signature(),
        inner
            .params
            .iter()
            .map(|param| param.dart_type.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let js_call_expr = wrap_dart_callable_as_js_function(dart_name, &inner, context)?;
    Ok(ParamInfo {
        name: dart_name.to_owned(),
        default: None,
        setup: None,
        writeback: None,
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
        .collect::<Result<Vec<_>>>()?;
    let decoded_args = signature.dart_arguments(&decoded_args);
    let call_expr = format!("{dart_callable_expr}({decoded_args})");
    let await_keyword = if signature.asynchronous { "await " } else { "" };
    let body = if signature.return_ty.is_none() {
        format!("{{ {await_keyword}{call_expr}; }}")
    } else {
        let encoded = signature.encode_return("__boltffiResult", context)?;
        format!("{{ final __boltffiResult = {await_keyword}{call_expr}; return {encoded}; }}")
    };
    let body = if signature.asynchronous {
        format!("{{ return (() async {body})().toJS; }}")
    } else {
        body
    };
    Ok(format!("(({js_params}) {body}).toJS"))
}

pub(super) fn callback_method_signature(
    callable: &ImportedCallable<Wasm32>,
    context: &RenderContext<Wasm32>,
) -> Result<CallSignature> {
    let error_ty = match callable.error() {
        boltffi_binding::ErrorDecl::None(_) => None,
        boltffi_binding::ErrorDecl::EncodedViaReturnSlot {
            ty: ty @ (TypeRef::String | TypeRef::Record(_) | TypeRef::Enum(_)),
            ..
        } => Some(ty.clone()),
        _ => return Err(unsupported("callback method error channel")),
    };
    let mut params = Vec::new();

    for param in callable.params() {
        let dart_name = Name::new(param.name()).dart_identifier();
        let value = param
            .payload()
            .as_value()
            .ok_or_else(|| unsupported("closure parameter on a callback/closure body"))?;
        let boundary = boundary_for_param(value, context)?;
        let js_call_expr = interop::to_js(&dart_name, &boundary.ty, context)?;
        params.push(ParamInfo {
            name: dart_name.clone(),
            default: None,
            setup: None,
            writeback: None,
            dart_type: boundary.dart_type,
            js_call_expr,
            value_ty: Some(boundary.ty),
        });
        apply_parameter_default(params.last_mut().unwrap(), param.meta().default(), context)?;
    }

    let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
    let (return_dart_type, return_ty) = return_boundary(callable.returns().plan(), context)?;

    Ok(CallSignature {
        cancellation_name: crate::target::dart::name_style::cancellation_token_name(
            params.iter().map(|p| p.name.as_str()),
        )
        .to_owned(),
        params,
        return_dart_type,
        return_ty,
        error_ty,
        asynchronous,
    })
}

pub(super) fn error_exception_type(
    ty: &TypeRef,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    match ty {
        TypeRef::String => Ok("$$BoltException".to_owned()),
        TypeRef::Record(_) | TypeRef::Enum(_) => interop::dart_type(ty, context),
        _ => Err(unsupported("callback error payload type")),
    }
}

// Builds the expression thrown on the Dart-implementation side once the
// wire error value (already decoded to `decoded_expr`) is known.
pub(super) fn error_throw_expression(ty: &TypeRef, decoded_expr: &str) -> Result<String> {
    match ty {
        TypeRef::String => Ok(format!("$$BoltException({decoded_expr})")),
        TypeRef::Record(_) | TypeRef::Enum(_) => Ok(decoded_expr.to_owned()),
        _ => Err(unsupported("callback error payload type")),
    }
}

// The expression that recovers the wire-encodable error value from a
// caught Dart exception of `error_exception_type(ty, ..)`.
pub(super) fn error_caught_value(ty: &TypeRef, caught_expr: &str) -> String {
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
