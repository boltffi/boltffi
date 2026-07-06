use crate::{
    core::Result,
    target::swift::syntax::{ArgumentList, Expression, Identifier, TypeName},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedBuffer {
    binding: Identifier,
}

impl OwnedBuffer {
    pub fn new(binding: Identifier) -> Self {
        Self { binding }
    }

    pub fn binding(&self) -> &Identifier {
        &self.binding
    }

    pub fn decode(&self, reader: &Identifier, decode: &Expression) -> Result<Expression> {
        Ok(Expression::trailing_closure(
            Self::decode_function()?,
            [self.pointer(), self.length()]
                .into_iter()
                .collect::<ArgumentList>(),
            reader,
            decode,
        ))
    }

    pub fn is_present(&self) -> Expression {
        Expression::or(
            Expression::not_equal(self.pointer(), Expression::nil()),
            Expression::not_equal(self.length(), "0"),
        )
    }

    pub fn free_call(&self, free: &Identifier) -> Expression {
        Expression::call(
            free,
            [Expression::identifier(self.binding.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        )
    }

    fn pointer(&self) -> Expression {
        Expression::member(&self.binding, "ptr")
    }

    fn length(&self) -> Expression {
        Expression::call(
            TypeName::int(),
            [Expression::member(&self.binding, "len")]
                .into_iter()
                .collect::<ArgumentList>(),
        )
    }

    fn decode_function() -> Result<Identifier> {
        Identifier::parse("boltffiDecodeOwnedBuf")
    }
}
