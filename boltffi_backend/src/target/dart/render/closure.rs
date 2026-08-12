use askama::Template;
use boltffi_binding::{
    ClosureParameter as BindingClosureParameter, ErrorDecl, HandlePresence, IntoRust, Native,
    OutgoingParam,
};

use crate::{
    bridge::c::{
        CBridgeContract, ClosureParameter as CClosureParameter, Function as CFunction,
        ParameterGroup, Type as CBridgeType,
    },
    core::{Error, RenderContext, Result},
};

use super::{
    callback::{
        method::{
            exceptional_return_value, public_return_type, render_fallible_entry_return,
            render_infallible_entry_return,
        },
        parameter::CallbackParameter,
    },
    indent,
};
use crate::target::dart::{
    name_style::Name,
    native::NativeFunctionSignature,
    syntax::{Identifier, Parameter, TypeFragment},
};

#[derive(Template)]
#[template(path = "target/dart/closure_argument.dart", escape = "none")]
struct ClosureRegistrationTemplate<'a> {
    registration: &'a ClosureRegistration,
}

struct ClosureRegistration {
    source: Identifier,
    presence: HandlePresence,
    call_callable: Identifier,
    release_callable: Identifier,
    invoke_function: Identifier,
    release_function: Identifier,
    callable_type: TypeFragment,
    release_callable_type: TypeFragment,
    invoke_return: TypeFragment,
    invoke_parameters: Vec<Parameter>,
    invoke_body: String,
    native_signature: TypeFragment,
    release_signature: TypeFragment,
    exceptional_return: Option<String>,
}

pub struct ClosureArgument {
    pub name: Identifier,
    pub public_type: TypeFragment,
    pub setup: Vec<String>,
    pub arguments: Vec<String>,
}

impl ClosureArgument {
    #[allow(clippy::too_many_arguments)]
    pub fn from_declaration(
        source_name: &boltffi_binding::CanonicalName,
        closure: &BindingClosureParameter<Native, IntoRust>,
        protocol: &CClosureParameter,
        function: &CFunction,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source = Name::new(source_name).lower_camel()?;
        let invoke = closure.invoke();
        if invoke.params().len() > protocol.parameter_groups().len() {
            return broken("Dart closure parameter groups are incomplete");
        }
        let (parameter_groups, return_groups) =
            protocol.parameter_groups().split_at(invoke.params().len());
        let parameters = invoke
            .params()
            .iter()
            .zip(parameter_groups)
            .map(|(parameter, parameter_group)| {
                let OutgoingParam::Value(_) = parameter.payload() else {
                    return super::super::unsupported("nested closure parameter");
                };
                CallbackParameter::from_declaration(parameter, parameter_group, protocol, context)
            })
            .collect::<Result<Vec<_>>>()?;

        let call_pointer = function.parameter(protocol.call()).ty();
        let signature = NativeFunctionSignature::from_pointer(call_pointer)?;
        let CBridgeType::FunctionPointer { params, .. } = call_pointer else {
            return broken("Dart closure call lane is not a C function pointer");
        };
        if params.len() != 1 + group_parameters(protocol).len() {
            return broken("Dart closure call signature disagrees with its parameter contract");
        }

        let public_return = public_return_type(invoke.returns().plan(), context)?;
        let closure_type = TypeFragment::function(
            public_return,
            parameters
                .iter()
                .map(|parameter| parameter.public_type().clone()),
        );
        let presence = match closure.presence() {
            HandlePresence::Required => HandlePresence::Required,
            HandlePresence::Nullable => HandlePresence::Nullable,
            _ => return super::super::unsupported("unknown closure presence"),
        };
        let public_type = match presence {
            HandlePresence::Nullable => closure_type.optional_function(),
            _ => closure_type,
        };

        let call_callable = Identifier::parse(format!("_l${source}Call"))?;
        let release_callable = Identifier::parse(format!("_l${source}Release"))?;
        let invoke_function = Identifier::parse(format!("_l${source}Invoke"))?;
        let release_function = Identifier::parse(format!("_l${source}Drop"))?;
        let native_signature = TypeFragment::new(signature.native());
        let release_signature = TypeFragment::new("$$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)");
        let native_parameters = native_parameters(protocol, &signature)?;
        let invocation_arguments = parameters
            .iter()
            .map(CallbackParameter::entry_argument)
            .collect::<Vec<_>>()
            .join(", ");
        let source_call = match presence {
            HandlePresence::Nullable => format!("{source}!({invocation_arguments})"),
            _ => format!("{source}({invocation_arguments})"),
        };
        let mut invoke_body = parameters
            .iter()
            .flat_map(|parameter| parameter.entry_setup().iter().cloned())
            .collect::<Vec<_>>();
        match invoke.error() {
            ErrorDecl::None(_) => invoke_body.extend(render_infallible_entry_return(
                invoke.returns().plan(),
                &source_call,
                bridge,
                context,
            )?),
            ErrorDecl::EncodedViaReturnSlot { ty, codec, .. } => {
                invoke_body.extend(render_fallible_entry_return(
                    invoke.returns().plan(),
                    ty,
                    codec,
                    return_groups,
                    protocol,
                    &source_call,
                    bridge,
                    context,
                )?)
            }
            _ => return super::super::unsupported("Dart closure error channel"),
        }

        let callable_type = match presence {
            HandlePresence::Nullable => {
                TypeFragment::new(format!("$$ffi.NativeCallable<{native_signature}>")).optional()
            }
            _ => TypeFragment::new(format!("$$ffi.NativeCallable<{native_signature}>")),
        };
        let release_callable_type = match presence {
            HandlePresence::Nullable => {
                TypeFragment::new(format!("$$ffi.NativeCallable<{release_signature}>")).optional()
            }
            _ => TypeFragment::new(format!("$$ffi.NativeCallable<{release_signature}>")),
        };
        let exceptional_return = exceptional_return_value(native_return(call_pointer)?)?;
        let registration = ClosureRegistration {
            source,
            presence,
            call_callable,
            release_callable,
            invoke_function,
            release_function,
            callable_type,
            release_callable_type,
            invoke_return: TypeFragment::new(signature.returns().dart()),
            invoke_parameters: native_parameters,
            invoke_body: indent(&invoke_body.join("\n"), 2),
            native_signature,
            release_signature,
            exceptional_return,
        };
        let setup = vec![registration.source()];

        let call_argument = match registration.presence {
            HandlePresence::Nullable => {
                format!(
                    "{}?.nativeFunction ?? $$ffi.nullptr",
                    registration.call_callable
                )
            }
            _ => format!("{}.nativeFunction", registration.call_callable),
        };
        let release_argument = match registration.presence {
            HandlePresence::Nullable => format!(
                "{}?.nativeFunction ?? $$ffi.nullptr",
                registration.release_callable
            ),
            _ => format!("{}.nativeFunction", registration.release_callable),
        };
        Ok(Self {
            name: registration.source,
            public_type,
            setup,
            arguments: vec![call_argument, "$$ffi.nullptr".to_owned(), release_argument],
        })
    }
}

