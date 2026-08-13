use askama::Template;
use boltffi_binding::{
    DirectValueType, DirectVectorElementType, EnumDecl, ErrorDecl, ExecutionDecl, HandlePresence,
    HandleTarget, ImportedMethodDecl, IntoRust, Native, Primitive, ReturnPlan, TypeRef, VTableSlot,
    WritePlan,
};

use crate::{
    bridge::c::{
        CBridgeContract, CallbackCompletionParameter, CallbackSlot, ParameterGroup,
        Type as CBridgeType,
    },
    core::{Error, RenderContext, Result},
};

use super::super::super::{
    codec::{
        Reader, Sizer, ValueScope, WriteStatement, Writer, primitive_read_method, primitive_size,
        primitive_write_method,
    },
    name_style::Name,
    native::{self, NativeParameterSource, NativeType},
    render::direct_vector::PrimitiveVector,
    syntax::{Expression, Identifier, Parameter, TypeFragment},
    type_name,
};
use super::super::{Documentation, indent};
use super::parameter::{CallbackParameter, group_indices};

#[derive(Template)]
#[template(path = "target/dart/callback_interface_method.dart", escape = "none")]
struct InterfaceMethodTemplate<'a> {
    documentation: &'a Documentation,
    return_type: &'a TypeFragment,
    name: &'a Identifier,
    parameters: &'a [Parameter],
}

#[derive(Template)]
#[template(path = "target/dart/callback_proxy_method.dart", escape = "none")]
struct ProxyMethodTemplate<'a> {
    documentation: &'a Documentation,
    return_type: &'a TypeFragment,
    name: &'a Identifier,
    parameters: &'a [Parameter],
    body: &'a str,
}

#[derive(Template)]
#[template(path = "target/dart/callback_entry.dart", escape = "none")]
struct CallbackEntryTemplate<'a> {
    return_type: &'a TypeFragment,
    name: &'a Identifier,
    parameters: &'a [Parameter],
    asynchronous: bool,
    body: &'a str,
}

#[derive(Template)]
#[template(path = "target/dart/callback_callable.dart", escape = "none")]
struct CallbackCallableTemplate<'a> {
    signature: &'a TypeFragment,
    name: &'a Identifier,
    entry: &'a Identifier,
}

pub struct CallbackMethod {
    interface: String,
    proxy: String,
    entry: String,
    callable: Option<String>,
    vtable_initializer: String,
}

