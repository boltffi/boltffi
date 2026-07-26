use boltffi_binding::{BinderId, FieldKey, ValueRef, ValueRoot};

use crate::core::Result;

use super::super::{
    name_style::Name,
    syntax::{Expression, Identifier},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) enum ValueScope {
    Current(Expression),
    Fields(Vec<(FieldKey, Expression)>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PositionAccess {
    TupleElements,
}

pub(super) struct ValueExpression {
    value: ValueRef,
    scope: ValueScope,
}

impl ValueExpression {
    pub(super) fn new(value: &ValueRef, scope: impl Into<ValueScope>) -> Self {
        Self {
            value: value.clone(),
            scope: scope.into(),
        }
    }

    pub(super) fn binder(binder: BinderId) -> Result<Identifier> {
        Identifier::parse(format!("boltffiValue{}", binder.raw()))
    }

    pub(super) fn render(self) -> Result<Expression> {
        match self.value.root() {
            ValueRoot::SelfValue => self.scope.render(self.value.path()),
            ValueRoot::Named(name) | ValueRoot::Local(name) => {
                self.scope.render_named(name, self.value.path())
            }
            ValueRoot::Binder(binder) => Self::render_path(
                Expression::identifier(Self::binder(*binder)?),
                self.value.path(),
                PositionAccess::TupleElements,
            ),
            _ => super::super::unsupported("unknown codec value root"),
        }
    }

    fn render_path(
        expression: Expression,
        path: &[FieldKey],
        positions: PositionAccess,
    ) -> Result<Expression> {
        path.iter().try_fold(expression, |expression, field| {
            Self::field(expression, field, positions)
        })
    }

    fn field(
        expression: Expression,
        field: &FieldKey,
        positions: PositionAccess,
    ) -> Result<Expression> {
        let field = match field {
            FieldKey::Named(name) => Name::new(name).pascal()?,
            FieldKey::Position(position) => match positions {
                PositionAccess::TupleElements => {
                    Identifier::parse(format!("Item{}", position + 1))?
                }
            },
            _ => return super::super::unsupported("unknown codec value field"),
        };
        Ok(Expression::new(format!("{expression}.{field}")))
    }
}

impl ValueScope {
    pub(in crate::target::csharp) fn fields(fields: Vec<(FieldKey, Expression)>) -> Self {
        Self::Fields(fields)
    }

    fn render(self, path: &[FieldKey]) -> Result<Expression> {
        match self {
            Self::Current(expression) => {
                ValueExpression::render_path(expression, path, PositionAccess::TupleElements)
            }
            Self::Fields(fields) => match path.split_first() {
                Some((field, rest)) => fields
                    .into_iter()
                    .find_map(|(key, value)| (key == *field).then_some(value))
                    .ok_or(crate::core::Error::UnsupportedTarget {
                        target: "csharp",
                        shape: "unknown codec payload field",
                    })
                    .and_then(|value| {
                        ValueExpression::render_path(value, rest, PositionAccess::TupleElements)
                    }),
                None => super::super::unsupported("whole codec payload value"),
            },
        }
    }

    fn render_named(
        self,
        name: &boltffi_binding::CanonicalName,
        path: &[FieldKey],
    ) -> Result<Expression> {
        match self {
            Self::Fields(fields) => fields
                .into_iter()
                .find_map(|(key, value)| match key {
                    FieldKey::Named(field) if field == *name => Some(value),
                    _ => None,
                })
                .ok_or(crate::core::Error::UnsupportedTarget {
                    target: "csharp",
                    shape: "unknown named codec field",
                })
                .and_then(|value| {
                    ValueExpression::render_path(value, path, PositionAccess::TupleElements)
                }),
            Self::Current(_) => ValueExpression::render_path(
                Expression::identifier(Name::new(name).camel()?),
                path,
                PositionAccess::TupleElements,
            ),
        }
    }
}

impl From<Expression> for ValueScope {
    fn from(expression: Expression) -> Self {
        Self::Current(expression)
    }
}
