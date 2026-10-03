use boltffi_binding::{ExportedMethodDecl, InitializerDecl, Native, NativeSymbol};

use crate::{
    core::Result,
    target::python::{
        cpython::render::class as class_render,
        name_style::Name,
        syntax::{ArgumentList, CallExpression, Expression, Identifier, Statement, TypeAnnotation},
    },
};

use super::{
    super::{Documentation, Package},
    body::CallableBody,
    parameter::{ParameterStub, Parameters},
    return_value::{ReturnStub, ReturnedValue},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssociatedCallable {
    pub documentation: Documentation,
    pub receiver: bool,
    pub python_name: Identifier,
    pub native_name: Identifier,
    pub parameters: Parameters,
    pub arguments: ArgumentList,
    pub return_annotation: TypeAnnotation,
    pub asynchronous: bool,
    pub body: Vec<Statement>,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
}

impl AssociatedCallable {
    pub fn from_class_initializer(
        initializer: &InitializerDecl<Native>,
        symbols: &class_render::Symbols,
        package: &Package,
    ) -> Result<Self> {
        let documentation = Documentation::new(initializer.meta().doc());
        let parameters = Parameters::from_declarations(initializer.callable().params(), package)?;
        let arguments = Self::arguments(None, &parameters);
        let native_name = symbols.initializer(initializer.name())?;
        let native_call = Self::native_call(&native_name, None, &parameters)?;
        let returned = ReturnedValue::class_handle(symbols.class_name().clone());
        let body = CallableBody::from_callable(
            initializer.callable(),
            &native_name,
            native_call,
            &returned,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers);
        Ok(Self {
            documentation,
            receiver: false,
            python_name: Name::new(initializer.name()).function()?,
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(&parameters),
            native_name,
            arguments,
            return_annotation: TypeAnnotation::identifier(symbols.class_name().clone()),
            parameters,
            uses_wire_helpers,
        })
    }

    pub fn from_class_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        symbols: &class_render::Symbols,
        package: &Package,
    ) -> Result<Self> {
        let documentation = Documentation::new(method.meta().doc());
        let receiver = method.callable().receiver().is_some();
        let parameters = Parameters::from_declarations(method.callable().params(), package)?;
        let returned = ReturnStub::from_callable(method.callable(), package)?;
        let receiver_argument = receiver.then(Self::self_receiver).transpose()?;
        let arguments = Self::arguments(receiver_argument.clone(), &parameters);
        let native_name = symbols.method(method.name())?;
        let native_call = Self::native_call(&native_name, receiver_argument, &parameters)?;
        let body = CallableBody::from_callable(
            method.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            documentation,
            receiver,
            python_name: Name::new(method.name()).function()?,
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(&parameters),
            native_name,
            arguments,
            parameters,
            return_annotation: returned.into_annotation(),
            uses_wire_helpers,
        })
    }

    pub fn from_value_initializer(
        initializer: &InitializerDecl<Native>,
        native_name: Identifier,
        package: &Package,
    ) -> Result<Self> {
        let documentation = Documentation::new(initializer.meta().doc());
        let parameters = Parameters::from_declarations(initializer.callable().params(), package)?;
        let returned = ReturnStub::from_callable(initializer.callable(), package)?;
        let arguments = Self::arguments(None, &parameters);
        let native_call = Self::native_call(&native_name, None, &parameters)?;
        let body = CallableBody::from_callable(
            initializer.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            documentation,
            receiver: false,
            python_name: Name::new(initializer.name()).function()?,
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(&parameters),
            native_name,
            arguments,
            return_annotation: returned.into_annotation(),
            parameters,
            uses_wire_helpers,
        })
    }

    pub fn from_value_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        native_name: Identifier,
        receiver: Option<Expression>,
        mutated_receiver_type: Option<TypeAnnotation>,
        package: &Package,
    ) -> Result<Self> {
        let documentation = Documentation::new(method.meta().doc());
        let parameters = Parameters::from_declarations(method.callable().params(), package)?;
        let returned = match mutated_receiver_type {
            Some(annotation) => ReturnStub::native(annotation),
            None => ReturnStub::from_callable(method.callable(), package)?,
        };
        let arguments = Self::arguments(receiver.clone(), &parameters);
        let native_call = Self::native_call(&native_name, receiver.clone(), &parameters)?;
        let body = CallableBody::from_callable(
            method.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            documentation,
            receiver: receiver.is_some(),
            python_name: Name::new(method.name()).function()?,
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(&parameters),
            native_name,
            arguments,
            parameters,
            return_annotation: returned.into_annotation(),
            uses_wire_helpers,
        })
    }
}

impl AssociatedCallable {
    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.uses_async_helpers
    }

    pub fn is_async(&self) -> bool {
        self.asynchronous
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

    pub fn validate_names(&self, owner: &Identifier) -> Result<()> {
        self.parameters
            .scope(format!("method `{}.{}`", owner, self.python_name))
            .map(|_| ())
    }

    pub fn member_name(&self) -> (String, String) {
        (
            self.python_name.to_string(),
            format!("method `{}`", self.python_name),
        )
    }

    fn arguments(receiver: Option<Expression>, parameters: &Parameters) -> ArgumentList {
        ArgumentList::from_iter(
            receiver.into_iter().chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.argument.clone()),
            ),
        )
    }

    fn native_call(
        native_name: &Identifier,
        receiver: Option<Expression>,
        parameters: &Parameters,
    ) -> Result<Expression> {
        Ok(Expression::call(
            Self::argument_expressions(receiver, parameters)
                .into_iter()
                .fold(
                    CallExpression::new(Expression::attribute(
                        Expression::identifier(Identifier::parse("_native")?),
                        native_name.clone(),
                    )),
                    CallExpression::positional,
                ),
        ))
    }

    fn argument_expressions(
        receiver: Option<Expression>,
        parameters: &Parameters,
    ) -> Vec<Expression> {
        receiver
            .into_iter()
            .chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.argument.clone()),
            )
            .collect()
    }

    fn self_receiver() -> Result<Expression> {
        Identifier::parse("self").map(Expression::identifier)
    }
}
