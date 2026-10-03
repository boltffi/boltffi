use boltffi_binding::{FunctionDecl, Native};

use crate::{
    core::Result,
    target::python::{
        name_style::Name,
        syntax::{CallExpression, Expression, Identifier, Statement, TypeAnnotation},
    },
};

use super::super::{Documentation, Package};
use super::{
    body::CallableBody,
    parameter::{ParameterStub, Parameters},
    return_value::ReturnStub,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionStub {
    pub documentation: Documentation,
    pub python_name: Identifier,
    pub parameters: Parameters,
    pub return_annotation: TypeAnnotation,
    pub asynchronous: bool,
    pub body: Vec<Statement>,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
}

impl FunctionStub {
    pub fn from_declaration(function: &FunctionDecl<Native>, package: &Package) -> Result<Self> {
        let parameters = Parameters::from_declarations(function.callable().params(), package)?;
        let returned = ReturnStub::from_callable(function.callable(), package)?;
        let native_name = Name::new(function.name()).function()?;
        let native_call = Expression::call(parameters.iter().fold(
            CallExpression::new(Expression::attribute(
                Expression::identifier(Identifier::parse("_native")?),
                native_name.clone(),
            )),
            |call, parameter| call.positional(parameter.argument.clone()),
        ));
        let body = CallableBody::from_callable(
            function.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            documentation: Documentation::new(function.meta().doc()),
            python_name: Name::new(function.name()).function()?,
            return_annotation: returned.into_annotation(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(&parameters),
            uses_wire_helpers,
            parameters,
        })
    }
}

impl FunctionStub {
    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.uses_async_helpers
    }

    pub fn uses_sequence_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_sequence_annotation)
    }

    pub fn uses_callable_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_callable_annotation)
    }

    pub fn validate_names(&self) -> Result<()> {
        self.parameters
            .scope(format!("function `{}`", self.python_name))
            .map(|_| ())
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.python_name.to_string(),
            format!("function `{}`", self.python_name),
        )
    }
}
