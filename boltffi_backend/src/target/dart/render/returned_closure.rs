use askama::Template;
use boltffi_binding::{
    ClosureReturn as BindingClosureReturn, HandlePresence, IncomingParam, Native, OutOfRust,
};

use crate::{
    bridge::c::{CBridgeContract, ClosureReturnParameter},
    core::{Error, RenderContext, Result},
};

use super::{
    function::{render_parameter, render_return},
    indent,
};
use crate::target::dart::{
    name_style::Name,
    native::NativeFunctionSignature,
    syntax::{Identifier, Parameter, TypeFragment},
};

#[derive(Template)]
#[template(path = "target/dart/returned_closure.dart", escape = "none")]
struct ReturnedClosureRegistrationTemplate<'a> {
    registration: &'a ReturnedClosureRegistration,
}

struct ReturnedClosureRegistration {
    presence: HandlePresence,
    storage: Identifier,
    registration: Identifier,
    returned: Identifier,
    owner: Identifier,
    parameters: Vec<Parameter>,
    statements: Vec<String>,
}

pub struct ReturnedClosure {
    pub public_type: TypeFragment,
    pub before_call: Vec<String>,
    pub arguments: Vec<String>,
    pub after_call: Vec<String>,
    pub expression: String,
}

impl ReturnedClosure {
    pub fn from_declaration(
        closure: &BindingClosureReturn<Native, OutOfRust>,
        protocol: &ClosureReturnParameter,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let invoke = closure.invoke();
        if invoke.params().len() > protocol.parameter_groups().len() {
            return broken("Dart returned closure parameter groups are incomplete");
        }
        let (parameter_groups, return_groups) =
            protocol.parameter_groups().split_at(invoke.params().len());
        let parameters = invoke
            .params()
            .iter()
            .zip(parameter_groups)
            .map(|(parameter, group)| match parameter.payload() {
                IncomingParam::Value(plan) => render_parameter(
                    Name::new(parameter.name()).lower_camel()?,
                    plan,
                    group,
                    protocol,
                    bridge,
                    context,
                ),
                IncomingParam::Closure(_) => {
                    super::super::unsupported("nested returned-closure parameter")
                }
            })
            .collect::<Result<Vec<_>>>()?;
        if parameters
            .iter()
            .any(|parameter| !parameter.writeback().is_empty())
        {
            return super::super::unsupported("mutable returned-closure parameter");
        }

        let returns = render_return(
            invoke.returns().plan(),
            invoke.error(),
            protocol,
            return_groups,
            bridge,
            context,
        )?;
        let closure_type = TypeFragment::function(
            returns.public_type.clone(),
            parameters
                .iter()
                .map(|parameter| parameter.public_type().clone()),
        );
        let presence = match closure.presence() {
            HandlePresence::Required => HandlePresence::Required,
            HandlePresence::Nullable => HandlePresence::Nullable,
            _ => return super::super::unsupported("unknown returned-closure presence"),
        };
        let public_type = match presence {
            HandlePresence::Nullable => closure_type.optional_function(),
            _ => closure_type,
        };
        let native_signature = NativeFunctionSignature::from_pointer(protocol.call_type())?;
        let storage = Identifier::parse("_l$returnedClosureStorage")?;
        let registration = Identifier::parse("_l$returnedClosureRegistration")?;
        let returned = Identifier::parse("_l$returnedClosure")?;
        let owner = Identifier::parse("_l$returnedClosureOwner")?;
        let native_invoke = Identifier::parse("_l$returnedClosureInvoke")?;
        let mut body = parameters
            .iter()
            .flat_map(|parameter| parameter.setup().iter().cloned())
            .collect::<Vec<_>>();
        body.extend(returns.before_call.iter().cloned());
        body.push(format!(
            "final {native_invoke} = {owner}.invoke.cast<$$ffi.NativeFunction<{}>>().asFunction<{}>();",
            native_signature.native(),
            native_signature.dart(),
        ));

        let mut native_arguments = std::iter::once(format!("{owner}.context"))
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.native_arguments().iter().cloned()),
            )
            .collect::<Vec<_>>();
        native_arguments.extend(returns.arguments.iter().cloned());
        let invocation = format!("{native_invoke}({})", native_arguments.join(", "));
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup().iter().cloned())
            .collect::<Vec<_>>();

        let mut finally = returns.finally.clone();
        finally.extend(cleanup.iter().cloned());

        if finally.is_empty() {
            body.push(match &returns.call_result {
                Some(result) => format!("final {result} = {invocation};"),
                None => format!("{invocation};"),
            });
            body.extend(returns.after_call.iter().cloned());
            if let Some(expression) = &returns.expression {
                body.push(format!("return {expression};"));
            }
        } else {
            // `after_call` can throw; pooled arg/return storage still releases.
            let mut inner = vec![match &returns.call_result {
                Some(result) => format!("final {result} = {invocation};"),
                None => format!("{invocation};"),
            }];
            inner.extend(returns.after_call.iter().cloned());

            match &returns.expression {
                Some(expression) => {
                    body.push(format!(
                        "late final {} _l$callResult;",
                        returns.public_type.as_str()
                    ));
                    inner.push(format!("_l$callResult = {expression};"));
                    body.push(format!(
                        "try {{\n{}\n}} finally {{\n{}\n}}",
                        indent(&inner.join("\n"), 2),
                        indent(&finally.join("\n"), 2),
                    ));
                    body.push("return _l$callResult;".to_owned());
                }
                None => {
                    body.push(format!(
                        "try {{\n{}\n}} finally {{\n{}\n}}",
                        indent(&inner.join("\n"), 2),
                        indent(&finally.join("\n"), 2),
                    ));
                }
            }
        }

        let registration = ReturnedClosureRegistration {
            presence,
            storage,
            registration,
            returned,
            owner,
            parameters: parameters
                .iter()
                .map(|parameter| parameter.signature().clone())
                .collect(),
            statements: body,
        };
        let after_call = vec![registration.source()];

        Ok(Self {
            public_type,
            before_call: vec![format!(
                "final {} = _$$BoltCallocPtr<_$$BoltReturnedClosureRegistration>.alloc($$ffi.sizeOf<_$$BoltReturnedClosureRegistration>());",
                registration.storage
            )],
            arguments: vec![format!("{}.ptr.cast<$$ffi.Void>()", registration.storage)],
            after_call,
            expression: registration.returned.to_string(),
        })
    }
}

impl ReturnedClosureRegistration {
    fn source(&self) -> String {
        ReturnedClosureRegistrationTemplate { registration: self }
            .render()
            .expect("rendering an in-memory Dart returned-closure registration cannot fail")
    }

    fn nullable(&self) -> bool {
        self.presence == HandlePresence::Nullable
    }

    fn body(&self) -> String {
        indent(&self.statements.join("\n"), 2)
    }

    fn nested_body(&self) -> String {
        indent(&self.statements.join("\n"), 4)
    }
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
