use boltffi_binding::{ExecutionDecl, ImportedMethodDecl, Native, VTableSlot};

use crate::core::Result;

use super::super::{
    Field, Identifier, Parameter, ParameterGroup, ParameterIndex, Type, function::Signature,
    names::Names,
};

/// One method slot in a native callback vtable.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackSlot {
    name: Identifier,
    returns: Type,
    parameters: Vec<Parameter>,
    parameter_groups: Vec<ParameterGroup>,
    return_parameter_groups: Vec<ParameterGroup>,
    source_parameter_groups: Vec<ParameterGroup>,
}

impl CallbackSlot {
    /// Returns the callback slot name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C return type for this callback slot.
    pub fn returns(&self) -> &Type {
        &self.returns
    }

    /// Returns the parameters in C ABI order.
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// Returns source-level parameter groups in declaration order.
    pub fn parameter_groups(&self) -> &[ParameterGroup] {
        &self.parameter_groups
    }

    /// Returns return-value groups in C ABI order.
    pub fn return_parameter_groups(&self) -> &[ParameterGroup] {
        &self.return_parameter_groups
    }

    /// Returns source-parameter groups in source declaration order.
    pub fn source_parameter_groups(&self) -> &[ParameterGroup] {
        &self.source_parameter_groups
    }

    /// Returns the C ABI parameter at the given position.
    pub fn parameter(&self, index: ParameterIndex) -> &Parameter {
        &self.parameters[index.position()]
    }
}

impl CallbackSlot {
    pub(in crate::bridge::c::callback) fn from_method(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        names: &Names,
    ) -> Result<Self> {
        let signature = Signature::new(names, Vec::new());
        if matches!(
            method.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Self::async_method(method, &signature);
        }
        let return_parameters = signature.callback_return_params(
            method.callable().returns().plan(),
            method.callable().error(),
        )?;
        let method_parameters = signature.imported_params(method.callable().params())?;
        let return_group_count = ParameterGroup::from_params(&return_parameters)?.len();
        let source_group_count = ParameterGroup::from_params(&method_parameters)?.len();
        let parameters = std::iter::once(Parameter::new("handle", Type::Uint64)?)
            .chain(return_parameters)
            .chain(method_parameters)
            .collect();
        Self::new(
            Identifier::escape(method.target().as_str())?,
            signature.callback_return_type(
                method.callable().returns().plan(),
                method.callable().error(),
            )?,
            parameters,
            return_group_count,
            source_group_count,
        )
    }

    pub(in crate::bridge::c::callback) fn field(&self) -> Field {
        Field::from_parts(
            self.name.clone(),
            Type::FunctionPointer {
                returns: Box::new(self.returns.clone()),
                params: self
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty().clone())
                    .collect(),
            },
        )
    }

    fn async_method(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        signature: &Signature,
    ) -> Result<Self> {
        let method_parameters = signature.imported_params(method.callable().params())?;
        let source_group_count = ParameterGroup::from_params(&method_parameters)?.len();
        let completion = signature.async_completion(
            method.callable().returns().plan(),
            method.callable().error(),
        )?;
        let parameters = std::iter::once(Parameter::new("handle", Type::Uint64)?)
            .chain(method_parameters)
            .chain([
                Parameter::callback_completion("complete", completion)?,
                Parameter::callback_completion_context("complete")?,
            ])
            .collect();
        Self::new(
            Identifier::escape(method.target().as_str())?,
            Type::Void,
            parameters,
            0,
            source_group_count,
        )
    }

    fn new(
        name: Identifier,
        returns: Type,
        parameters: Vec<Parameter>,
        return_group_count: usize,
        source_group_count: usize,
    ) -> Result<Self> {
        let parameter_groups = ParameterGroup::from_params(&parameters)?;
        let return_parameter_groups = parameter_groups
            .iter()
            .skip(1)
            .take(return_group_count)
            .cloned()
            .collect();
        let source_parameter_groups = parameter_groups
            .iter()
            .skip(1 + return_group_count)
            .take(source_group_count)
            .cloned()
            .collect();
        Ok(Self {
            name,
            returns,
            parameters,
            parameter_groups,
            return_parameter_groups,
            source_parameter_groups,
        })
    }
}
