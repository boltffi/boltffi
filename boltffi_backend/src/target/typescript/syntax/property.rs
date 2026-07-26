use std::fmt;

use boltffi_binding::FieldKey;

use crate::core::{Error, Result};

use super::super::name_style::Name;
use super::{Expression, Identifier};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PropertyKey {
    Named(Identifier),
    Position(u32),
}

impl PropertyKey {
    pub fn from_field(field: &FieldKey) -> Result<Self> {
        match field {
            FieldKey::Named(name) => Ok(Self::Named(Name::new(name).identifier()?)),
            FieldKey::Position(position) => Ok(Self::Position(*position)),
            _ => Err(Error::UnsupportedTarget {
                target: "typescript",
                shape: "unknown field key",
            }),
        }
    }

    pub fn access(&self, receiver: Expression) -> Result<Expression> {
        match self {
            Self::Named(identifier) => Ok(Expression::property(receiver, identifier.clone())),
            Self::Position(position) => Identifier::parse(format!("value{position}"))
                .map(|identifier| Expression::property(receiver, identifier)),
        }
    }

    pub fn local(&self) -> Result<Identifier> {
        match self {
            Self::Named(identifier) => Ok(identifier.clone()),
            Self::Position(position) => Identifier::parse(format!("__boltffiField{position}")),
        }
    }
}

impl fmt::Display for PropertyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(identifier) => identifier.fmt(formatter),
            Self::Position(position) => write!(formatter, "value{position}"),
        }
    }
}