impl CallbackMethod {
    pub fn new(
        declaration: &ImportedMethodDecl<Native, VTableSlot>,
        slot: &CallbackSlot,
        callback_name: &Identifier,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if declaration.callable().params().len() != slot.source_parameter_groups().len() {
            return broken("Dart callback source parameter count disagrees with the C bridge");
        }
        let parameters = declaration
            .callable()
            .params()
            .iter()
            .zip(slot.source_parameter_groups())
            .map(|(parameter, group)| {
                CallbackParameter::from_declaration(parameter, group, slot, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let name = Name::new(declaration.name()).lower_camel()?;
        let asynchronous = matches!(
            declaration.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        );
        let public_return = public_return_type(declaration.callable().returns().plan(), context)?;
        let declared_return = match asynchronous {
            true => public_return.clone().future(),
            false => public_return,
        };
        let public_parameters = parameters
            .iter()
            .map(|parameter| parameter.signature().clone())
            .collect::<Vec<_>>();
        let documentation = Documentation::new(declaration.meta().doc(), 0);
        let interface = InterfaceMethodTemplate {
            documentation: &documentation,
            return_type: &declared_return,
            name: &name,
            parameters: &public_parameters,
        }
        .render()
        .expect("rendering an in-memory Dart callback interface method cannot fail");

        let proxy_body = match asynchronous {
            true => render_async_proxy(declaration, slot, &parameters, bridge, context)?,
            false => render_sync_proxy(declaration, slot, &parameters, bridge, context)?,
        };
        let proxy_body = indent(&proxy_body, 2);
        let proxy = ProxyMethodTemplate {
            documentation: &documentation,
            return_type: &declared_return,
            name: &name,
            parameters: &public_parameters,
            body: &proxy_body,
        }
        .render()
        .expect("rendering an in-memory Dart callback proxy method cannot fail");

        let native_return = TypeFragment::new(NativeType::from_c(slot.returns())?.dart());
        let native_parameters = slot
            .parameters()
            .iter()
            .map(|parameter| {
                NativeType::from_c(parameter.ty()).and_then(|ty| {
                    native::parameter_name(parameter.name()).and_then(|name| {
                        Identifier::parse(name)
                            .map(|name| Parameter::new(name, TypeFragment::new(ty.dart())))
                    })
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let entry_body = match asynchronous {
            true => render_async_entry(declaration, slot, &parameters, bridge, context)?,
            false => render_sync_entry(declaration, slot, &parameters, bridge, context)?,
        };
        let entry_name = Identifier::parse(format!("_m${name}"))?;
        let entry_body = indent(&entry_body, 2);
        let entry = CallbackEntryTemplate {
            return_type: &native_return,
            name: &entry_name,
            parameters: &native_parameters,
            asynchronous,
            body: &entry_body,
        }
        .render()
        .expect("rendering an in-memory Dart callback entry cannot fail");

        let native_signature = TypeFragment::new(native_function_signature(slot)?);
        let (callable, vtable_initializer) = if asynchronous {
            let callable_name = Identifier::parse(format!("_k${name}Callable"))?;
            (
                Some(
                    CallbackCallableTemplate {
                        signature: &native_signature,
                        name: &callable_name,
                        entry: &entry_name,
                    }
                    .render()
                    .expect("rendering an in-memory Dart callback callable cannot fail"),
                ),
                format!("..{} = {callable_name}.nativeFunction", slot.name()),
            )
        } else {
            let exceptional = native_exceptional_return(slot.returns())?;
            (
                None,
                format!(
                    "..{} = $$ffi.Pointer.fromFunction<{native_signature}>({callback_name}Bridge.{entry_name}{exceptional})",
                    slot.name()
                ),
            )
        };

        Ok(Self {
            interface,
            proxy,
            entry,
            callable,
            vtable_initializer,
        })
    }

    pub fn interface(&self) -> &str {
        &self.interface
    }

    pub fn proxy(&self) -> &str {
        &self.proxy
    }

    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn callable(&self) -> Option<&str> {
        self.callable.as_deref()
    }

    pub fn vtable_initializer(&self) -> Result<String> {
        Ok(self.vtable_initializer.clone())
    }
}

fn render_sync_entry(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    slot: &CallbackSlot,
    parameters: &[CallbackParameter],
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let method = Name::new(declaration.name()).lower_camel()?;
    let handle = native::parameter_name(
        slot.parameters()
            .first()
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart callback slot has no handle parameter",
            })?
            .name(),
    )?;
    let mut setup = vec![format!("final implementation = _k$handles.get({handle});")];
    setup.push(format!(
        "if (implementation == null) return {};",
        native_default(slot.returns())?
    ));
    setup.extend(
        parameters
            .iter()
            .flat_map(|parameter| parameter.entry_setup().iter().cloned()),
    );
    let arguments = parameters
        .iter()
        .map(CallbackParameter::entry_argument)
        .collect::<Vec<_>>()
        .join(", ");
    let call = format!("implementation.{method}({arguments})");
    match declaration.callable().error() {
        ErrorDecl::None(_) => setup.extend(render_infallible_entry_return(
            declaration.callable().returns().plan(),
            &call,
            bridge,
            context,
        )?),
        ErrorDecl::EncodedViaReturnSlot { ty, codec, .. } => {
            setup.extend(render_fallible_entry_return(
                declaration.callable().returns().plan(),
                ty,
                codec,
                slot.return_parameter_groups(),
                slot,
                &call,
                bridge,
                context,
            )?)
        }
        _ => return super::unsupported("Dart callback error channel"),
    }
    Ok(setup.join("\n"))
}

pub fn render_infallible_entry_return(
    plan: &ReturnPlan<Native, IntoRust>,
    call: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    Ok(match plan {
        ReturnPlan::Void => vec![format!("{call};")],
        ReturnPlan::DirectViaReturnSlot { ty } => {
            vec![format!(
                "return {};",
                direct_into_native(ty, call, context)?
            )]
        }
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            let mut statements = vec![format!("final _l$value = {call};")];
            statements.extend(encode_value(
                codec,
                "_l$value",
                "_l$return",
                bridge,
                context,
            )?);
            statements.push("return _l$returnBuffer;".to_owned());
            statements
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
            let mut statements = encode_scalar_option(call, *primitive, "_l$return", bridge)?;
            statements.push("return _l$returnBuffer;".to_owned());
            statements
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let mut statements = encode_direct_vector(call, element, "_l$return", bridge, context)?;
            statements.push("return _l$returnBuffer;".to_owned());
            statements
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        } => vec![format!(
            "return {};",
            handle_into_native(target, *presence, call, context)?
        )],
        _ => return super::unsupported("Dart infallible callback return"),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn render_fallible_entry_return(
    plan: &ReturnPlan<Native, IntoRust>,
    error_type: &TypeRef,
    error_codec: &WritePlan,
    return_groups: &[ParameterGroup],
    parameters: &impl NativeParameterSource,
    call: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let success_group = match (plan, return_groups) {
        (ReturnPlan::Void, []) => None,
        (_, [ParameterGroup::SuccessOut(index)]) => Some(parameters.parameter(*index)),
        _ => return broken("Dart fallible callback success out-parameter group"),
    };
    let mut success = Vec::new();
    match plan {
        ReturnPlan::Void => success.push(format!("{call};")),
        ReturnPlan::DirectViaOutPointer { ty } => {
            let output = success_group.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart direct callback success pointer is missing",
            })?;
            let output_name = native::parameter_name(output.name())?;
            success.push(format!(
                "{} = {};",
                pointer_assignment(&output_name, output.ty())?,
                direct_into_native(ty, call, context)?
            ));
        }
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let output = success_group.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart encoded callback success pointer is missing",
            })?;
            success.push(format!("final _l$value = {call};"));
            success.extend(encode_value(
                codec,
                "_l$value",
                "_l$success",
                bridge,
                context,
            )?);
            success.push(format!(
                "{}.ref = _l$successBuffer;",
                native::parameter_name(output.name())?
            ));
        }
        ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => {
            let output = success_group.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart callback handle success pointer is missing",
            })?;
            let output_name = native::parameter_name(output.name())?;
            success.push(format!(
                "{} = {};",
                pointer_assignment(&output_name, output.ty())?,
                handle_into_native(target, *presence, call, context)?
            ));
        }
        _ => return super::unsupported("Dart fallible callback success return"),
    }
    success.push("return $$ffi.Struct.create<_$$BoltFFIBuf>();".to_owned());

    let error_binding = error_catch_binding(error_type, context)?;
    let mut failure = encode_value(
        error_codec,
        error_binding.value.as_str(),
        "_l$error",
        bridge,
        context,
    )?;
    failure.push("return _l$errorBuffer;".to_owned());
    Ok(vec![format!(
        "try {{\n{}\n}} on {} catch ({}) {{\n{}\n}}",
        indent(&success.join("\n"), 2),
        error_binding.ty,
        error_binding.name,
        indent(&failure.join("\n"), 2),
    )])
}

fn render_sync_proxy(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    slot: &CallbackSlot,
    parameters: &[CallbackParameter],
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let dart_signature = dart_function_signature(slot)?;
    let mut statements = vec![
        "if (_handle.handle == 0) throw StateError('callback has been disposed');".to_owned(),
        format!(
            "final _l$invoke = _vtable.{}.asFunction<{dart_signature}>();",
            slot.name()
        ),
    ];
    statements.extend(parameters.iter().flat_map(|parameter| {
        parameter
            .proxy_setup()
            .iter()
            .filter(|line| !line.is_empty())
            .cloned()
    }));
    let mut arguments = vec![None; slot.parameters().len()];
    arguments[0] = Some("_handle.handle".to_owned());
    populate_source_arguments(&mut arguments, slot, parameters)?;

    let call = match declaration.callable().error() {
        ErrorDecl::None(_) => {
            let arguments = complete_arguments(arguments)?;
            render_infallible_proxy_return(
                declaration.callable().returns().plan(),
                &format!("_l$invoke({})", arguments.join(", ")),
                bridge,
                context,
            )?
        }
        ErrorDecl::EncodedViaReturnSlot { ty, codec, .. } => render_fallible_proxy_return(
            declaration.callable().returns().plan(),
            ty,
            codec,
            slot.return_parameter_groups(),
            slot,
            "_l$invoke",
            &[],
            arguments,
            bridge,
            context,
        )?,
        _ => return super::unsupported("Dart callback proxy error channel"),
    };
    statements.extend(call);
    Ok(statements.join("\n"))
}

