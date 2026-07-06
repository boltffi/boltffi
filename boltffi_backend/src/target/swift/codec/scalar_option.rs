use boltffi_binding::Primitive;

use crate::{
    core::Result,
    target::swift::{
        name_style::Name,
        primitive::SwiftPrimitive,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

use super::ArgumentBuffer;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScalarOption {
    primitive: Primitive,
}

impl ScalarOption {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }

    pub fn ty(self) -> Result<TypeName> {
        SwiftPrimitive::new(self.primitive)
            .api_type()
            .map(TypeName::optional)
    }

    pub fn write(self, name: &Name, value: Expression) -> Result<ArgumentBuffer> {
        let buffer = ArgumentBuffer::new(name)?;
        let writer = buffer.writer().clone();
        Ok(buffer.with_statement(self.write_statement(writer, value)?))
    }

    pub fn read(self, reader: Identifier) -> Result<Expression> {
        let parameter = Identifier::parse("reader")?;
        Ok(Expression::trailing_closure(
            Expression::member(reader, "readOptional"),
            ArgumentList::default(),
            parameter.clone(),
            self.read_value(parameter)?,
        ))
    }

    fn read_value(self, reader: Identifier) -> Result<Expression> {
        SwiftPrimitive::new(self.primitive).read_expression(reader)
    }

    pub fn write_statement(self, writer: Identifier, value: Expression) -> Result<Statement> {
        let parameter_writer = Identifier::parse("writer")?;
        let parameter_value = Identifier::parse("value")?;
        Ok(Statement::expression(
            Expression::trailing_closure_parameters(
                Expression::member(writer.clone(), "writeOptional"),
                [value].into_iter().collect::<ArgumentList>(),
                [parameter_writer.clone(), parameter_value.clone()],
                SwiftPrimitive::new(self.primitive)
                    .write_statement(parameter_writer, Expression::identifier(parameter_value))?,
            ),
        ))
    }
}
