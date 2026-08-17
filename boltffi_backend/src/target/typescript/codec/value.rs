use boltffi_binding::{BinderId, ValueRef, ValueRoot};

use crate::core::{Error, Result};

use super::super::{
    name_style::Name,
    syntax::{Expression, Identifier, PropertyKey},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecordDefaults(Vec<(PropertyKey, Expression)>);

pub struct ValueExpression {
    value: ValueRef,
    current: Expression,
    defaults: RecordDefaults,
}

impl RecordDefaults {
    pub fn new(defaults: impl IntoIterator<Item = (PropertyKey, Expression)>) -> Self {
        Self(defaults.into_iter().collect())
    }

    fn get(&self, field: &boltffi_binding::FieldKey) -> Result<Option<&Expression>> {
        let key = PropertyKey::from_field(field)?;
        Ok(self
            .0
            .iter()
            .find(|(candidate, _)| candidate == &key)
            .map(|(_, default)| default))
    }
}

impl ValueExpression {
    pub fn new(value: &ValueRef, current: Expression) -> Self {
        Self {
            value: value.clone(),
            current,
            defaults: RecordDefaults::default(),
        }
    }

    pub fn with_defaults(mut self, defaults: &RecordDefaults) -> Self {
        self.defaults = defaults.clone();
        self
    }

    pub fn binder(binder: BinderId) -> Result<Identifier> {
        Identifier::parse(format!("__boltffiValue{}", binder.raw()))
    }

    pub fn render(self) -> Result<Expression> {
        let defaulted_root = matches!(self.value.root(), ValueRoot::SelfValue);
        let root = match self.value.root() {
            ValueRoot::SelfValue => self.current,
            ValueRoot::Named(name) | ValueRoot::Local(name) => {
                Expression::identifier(Name::new(name).identifier()?)
            }
            ValueRoot::Binder(binder) => Expression::identifier(Self::binder(*binder)?),
            _ => return Self::unsupported("unknown codec value root"),
        };
        self.value
            .path()
            .iter()
            .enumerate()
            .try_fold(root, |value, (index, field)| {
                let value = PropertyKey::from_field(field)?.access(value)?;
                match defaulted_root && index == 0 {
                    true => Ok(self
                        .defaults
                        .get(field)?
                        .cloned()
                        .map(|default| value.clone().default_when_undefined(default))
                        .unwrap_or(value)),
                    false => Ok(value),
                }
            })
    }

    fn unsupported<T>(shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }
}
