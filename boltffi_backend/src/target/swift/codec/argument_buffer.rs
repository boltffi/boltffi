use crate::{
    core::Result,
    target::swift::{
        name_style::Name,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArgumentBuffer {
    bytes: Identifier,
    buffer: Identifier,
    writer: Identifier,
    statements: Vec<Statement>,
}

impl ArgumentBuffer {
    pub fn new(name: &Name) -> Result<Self> {
        Ok(Self {
            bytes: name.generated("bytes")?,
            buffer: name.generated("buffer")?,
            writer: name.generated("writer")?,
            statements: Vec::new(),
        })
    }

    pub fn from_parts(bytes: Identifier, buffer: Identifier, writer: Identifier) -> Self {
        Self {
            bytes,
            buffer,
            writer,
            statements: Vec::new(),
        }
    }

    pub fn with_statements(mut self, statements: Vec<Statement>) -> Self {
        self.statements = statements;
        self
    }

    pub fn with_statement(mut self, statement: Statement) -> Self {
        self.statements.push(statement);
        self
    }

    pub fn writer(&self) -> &Identifier {
        &self.writer
    }

    pub fn arguments(&self) -> Vec<Expression> {
        vec![
            Expression::forced(Expression::member(&self.buffer, "baseAddress")),
            Expression::call(
                TypeName::uint(),
                [Expression::member(&self.buffer, "count")]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        ]
    }

    pub fn bytes_statement(&self) -> Statement {
        Statement::let_value(&self.bytes, self.encode_call())
    }

    pub fn effect_scope(&self, body: Statement, indent: &str, throwing: bool) -> String {
        Statement::discarding_unsafe_buffer_scope(&self.bytes, &self.buffer, body, indent, throwing)
    }

    pub fn unsafe_buffer_scope(&self, body: Statement, indent: &str) -> String {
        Statement::unsafe_buffer_scope(&self.bytes, &self.buffer, body, indent)
    }

    pub fn returning_scope(&self, body: Statement, indent: &str, throwing: bool) -> String {
        Statement::returning_unsafe_buffer_scope(&self.bytes, &self.buffer, body, indent, throwing)
    }

    pub fn binding_scope(
        &self,
        binding: &Identifier,
        body: Statement,
        indent: &str,
        throwing: bool,
    ) -> String {
        Statement::binding_unsafe_buffer_scope(
            &self.bytes,
            &self.buffer,
            binding,
            body,
            indent,
            throwing,
        )
    }

    pub fn copy_expression(&self, copy: &Identifier) -> Expression {
        Expression::call(
            copy,
            [
                Expression::member(&self.buffer, "baseAddress"),
                Expression::call(
                    TypeName::uint(),
                    [Expression::member(&self.buffer, "count")]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )
    }

    fn encode_call(&self) -> Expression {
        Expression::trailing_closure_statements(
            "boltffiEncode",
            ArgumentList::default(),
            &self.writer,
            self.statements.clone(),
        )
    }
}