impl ClosureRegistration {
    fn source(&self) -> String {
        ClosureRegistrationTemplate { registration: self }
            .render()
            .expect("rendering an in-memory Dart closure registration cannot fail")
    }

    fn nullable(&self) -> bool {
        self.presence == HandlePresence::Nullable
    }
}

fn native_parameters(
    closure: &CClosureParameter,
    signature: &NativeFunctionSignature,
) -> Result<Vec<Parameter>> {
    let names = std::iter::once(Identifier::parse("_p$context"))
        .chain(group_parameters(closure).iter().map(|parameter| {
            crate::target::dart::native::parameter_name(parameter.name())
                .and_then(Identifier::parse)
        }))
        .collect::<Result<Vec<_>>>()?;
    if names.len() != signature.parameters().len() {
        return broken("Dart closure parameter names disagree with its native signature");
    }
    signature
        .parameters()
        .iter()
        .zip(names)
        .map(|(ty, name)| Ok(Parameter::new(name, TypeFragment::new(ty.dart()))))
        .collect()
}

fn group_parameters(closure: &CClosureParameter) -> Vec<&crate::bridge::c::Parameter> {
    closure
        .parameter_groups()
        .iter()
        .flat_map(parameter_group_indices)
        .map(|index| closure.parameter(index))
        .collect()
}

fn parameter_group_indices(
    group: &ParameterGroup,
) -> impl Iterator<Item = crate::bridge::c::ParameterIndex> + '_ {
    let indices = match group {
        ParameterGroup::Value(index)
        | ParameterGroup::SuccessOut(index)
        | ParameterGroup::CompletionStatusOut(index) => vec![*index],
        ParameterGroup::ByteSlice(bytes) => vec![bytes.pointer(), bytes.length()],
        ParameterGroup::DirectVector(vector) => vec![vector.pointer(), vector.length()],
        _ => Vec::new(),
    };
    indices.into_iter()
}

fn native_return(call_pointer: &CBridgeType) -> Result<&CBridgeType> {
    match call_pointer {
        CBridgeType::FunctionPointer { returns, .. } => Ok(returns),
        _ => broken("Dart closure call lane is not a C function pointer"),
    }
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