pub fn render_infallible_proxy_return(
    plan: &ReturnPlan<Native, IntoRust>,
    call: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    Ok(match plan {
        ReturnPlan::Void => vec![format!("{call};")],
        ReturnPlan::DirectViaReturnSlot { ty } => {
            vec![format!(
                "return {};",
                direct_from_native(ty, call, context)?
            )]
        }
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            decode_buffer_return(codec, call, "_l$return", bridge, context)?
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
            decode_scalar_option_return(*primitive, call, "_l$return", bridge)?
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            decode_direct_vector_return(element, call, "_l$return", bridge, context)?
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        } => vec![format!(
            "return {};",
            handle_from_native(target, *presence, call, context)?
        )],
        _ => return super::unsupported("Dart infallible callback proxy return"),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn render_fallible_proxy_return(
    plan: &ReturnPlan<Native, IntoRust>,
    error_type: &TypeRef,
    error_codec: &WritePlan,
    return_groups: &[ParameterGroup],
    parameters: &impl NativeParameterSource,
    invoke: &str,
    leading_arguments: &[String],
    mut arguments: Vec<Option<String>>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let success = match (plan, return_groups) {
        (ReturnPlan::Void, []) => None,
        (_, [ParameterGroup::SuccessOut(index)]) => {
            let output = parameters.parameter(*index);
            let CBridgeType::MutPointer(inner) = output.ty() else {
                return broken("Dart callback success output is not a pointer");
            };
            let native = NativeType::from_c(inner)?;
            let storage = "_l$successOut";
            arguments[index.position()] = Some(format!("{storage}.ptr"));
            Some((
                format!(
                    "final {storage} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>());",
                    native.native(),
                    native.native()
                ),
                format!("{storage}.ptr"),
            ))
        }
        _ => return broken("Dart fallible callback proxy success group"),
    };
    let mut statements = success
        .as_ref()
        .map(|(allocation, _)| allocation.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let arguments = complete_arguments(arguments)?;
    let arguments = leading_arguments
        .iter()
        .cloned()
        .chain(arguments)
        .collect::<Vec<_>>();
    statements.push(format!(
        "final _l$errorBuffer = {invoke}({});",
        arguments.join(", ")
    ));
    let error_decode = error_codec
        .read_plan()
        .render_with(&mut Reader::new("_l$errorReader", context))?
        .into_source();
    statements.push(format!(
        "if (_l$errorBuffer.ptr != $$ffi.nullptr) {{\n  try {{\n    final _l$errorReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan(_l$errorBuffer.ptr, _l$errorBuffer.len));\n    throw {};\n  }} finally {{\n    _f${}(_l$errorBuffer);\n  }}\n}}",
        error_throw_expression(error_type, &error_decode, context)?,
        bridge.support().buffer_free()?.name(),
    ));
    match plan {
        ReturnPlan::Void => {}
        ReturnPlan::DirectViaOutPointer { ty } => {
            let (_, pointer) = success.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart direct callback success storage is missing",
            })?;
            let CBridgeType::MutPointer(inner) = parameters
                .parameter(match return_groups {
                    [ParameterGroup::SuccessOut(index)] => *index,
                    _ => return broken("Dart direct callback success group"),
                })
                .ty()
            else {
                return broken("Dart direct callback success pointer");
            };
            statements.push(format!(
                "return {};",
                direct_from_native(ty, &native::pointer_read(inner, &pointer)?, context)?
            ));
        }
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let (_, pointer) = success.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart encoded callback success storage is missing",
            })?;
            statements.extend(decode_buffer_return(
                codec,
                &format!("{pointer}.ref"),
                "_l$success",
                bridge,
                context,
            )?);
        }
        ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => {
            let (_, pointer) = success.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart callback handle success storage is missing",
            })?;
            let CBridgeType::MutPointer(inner) = parameters
                .parameter(match return_groups {
                    [ParameterGroup::SuccessOut(index)] => *index,
                    _ => return broken("Dart callback handle success group"),
                })
                .ty()
            else {
                return broken("Dart callback handle success pointer");
            };
            statements.push(format!(
                "return {};",
                handle_from_native(
                    target,
                    *presence,
                    &native::pointer_read(inner, &pointer)?,
                    context,
                )?
            ));
        }
        _ => return super::unsupported("Dart fallible callback proxy success"),
    }
    Ok(statements)
}

