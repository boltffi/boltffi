use boltffi_binding::{DefaultValue, IncomingParam, IntoRust, Native, ParamDecl};

use crate::core::{RenderContext, Result};

use super::super::{
    name_style::Namespace,
    syntax::{Expression, Identifier, TypeFragment},
    unsupported,
};
use super::default_value::DefaultExpression;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: TypeFragment,
    pub default: Option<DefaultExpression>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Overload {
    pub parameters: Vec<Parameter>,
    pub arguments: Vec<Expression>,
}

impl Parameter {
    pub fn default_for(
        parameter: &ParamDecl<Native, IntoRust>,
        namespace: Option<&Namespace>,
        context: &RenderContext<Native>,
    ) -> Result<Option<DefaultExpression>> {
        let Some(value) = parameter.meta().default() else {
            return Ok(None);
        };
        match parameter.payload() {
            IncomingParam::Value(plan) => {
                let Some(ty) = plan.value_type() else {
                    return unsupported("default value for this parameter type");
                };
                DefaultExpression::render(&ty, value, namespace, context).map(Some)
            }
            IncomingParam::Closure(_) if matches!(value, DefaultValue::Null) => {
                Ok(Some(DefaultExpression::Constant(Expression::new("null"))))
            }
            IncomingParam::Closure(_) => unsupported("non-null closure default"),
        }
    }

    pub fn declarations(parameters: &[Self]) -> Vec<String> {
        let last_required = parameters.iter().rposition(|parameter| {
            parameter
                .default
                .as_ref()
                .and_then(DefaultExpression::constant)
                .is_none()
        });
        parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let declaration = format!("{} {}", parameter.ty, parameter.name);
                let Some(value) = parameter.default.as_ref().and_then(DefaultExpression::constant)
                else {
                    return declaration;
                };
                if last_required.is_some_and(|required| index < required) {
                    format!("[global::System.Runtime.InteropServices.Optional, global::System.Runtime.InteropServices.DefaultParameterValue({value})] {declaration}")
                } else {
                    format!("{declaration} = {value}")
                }
            })
            .collect()
    }

    fn same_overload_type(&self, other: &Self) -> bool {
        if self.ty == other.ty {
            return true;
        }
        let reference_type = [self, other].iter().any(|parameter| {
            parameter
                .default
                .as_ref()
                .is_some_and(DefaultExpression::constructs_reference)
        });
        reference_type
            && self.ty.to_string().trim_end_matches('?')
                == other.ty.to_string().trim_end_matches('?')
    }
}

impl Overload {
    pub fn from_parameters(parameters: &[Parameter]) -> Result<Vec<Self>> {
        let mut runtime_defaults =
            parameters
                .iter()
                .enumerate()
                .filter_map(|(index, parameter)| {
                    parameter
                        .default
                        .as_ref()
                        .and_then(DefaultExpression::runtime)
                        .map(|value| (index, parameter, value))
                });
        match runtime_defaults.clone().count() {
            0 => return Ok(Vec::new()),
            1..=8 => {}
            _ => {
                return unsupported(
                    "callables with more than 8 runtime defaults require too many C# overloads",
                );
            }
        }
        let mut overloads = vec![Self {
            parameters: parameters.to_vec(),
            arguments: parameters
                .iter()
                .map(|parameter| Expression::identifier(parameter.name.clone()))
                .collect(),
        }];
        runtime_defaults.try_for_each(|(index, parameter, value)| {
            let omitted = overloads
                .iter()
                .map(|overload| {
                    let mut arguments = overload.arguments.clone();
                    arguments[index] = value.clone();
                    Self {
                        parameters: overload
                            .parameters
                            .iter()
                            .filter(|included| included.name != parameter.name)
                            .cloned()
                            .collect(),
                        arguments,
                    }
                })
                .collect::<Vec<_>>();
            if omitted.iter().enumerate().any(|(index, candidate)| {
                omitted[..index].iter().chain(&overloads).any(|existing| {
                    candidate.parameters.len() == existing.parameters.len()
                        && candidate
                            .parameters
                            .iter()
                            .zip(&existing.parameters)
                            .all(|(candidate, existing)| candidate.same_overload_type(existing))
                })
            }) {
                return unsupported("runtime defaults create indistinguishable C# overloads");
            }
            overloads.extend(omitted);
            Ok(())
        })?;
        Ok(overloads.into_iter().skip(1).collect())
    }

    pub fn parameter_declarations(&self) -> Vec<String> {
        Parameter::declarations(&self.parameters)
    }
}
