use crate::{
    core::Result,
    target::kotlin::{
        KotlinHost,
        syntax::{ArgumentList, Expression, TypeName},
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Arity {
    Pair,
    Triple,
}

impl Arity {
    pub fn from_count(count: usize) -> Result<Self> {
        match count {
            2 => Ok(Self::Pair),
            3 => Ok(Self::Triple),
            _ => Err(KotlinHost::unsupported("tuple arity")),
        }
    }

    pub fn type_name(self, elements: Vec<TypeName>) -> Result<TypeName> {
        self.require_count(elements.len())?;
        Ok(TypeName::parameterized(self.constructor(), elements))
    }

    pub fn expression(self, elements: Vec<Expression>) -> Result<Expression> {
        self.require_count(elements.len())?;
        Ok(Expression::construct(
            TypeName::new(self.constructor()),
            elements.into_iter().collect::<ArgumentList>(),
        ))
    }

    fn constructor(self) -> &'static str {
        match self {
            Self::Pair => "Pair",
            Self::Triple => "Triple",
        }
    }

    fn require_count(self, count: usize) -> Result<()> {
        match count == self.count() {
            true => Ok(()),
            false => Err(KotlinHost::unsupported("tuple arity")),
        }
    }

    fn count(self) -> usize {
        match self {
            Self::Pair => 2,
            Self::Triple => 3,
        }
    }
}