fn render_async_entry(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    slot: &CallbackSlot,
    parameters: &[CallbackParameter],
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let completion = completion_group(slot)?;
    let complete_parameter = slot.parameter(completion.callback());
    let CBridgeType::FunctionPointer {
        params: completion_parameters,
        ..
    } = complete_parameter.ty()
    else {
        return broken("Dart callback completion is not a function pointer");
    };
    let completion_signature = dart_callback_signature(complete_parameter.ty())?;
    let complete = native::parameter_name(complete_parameter.name())?;
    let completion_context = native::parameter_name(slot.parameter(completion.context()).name())?;
    let handle = native::parameter_name(slot.parameters()[0].name())?;
    let method = Name::new(declaration.name()).lower_camel()?;
    let missing_implementation = match completion_parameters.get(2) {
        Some(payload) => format!(
            "if (implementation == null) {{\n  _l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100, {});\n  return;\n}}",
            native_default(payload)?
        ),
        None => format!(
            "if (implementation == null) {{\n  _l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100);\n  return;\n}}"
        ),
    };
    let mut statements = vec![
        format!("final _l$complete = {complete}.asFunction<{completion_signature}>();"),
        format!("final implementation = _k$handles.get({handle});"),
        missing_implementation,
    ];
    let decode = parameters
        .iter()
        .flat_map(|parameter| parameter.entry_setup().iter().cloned())
        .collect::<Vec<_>>();
    let arguments = parameters
        .iter()
        .map(CallbackParameter::entry_argument)
        .collect::<Vec<_>>()
        .join(", ");
    let call = format!("await implementation.{method}({arguments})");
    let has_payload = completion_parameters.len() == 3;
    let mut success = decode;
    success.extend(async_success_payload(
        declaration.callable().returns().plan(),
        &call,
        has_payload.then(|| &completion_parameters[2]),
        bridge,
        context,
    )?);
    let success_payload = has_payload.then(|| {
        if completion_parameters[2] == CBridgeType::Buffer {
            "_l$payloadBuffer"
        } else {
            "_l$payload"
        }
    });
    success.push(completion_call(&completion_context, 0, success_payload));

    let mut catches = Vec::new();
    if let ErrorDecl::EncodedViaReturnSlot { ty, codec, .. } = declaration.callable().error() {
        let binding = error_catch_binding(ty, context)?;
        // A C-style enum error crosses as a bare discriminant in its
        // declared repr width, not a wire-encoded payload -- a data enum
        // (with variant fields) still needs the normal codec below, since
        // it has no `.value` getter.
        let c_style_repr = match ty {
            TypeRef::Enum(id) => match context.enumeration(*id) {
                Some(EnumDecl::CStyle(decl)) => Some(decl.repr()),
                _ => None,
            },
            _ => None,
        };
        let mut body = if let Some(repr) = c_style_repr {
            let primitive = repr.primitive();
            let size = primitive_size(primitive);
            let write_method = primitive_write_method(primitive);
            vec![
                format!("final _l$errorStorage = _$$BoltCallocPtr<$$ffi.Uint8>.alloc({size});"),
                format!(
                    "_$$BoltWireEncoder(_$$BoltBufWriter.fromSpan(_l$errorStorage.ptr, _l$errorStorage.len)).{write_method}({}.value);",
                    binding.name
                ),
                format!(
                    "final _l$errorBuffer = _f${}(_l$errorStorage.ptr, {size});",
                    bridge.support().buffer_from_bytes()?.name()
                ),
            ]
        } else {
            encode_value(codec, binding.value.as_str(), "_l$error", bridge, context)?
        };
        body.push(completion_call(
            &completion_context,
            1,
            Some("_l$errorBuffer"),
        ));
        let unexpected = if has_payload && completion_parameters[2] == CBridgeType::Buffer {
            format!(
                "_l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 1, _f$encodeUnexpectedCallbackError({}));",
                binding.name
            )
        } else if has_payload {
            format!(
                "_l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100, {});",
                native_default(&completion_parameters[2])?
            )
        } else {
            format!(
                "_l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100);"
            )
        };
        catches.push(format!(
            "catch ({}) {{\n  if ({} is {}) {{\n{}\n  }} else {{\n    {}\n  }}\n}}",
            binding.name,
            binding.name,
            binding.ty,
            indent(&body.join("\n"), 4),
            unexpected
        ));
    } else {
        let unexpected = if has_payload && completion_parameters[2] == CBridgeType::Buffer {
            format!(
                "_l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100, $$ffi.Struct.create<_$$BoltFFIBuf>());"
            )
        } else if has_payload {
            format!(
                "_l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100, {});",
                native_default(&completion_parameters[2])?
            )
        } else {
            format!(
                "_l$complete({completion_context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = 100);"
            )
        };
        catches.push(format!("catch (_l$caught) {{\n  {unexpected}\n}}"));
    }
    statements.push(format!(
        "try {{\n{}\n}} {}",
        indent(&success.join("\n"), 2),
        catches.join(" ")
    ));
    Ok(statements.join("\n"))
}

fn render_async_proxy(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    slot: &CallbackSlot,
    parameters: &[CallbackParameter],
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let completion = completion_group(slot)?;
    let complete_parameter = slot.parameter(completion.callback());
    let CBridgeType::FunctionPointer {
        params: completion_parameters,
        ..
    } = complete_parameter.ty()
    else {
        return broken("Dart callback completion is not a function pointer");
    };
    let native_completion_signature = native_callback_signature(complete_parameter.ty())?;
    let dart_completion_parameters = completion_parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            NativeType::from_c(ty).map(|ty| format!("{} _p$value{index}", ty.dart()))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let dart_signature = dart_function_signature(slot)?;
    let public_return = public_return_type(declaration.callable().returns().plan(), context)?;
    let mut statements = vec![
        "if (_handle.handle == 0) throw StateError('callback has been disposed');".to_owned(),
        format!(
            "final _l$invoke = _vtable.{}.asFunction<{dart_signature}>();",
            slot.name()
        ),
    ];
    statements.extend(parameters.iter().flat_map(|parameter| {
        parameter
            .proxy_setup()
            .iter()
            .filter(|line| !line.is_empty())
            .cloned()
    }));
    statements.push(format!(
        "final _l$completer = $$async.Completer<{public_return}>();"
    ));
    let has_payload = completion_parameters.len() == 3;
    let success = async_proxy_success(
        declaration.callable().returns().plan(),
        has_payload.then_some("_p$value2"),
        has_payload.then_some(&completion_parameters[2]),
        bridge,
        context,
    )?;
    let mut completion_body = vec![
        "_l$completion.close();".to_owned(),
        format!(
            "if (_p$value1.code == 0) {{\n{}\n}}",
            indent(&success.join("\n"), 2)
        ),
    ];
    if let ErrorDecl::EncodedViaReturnSlot { ty, codec, .. } = declaration.callable().error() {
        let decode = codec
            .read_plan()
            .render_with(&mut Reader::new("_l$errorReader", context))?
            .into_source();
        completion_body.push(format!(
            "else if (_p$value1.code == 1) {{\n  final _l$errorReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan(_p$value2.ptr, _p$value2.len));\n  _l$completer.completeError({});\n}}",
            error_throw_expression(ty, &decode, context)?
        ));
    }
    completion_body.push(
        "else {\n  _l$completer.completeError($$BoltException('callback failed with status ${_p$value1.code}'));\n}"
            .to_owned(),
    );
    if has_payload && completion_parameters[2] == CBridgeType::Buffer {
        completion_body.push(format!(
            "if (_p$value2.ptr != $$ffi.nullptr) _f${}(_p$value2);",
            bridge.support().buffer_free()?.name()
        ));
    }
    statements.push(format!(
        "late final $$ffi.NativeCallable<{native_completion_signature}> _l$completion;\n_l$completion = $$ffi.NativeCallable.listener(({dart_completion_parameters}) {{\n{}\n}});",
        indent(&completion_body.join("\n"), 2)
    ));
    let mut arguments = vec![None; slot.parameters().len()];
    arguments[0] = Some("_handle.handle".to_owned());
    populate_source_arguments(&mut arguments, slot, parameters)?;
    arguments[completion.callback().position()] = Some("_l$completion.nativeFunction".to_owned());
    arguments[completion.context().position()] = Some("$$ffi.nullptr".to_owned());
    statements.push(format!(
        "_l$invoke({});",
        complete_arguments(arguments)?.join(", ")
    ));
    statements.push("return _l$completer.future;".to_owned());
    Ok(statements.join("\n"))
}

