use crate::{
    bridge::{
        c::{self, Expression, Identifier, Statement, TypeFragment},
        jni::ClosureCParameter,
    },
    core::Result,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureRecordArgument {
    parameter: ClosureCParameter,
    array: Identifier,
    value: Identifier,
}

impl ClosureRecordArgument {
    pub(in crate::bridge::jni::contract::closure) fn from_parameter(
        parameter: &c::Parameter,
    ) -> Result<Self> {
        let parameter = ClosureCParameter::from_parameter(parameter)?;
        Ok(Self {
            array: Identifier::parse(format!("__boltffi_{}_array", parameter.name()))?,
            value: Identifier::parse(format!("__boltffi_{}_value", parameter.name()))?,
            parameter,
        })
    }

    pub fn parameter(&self) -> &ClosureCParameter {
        &self.parameter
    }

    pub fn array(&self) -> &Identifier {
        &self.array
    }

    pub fn value(&self) -> &Identifier {
        &self.value
    }

    pub fn value_declaration(&self) -> Statement {
        Statement::new(format!("{} {} = {{0}};", self.parameter.ty(), self.value))
    }

    pub fn c_parameters(&self) -> Vec<ClosureCParameter> {
        vec![self.parameter.clone()]
    }

    pub fn handle_parameters(&self) -> Vec<ClosureCParameter> {
        vec![ClosureCParameter::new(
            self.parameter.name().clone(),
            TypeFragment::new("jbyteArray"),
        )]
    }

    pub fn jvm_arguments(&self) -> Vec<Expression> {
        vec![Expression::identifier(self.array.clone())]
    }

    pub fn rust_arguments(&self) -> Vec<Expression> {
        vec![Expression::identifier(self.value.clone())]
    }

    pub const fn jni_signature(&self) -> &'static str {
        "[B"
    }
}
