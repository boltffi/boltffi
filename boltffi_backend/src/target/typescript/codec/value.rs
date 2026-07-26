use boltffi_binding::{BinderId, ValueRef, ValueRoot};

use crate::core::{Error, Result};

use super::super::{
    name_style::Name,
    syntax::{Expression, Identifier, PropertyKey},
};

pub struct ValueExpression {
    value: ValueRef,
    current: Expression,
}

impl ValueExpression {
    pub fn new(value: &ValueRef, current: Expression) -> Self {
        Self {
            value: value.clone(),
            current,
        }
    }

    pub fn binder(binder: BinderId) -> Result<Identifier> {
        Identifier::parse(format!("__boltffiValue{}", binder.raw()))
    }

    pub fn render(self) -> Result<Expression> {
        let root = match self.value.root() {
            ValueRoot::SelfValue => self.current,
            ValueRoot::Named(name) | ValueRoot::Local(name) => {
                Expression::identifier(Name::new(name).identifier()?)
            }
            ValueRoot::Binder(binder) => Expression::identifier(Self::binder(*binder)?),
            _ => return Self::unsupported("unknown codec value root"),
        };
        self.value.path().iter().try_fold(root, |value, field| {
            PropertyKey::from_field(field)?.access(value)
        })
    }

    fn unsupported<T>(shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }
}