fn async_success_payload(
    plan: &ReturnPlan<Native, IntoRust>,
    call: &str,
    payload: Option<&CBridgeType>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let Some(payload) = payload else {
        return match plan {
            ReturnPlan::Void => Ok(vec![format!("{call};")]),
            _ => broken("Dart async callback payload is missing"),
        };
    };
    match (plan, payload) {
        (ReturnPlan::Void, CBridgeType::Buffer) => Ok(vec![
            "final _l$payloadBuffer = $$ffi.Struct.create<_$$BoltFFIBuf>();".to_owned(),
        ]),
        (ReturnPlan::DirectViaReturnSlot { ty }, CBridgeType::Buffer)
        | (ReturnPlan::DirectViaOutPointer { ty }, CBridgeType::Buffer) => {
            encode_direct_wire(call, ty, "_l$payload", bridge, context)
        }
        (ReturnPlan::DirectViaReturnSlot { ty }, _) => Ok(vec![format!(
            "final _l$payload = {};",
            direct_into_native(ty, call, context)?
        )]),
        (ReturnPlan::EncodedViaReturnSlot { codec, .. }, CBridgeType::Buffer)
        | (ReturnPlan::EncodedViaOutPointer { codec, .. }, CBridgeType::Buffer) => {
            let mut statements = vec![format!("final _l$value = {call};")];
            statements.extend(encode_value(
                codec,
                "_l$value",
                "_l$payload",
                bridge,
                context,
            )?);
            Ok(statements)
        }
        (ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. }, CBridgeType::Buffer) => {
            encode_scalar_option(call, *primitive, "_l$payload", bridge)
        }
        (ReturnPlan::DirectVecViaReturnSlot { element }, CBridgeType::Buffer) => {
            encode_direct_vector(call, element, "_l$payload", bridge, context)
        }
        (
            ReturnPlan::HandleViaReturnSlot {
                target, presence, ..
            }
            | ReturnPlan::HandleViaOutPointer {
                target, presence, ..
            },
            _,
        ) => Ok(vec![format!(
            "final _l$payload = {};",
            handle_into_native(target, *presence, call, context)?
        )]),
        _ => super::unsupported("Dart async callback success payload"),
    }
}

fn async_proxy_success(
    plan: &ReturnPlan<Native, IntoRust>,
    payload: Option<&str>,
    payload_ty: Option<&CBridgeType>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let Some(payload) = payload else {
        return match plan {
            ReturnPlan::Void => Ok(vec!["_l$completer.complete();".to_owned()]),
            _ => broken("Dart async callback proxy payload is missing"),
        };
    };
    Ok(match plan {
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            let decoded = match (ty, payload_ty) {
                (DirectValueType::Record(_), Some(CBridgeType::Buffer)) => format!(
                    "{}._m$wireDecode(_$$BoltWireDecoder(_$$BoltBufReader.fromSpan({payload}.ptr, {payload}.len)))",
                    type_name::direct_value(ty, context)?
                ),
                _ => direct_from_native(ty, payload, context)?,
            };
            vec![format!("_l$completer.complete({decoded});")]
        }
        ReturnPlan::EncodedViaReturnSlot { codec, .. }
        | ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let decode = codec
                .read_plan()
                .render_with(&mut Reader::new("_l$successReader", context))?
                .into_source();
            vec![
                format!(
                    "final _l$successReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({payload}.ptr, {payload}.len));"
                ),
                format!("_l$completer.complete({decode});"),
            ]
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => vec![
            format!(
                "final _l$successReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({payload}.ptr, {payload}.len));"
            ),
            format!(
                "_l$completer.complete(_l$successReader.readU8() == 0 ? null : _l$successReader.{}());",
                primitive_read_method(*primitive)
            ),
        ],
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            direct_vector_decode_statements(element, payload, "_l$decoded", bridge, context)?
                .into_iter()
                .chain(["_l$completer.complete(_l$decoded);".to_owned()])
                .collect()
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => vec![format!(
            "_l$completer.complete({});",
            handle_from_native(target, *presence, payload, context)?
        )],
        ReturnPlan::Void => vec!["_l$completer.complete();".to_owned()],
        _ => return super::unsupported("Dart async callback proxy success"),
    })
}

fn encode_value(
    codec: &WritePlan,
    value: &str,
    prefix: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let storage = format!("{prefix}Storage");
    let writer = format!("{prefix}Writer");
    let buffer = format!("{prefix}Buffer");
    let size = codec
        .size_with(&mut Sizer::new(ValueScope::current(value), context))?
        .into_source();
    let writes = codec
        .render_with(&mut Writer::new(
            &writer,
            ValueScope::current(value),
            context,
        ))
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .map(WriteStatement::into_source)
        .collect::<Vec<_>>();
    Ok(vec![
        format!("final {storage} = _$$BoltStoragePool.acquireStorage({size});"),
        format!(
            "final {writer} = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({storage}.ptr, {storage}.len));"
        ),
        writes.join("\n"),
        // `buffer_from_bytes` copies into a Rust-owned buffer, so the pooled
        // storage can go back to the pool immediately afterward.
        format!("final {buffer} = _f$buffer_symbol({storage}.ptr, {writer}.len);").replace(
            "buffer_symbol",
            bridge.support().buffer_from_bytes()?.name(),
        ),
        format!("_$$BoltStoragePool.releaseStorage({storage});"),
    ])
}

