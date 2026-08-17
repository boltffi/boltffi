use boltffi_binding::{
    ClosureParameter as BindingClosureParameter, ErrorDecl, HandlePresence, IntoRust, Native,
    OutgoingParam,
};

use boltffi_binding::CanonicalName;

use crate::{
    bridge::c::{
        CBridgeContract, ClosureParameter as CClosureParameter, Function as CFunction,
        ParameterGroup, Type as CBridgeType,
    },
    core::{Error, HelperId, RenderContext, Result},
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
    type_name,
};

pub struct ClosureArgument {
    pub name: Identifier,
    pub public_type: TypeFragment,
    pub setup: Vec<String>,
    pub arguments: Vec<String>,
    pub helper: Option<(HelperId, String)>,
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
                CallbackParameter::from_declaration(
                    parameter,
                    parameter_group,
                    protocol,
                    bridge,
                    context,
                )
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

        let native_signature = TypeFragment::new(signature.native());
        let native_parameters = native_parameters(protocol, &signature)?;
        let invocation_arguments = parameters
            .iter()
            .map(CallbackParameter::entry_argument)
            .collect::<Vec<_>>()
            .join(", ");
        let source_call = match presence {
            HandlePresence::Nullable => format!("implementation!({invocation_arguments})"),
            _ => format!("implementation({invocation_arguments})"),
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

        let returns = native_return(call_pointer)?;
        let missing = super::callback::method::native_default(returns)?;
        let missing_return = if missing.is_empty() {
            "return;".to_owned()
        } else {
            format!("return {missing};")
        };
        let invoke_body = format!(
            "final implementation = _map[_p$context.address];\n  if (implementation == null) {missing_return}\n{}",
            indent(&invoke_body.join("\n"), 2)
        );
        let error_key = match invoke.error() {
            ErrorDecl::None(_) => "infallible".to_owned(),
            ErrorDecl::EncodedViaReturnSlot { ty, .. } => {
                type_name::type_ref(ty, context)?.to_string()
            }
            _ => return super::super::unsupported("Dart closure error channel"),
        };
        let class = helper_class_name(&format!(
            "{}__{}__{}",
            public_type.as_str(),
            signature.native(),
            error_key
        ));
        let helper_id = HelperId::new(CanonicalName::single(class.clone()));
        let exceptional = exceptional_return_value(returns)?;
        let exceptional_clause = exceptional
            .as_deref()
            .map(|value| format!(", {value}"))
            .unwrap_or_default();
        let invoke_params = native_parameters
            .iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let helper = format!(
            "final class {class} {{\n  static final _map = <int, {public_inner}>{{}};\n  static int _n = 1;\n\n  static int insert({public_inner} value) {{\n    final handle = _n += 2;\n    _map[handle] = value;\n    return handle;\n  }}\n\n  static {invoke_ret} call({invoke_params}) {{\n    {invoke_body}\n  }}\n\n  static void release($$ffi.Pointer<$$ffi.Void> _p$context) {{\n    _map.remove(_p$context.address);\n  }}\n\n  static final callPtr = $$ffi.Pointer.fromFunction<{native_signature}>(call{exceptional_clause});\n  static final _releaseCallable = _$$boltTrackListener($$ffi.NativeCallable<$$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)>.listener(release));\n  static final releasePtr = _releaseCallable.nativeFunction;\n}}\n",
            public_inner = public_type.as_str().trim_end_matches('?'),
            invoke_ret = signature.returns().dart(),
        );
        let handle = format!("_l${source}Handle");
        let (setup, arguments) = match presence {
            HandlePresence::Nullable => (
                vec![format!(
                    "final {handle} = {source} == null ? 0 : {class}.insert({source});"
                )],
                vec![
                    format!("{handle} == 0 ? $$ffi.nullptr : {class}.callPtr"),
                    format!(
                        "{handle} == 0 ? $$ffi.nullptr : $$ffi.Pointer<$$ffi.Void>.fromAddress({handle})"
                    ),
                    format!("{handle} == 0 ? $$ffi.nullptr : {class}.releasePtr"),
                ],
            ),
            _ => (
                vec![format!("final {handle} = {class}.insert({source});")],
                vec![
                    format!("{class}.callPtr"),
                    format!("$$ffi.Pointer<$$ffi.Void>.fromAddress({handle})"),
                    format!("{class}.releasePtr"),
                ],
            ),
        };
        Ok(Self {
            name: source,
            public_type,
            setup,
            arguments,
            helper: Some((helper_id, helper)),
        })
    }
}

fn helper_class_name(signature: &str) -> String {
    let sanitized = signature
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    format!("_Cl${sanitized}")
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