fn encode_direct_wire(
    value: &str,
    ty: &DirectValueType,
    prefix: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let (size, write) = match ty {
        DirectValueType::Primitive(primitive) => (
            super::super::super::codec::primitive_size(*primitive).to_string(),
            format!(
                "{prefix}Writer.{}(_l$value);",
                primitive_write_method(*primitive)
            ),
        ),
        DirectValueType::Record(_) => (
            "_l$value._m$wireEncodedSize()".to_owned(),
            format!("_l$value._m$wireEncode({prefix}Writer);"),
        ),
        DirectValueType::Enum(id) => {
            let Some(EnumDecl::CStyle(enumeration)) = context.enumeration(*id) else {
                return super::unsupported("Dart direct callback enum wire encoding");
            };
            let primitive = enumeration.repr().primitive();
            (
                super::super::super::codec::primitive_size(primitive).to_string(),
                format!(
                    "{prefix}Writer.{}(_l$value.value);",
                    primitive_write_method(primitive)
                ),
            )
        }
        _ => return super::unsupported("Dart direct callback wire encoding"),
    };
    let mut statements = vec![format!("final _l$value = {value};")];
    statements.extend([
        format!("final {prefix}Storage = _$$BoltStoragePool.acquireStorage({size});"),
        format!("final {prefix}Writer = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({prefix}Storage.ptr, {prefix}Storage.len));"),
        write,
        format!(
            "final {prefix}Buffer = _f$buffer_symbol({prefix}Storage.ptr, {prefix}Writer.len);"
        )
        .replace("buffer_symbol", bridge.support().buffer_from_bytes()?.name()),
        format!("_$$BoltStoragePool.releaseStorage({prefix}Storage);"),
    ]);
    Ok(statements)
}

fn encode_scalar_option(
    value: &str,
    primitive: Primitive,
    prefix: &str,
    bridge: &CBridgeContract,
) -> Result<Vec<String>> {
    let size = 1 + super::super::super::codec::primitive_size(primitive);
    Ok(vec![
        format!("final _l$value = {value};"),
        format!("final {prefix}Storage = _$$BoltStoragePool.acquireStorage({size});"),
        format!(
            "final {prefix}Writer = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({prefix}Storage.ptr, {prefix}Storage.len));"
        ),
        format!(
            "if (_l$value == null) {{\n  {prefix}Writer.writeU8(0);\n}} else {{\n  {prefix}Writer.writeU8(1);\n  {prefix}Writer.{}(_l$value);\n}}",
            primitive_write_method(primitive)
        ),
        format!(
            "final {prefix}Buffer = _f$buffer_symbol({prefix}Storage.ptr, {prefix}Writer.len);"
        )
        .replace(
            "buffer_symbol",
            bridge.support().buffer_from_bytes()?.name(),
        ),
        format!("_$$BoltStoragePool.releaseStorage({prefix}Storage);"),
    ])
}

fn encode_direct_vector(
    value: &str,
    element: &DirectVectorElementType,
    prefix: &str,
    bridge: &CBridgeContract,
    _context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let mut statements = vec![format!("final _l$value = {value};")];
    match element {
        DirectVectorElementType::Primitive(primitive) => {
            let primitive = primitive.primitive();
            let vector = PrimitiveVector::new(primitive)?;
            let native = vector.native();
            statements.push(format!(
                "final {prefix}Storage = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>() * _l$value.length);",
                native.native(), native.native()
            ));
            statements.push(vector.populate(&format!("{prefix}Storage"), "_l$value")?);
            statements.push(format!(
                "final {prefix}Buffer = _f$buffer_symbol({prefix}Storage.ptr.cast<$$ffi.Uint8>(), _l$value.length * $$ffi.sizeOf<{}>());",
                native.native()
            ).replace("buffer_symbol", bridge.support().buffer_from_bytes()?.name()));
        }
        DirectVectorElementType::Record(record) => {
            let native = native::direct_record_struct(bridge, *record)?;
            statements.extend([
                format!("final {prefix}Storage = _$$BoltCallocPtr<{native}>.alloc($$ffi.sizeOf<{native}>() * _l$value.length);"),
                format!("for (var _l$index = 0; _l$index < _l$value.length; _l$index++) {{ _l$value[_l$index]._m$writeStruct({prefix}Storage.ptr.elementAt(_l$index)); }}"),
                format!("final {prefix}Buffer = _f$buffer_symbol({prefix}Storage.ptr.cast<$$ffi.Uint8>(), _l$value.length * $$ffi.sizeOf<{native}>());")
                    .replace("buffer_symbol", bridge.support().buffer_from_bytes()?.name()),
            ]);
        }
        _ => return super::unsupported("Dart direct-vector callback return element"),
    }
    Ok(statements)
}

fn decode_buffer_return(
    codec: &WritePlan,
    call: &str,
    prefix: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let decode = codec
        .read_plan()
        .render_with(&mut Reader::new(format!("{prefix}Reader"), context))?
        .into_source();
    Ok(vec![
        format!("final {prefix}Buffer = {call};"),
        format!(
            "try {{\n  final {prefix}Reader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({prefix}Buffer.ptr, {prefix}Buffer.len));\n  return {decode};\n}} finally {{\n  _f${}({prefix}Buffer);\n}}",
            bridge.support().buffer_free()?.name()
        ),
    ])
}

fn decode_scalar_option_return(
    primitive: Primitive,
    call: &str,
    prefix: &str,
    bridge: &CBridgeContract,
) -> Result<Vec<String>> {
    Ok(vec![
        format!("final {prefix}Buffer = {call};"),
        format!(
            "try {{\n  final {prefix}Reader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({prefix}Buffer.ptr, {prefix}Buffer.len));\n  return {prefix}Reader.readU8() == 0 ? null : {prefix}Reader.{}();\n}} finally {{\n  _f${}({prefix}Buffer);\n}}",
            primitive_read_method(primitive),
            bridge.support().buffer_free()?.name(),
        ),
    ])
}

fn decode_direct_vector_return(
    element: &DirectVectorElementType,
    call: &str,
    prefix: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let mut statements = vec![format!("final {prefix}Buffer = {call};")];
    statements.push("try {".to_owned());
    statements.extend(
        direct_vector_decode_statements(
            element,
            &format!("{prefix}Buffer"),
            "_l$value",
            bridge,
            context,
        )?
        .into_iter()
        .map(|statement| format!("  {statement}")),
    );
    statements.push("  return _l$value;".to_owned());
    statements.push(format!(
        "}} finally {{\n  _f${}({prefix}Buffer);\n}}",
        bridge.support().buffer_free()?.name()
    ));
    Ok(statements)
}

fn direct_vector_decode_statements(
    element: &DirectVectorElementType,
    buffer: &str,
    value: &str,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    Ok(match element {
        DirectVectorElementType::Primitive(primitive) => {
            let primitive = primitive.primitive();
            let vector = PrimitiveVector::new(primitive)?;
            let length = format!(
                "{buffer}.len ~/ $$ffi.sizeOf<{}>()",
                vector.native().native()
            );
            let expression = vector.copied_from(&format!("{buffer}.ptr"), &length)?;
            vec![format!("final {value} = {expression};")]
        }
        DirectVectorElementType::Record(record) => {
            let public = type_name::direct_value(&DirectValueType::Record(*record), context)?;
            let native = native::direct_record_struct(bridge, *record)?;
            vec![
                format!("final _l$count = {buffer}.len ~/ $$ffi.sizeOf<{native}>();"),
                format!(
                    "final {value} = List<{public}>.generate(_l$count, (_l$index) => {public}._m$fromStruct({buffer}.ptr.cast<{native}>().elementAt(_l$index).ref));"
                ),
            ]
        }
        _ => return super::unsupported("Dart direct-vector callback decoding"),
    })
}

fn direct_into_native(
    ty: &DirectValueType,
    expression: &str,
    _context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match ty {
        DirectValueType::Primitive(_) => expression.to_owned(),
        DirectValueType::Enum(_) => format!("({expression}).value"),
        DirectValueType::Record(_) => format!("({expression})._m$toStruct()"),
        _ => return super::unsupported("Dart direct callback return"),
    })
}

fn direct_from_native(
    ty: &DirectValueType,
    expression: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    Ok(match ty {
        DirectValueType::Primitive(_) => expression.to_owned(),
        DirectValueType::Enum(_) => format!(
            "{}._m$fromDiscriminant({expression})",
            type_name::direct_value(ty, context)?
        ),
        DirectValueType::Record(_) => format!(
            "{}._m$fromStruct({expression})",
            type_name::direct_value(ty, context)?
        ),
        _ => return super::unsupported("Dart direct callback proxy return"),
    })
}

fn handle_into_native(
    target: &HandleTarget,
    presence: HandlePresence,
    expression: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    match target {
        HandleTarget::Class(_) => Ok(match presence {
            HandlePresence::Required => format!("({expression})._handle"),
            HandlePresence::Nullable => format!("({expression})?._handle ?? 0"),
            _ => return super::unsupported("Dart callback class return presence"),
        }),
        HandleTarget::Callback(id) => Ok(format!(
            "{}Bridge.create({expression})",
            callback_type(*id, context)?
        )),
        HandleTarget::Stream(_) => super::unsupported("Dart callback stream return"),
        _ => super::unsupported("Dart callback handle return target"),
    }
}

fn handle_from_native(
    target: &HandleTarget,
    presence: HandlePresence,
    expression: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    let required = type_name::handle(target, HandlePresence::Required, context)?;
    match target {
        HandleTarget::Class(_) => Ok(match presence {
            HandlePresence::Required => format!("{required}._({expression})"),
            HandlePresence::Nullable => {
                format!("{expression} == 0 ? null : {required}._({expression})")
            }
            _ => return super::unsupported("Dart callback class proxy return presence"),
        }),
        HandleTarget::Callback(_) => Ok(match presence {
            HandlePresence::Required => format!("{required}Bridge.wrap({expression})"),
            HandlePresence::Nullable => {
                format!("{expression}.handle == 0 ? null : {required}Bridge.wrap({expression})")
            }
            _ => return super::unsupported("Dart callback proxy return presence"),
        }),
        HandleTarget::Stream(_) => super::unsupported("Dart callback stream proxy return"),
        _ => super::unsupported("Dart callback proxy return target"),
    }
}

pub fn public_return_type(
    plan: &ReturnPlan<Native, IntoRust>,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    Ok(match plan {
        ReturnPlan::Void => TypeFragment::new("void"),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            type_name::direct_value(ty, context)?
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => type_name::type_ref(ty, context)?,
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => type_name::handle(target, *presence, context)?,
        ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
            type_name::primitive_type(*primitive)?.optional()
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            type_name::direct_vector(element, context)?
        }
        _ => return super::unsupported("Dart callback public return type"),
    })
}

fn populate_source_arguments(
    arguments: &mut [Option<String>],
    slot: &CallbackSlot,
    parameters: &[CallbackParameter],
) -> Result<()> {
    slot.source_parameter_groups()
        .iter()
        .zip(parameters)
        .try_for_each(|(group, parameter)| {
            let indices = group_indices(group)?;
            if indices.len() != parameter.proxy_arguments().len() {
                return broken("Dart callback source argument width disagrees with C bridge");
            }
            indices
                .into_iter()
                .zip(parameter.proxy_arguments())
                .for_each(|(index, argument)| arguments[index] = Some(argument.clone()));
            Ok(())
        })
}

fn complete_arguments(arguments: Vec<Option<String>>) -> Result<Vec<String>> {
    arguments
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "Dart callback native argument is missing",
        })
}

fn pointer_assignment(name: &str, ty: &CBridgeType) -> Result<String> {
    let CBridgeType::MutPointer(inner) = ty else {
        return broken("Dart callback out-parameter is not a pointer");
    };
    Ok(match inner.as_ref() {
        CBridgeType::Status
        | CBridgeType::Buffer
        | CBridgeType::String
        | CBridgeType::Span
        | CBridgeType::CallbackHandle(_)
        | CBridgeType::Named(_)
        | CBridgeType::DirectRecord(_) => format!("{name}.ref"),
        _ => format!("{name}.value"),
    })
}

fn native_function_signature(slot: &CallbackSlot) -> Result<String> {
    Ok(format!(
        "{} Function({})",
        NativeType::from_c(slot.returns())?.native(),
        slot.parameters()
            .iter()
            .map(|parameter| NativeType::from_c(parameter.ty()).map(|ty| ty.native().to_owned()))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}

fn dart_function_signature(slot: &CallbackSlot) -> Result<String> {
    Ok(format!(
        "{} Function({})",
        NativeType::from_c(slot.returns())?.dart(),
        slot.parameters()
            .iter()
            .map(|parameter| NativeType::from_c(parameter.ty()).map(|ty| ty.dart().to_owned()))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}

fn native_callback_signature(ty: &CBridgeType) -> Result<String> {
    NativeType::from_c(ty).and_then(|ty| {
        ty.native()
            .strip_prefix("$$ffi.Pointer<$$ffi.NativeFunction<")
            .and_then(|signature| signature.strip_suffix(">>"))
            .map(str::to_owned)
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart callback completion native signature is malformed",
            })
    })
}

fn dart_callback_signature(ty: &CBridgeType) -> Result<String> {
    let CBridgeType::FunctionPointer { returns, params } = ty else {
        return broken("Dart callback completion type");
    };
    Ok(format!(
        "{} Function({})",
        NativeType::from_c(returns)?.dart(),
        params
            .iter()
            .map(|parameter| NativeType::from_c(parameter).map(|ty| ty.dart().to_owned()))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}

fn completion_group(slot: &CallbackSlot) -> Result<&CallbackCompletionParameter> {
    match slot.parameter_groups().last() {
        Some(ParameterGroup::CallbackCompletion(completion)) => Ok(completion),
        _ => broken("Dart async callback completion group is missing"),
    }
}

fn completion_call(context: &str, status: i32, payload: Option<&str>) -> String {
    format!(
        "_l$complete({context}, $$ffi.Struct.create<_$$BoltFFIStatus>()..code = {status}{});",
        payload
            .map(|payload| format!(", {payload}"))
            .unwrap_or_default()
    )
}

fn native_exceptional_return(ty: &CBridgeType) -> Result<String> {
    Ok(match exceptional_return_value(ty)? {
        Some(value) => format!(", {value}"),
        None => String::new(),
    })
}

pub fn exceptional_return_value(ty: &CBridgeType) -> Result<Option<String>> {
    Ok(match ty {
        CBridgeType::Void
        | CBridgeType::Status
        | CBridgeType::Buffer
        | CBridgeType::String
        | CBridgeType::Span
        | CBridgeType::CallbackHandle(_)
        | CBridgeType::Named(_)
        | CBridgeType::DirectRecord(_)
        | CBridgeType::FutureHandle
        | CBridgeType::ConstPointer(_)
        | CBridgeType::MutPointer(_)
        | CBridgeType::FunctionPointer { .. } => None,
        _ => Some(native_default(ty)?),
    })
}

pub fn native_default(ty: &CBridgeType) -> Result<String> {
    Ok(match ty {
        CBridgeType::Void => String::new(),
        CBridgeType::Bool => "false".to_owned(),
        CBridgeType::Float32 | CBridgeType::Float64 => "0.0".to_owned(),
        CBridgeType::Int8
        | CBridgeType::Uint8
        | CBridgeType::Int16
        | CBridgeType::Uint16
        | CBridgeType::Int32
        | CBridgeType::Uint32
        | CBridgeType::Int64
        | CBridgeType::Uint64
        | CBridgeType::SignedPointerWidth
        | CBridgeType::PointerWidth
        | CBridgeType::StreamPollResult
        | CBridgeType::WaitResult
        | CBridgeType::CStyleEnum { .. } => "0".to_owned(),
        CBridgeType::FutureHandle
        | CBridgeType::ConstPointer(_)
        | CBridgeType::MutPointer(_)
        | CBridgeType::FunctionPointer { .. } => "$$ffi.nullptr".to_owned(),
        CBridgeType::Status => "$$ffi.Struct.create<_$$BoltFFIStatus>()".to_owned(),
        CBridgeType::Buffer => "$$ffi.Struct.create<_$$BoltFFIBuf>()".to_owned(),
        CBridgeType::String => "$$ffi.Struct.create<_$$BoltFFIString>()".to_owned(),
        CBridgeType::CallbackHandle(_) => "$$ffi.Struct.create<_$$BoltCallbackHandle>()".to_owned(),
        CBridgeType::Named(name) | CBridgeType::DirectRecord(name) => {
            format!("$$ffi.Struct.create<_$${}>()", name.as_str())
        }
        CBridgeType::Span => "$$ffi.Struct.create<_$$BoltFFISpan>()".to_owned(),
    })
}

struct ErrorBinding {
    ty: TypeFragment,
    name: Identifier,
    value: Expression,
}

fn error_catch_binding(ty: &TypeRef, context: &RenderContext<Native>) -> Result<ErrorBinding> {
    Ok(match ty {
        TypeRef::String => ErrorBinding {
            ty: TypeFragment::new("Object"),
            name: Identifier::parse("_l$caught")?,
            value: Expression::new(
                "_l$caught is $$BoltException ? _l$caught.message : _l$caught.toString()",
            ),
        },
        TypeRef::Record(_) | TypeRef::Enum(_) => {
            let ty = type_name::type_ref(ty, context)?;
            ErrorBinding {
                ty,
                name: Identifier::parse("_l$caught")?,
                value: Expression::new("_l$caught"),
            }
        }
        _ => return super::unsupported("Dart callback error payload type"),
    })
}

fn error_throw_expression(
    ty: &TypeRef,
    decoded: &str,
    _: &RenderContext<Native>,
) -> Result<String> {
    Ok(match ty {
        TypeRef::String => format!("$$BoltException({decoded})"),
        TypeRef::Record(_) | TypeRef::Enum(_) => decoded.to_owned(),
        _ => return super::unsupported("Dart callback decoded error type"),
    })
}

fn callback_type(
    id: boltffi_binding::CallbackId,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    type_name::handle(
        &HandleTarget::Callback(id),
        HandlePresence::Required,
        context,
    )
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
