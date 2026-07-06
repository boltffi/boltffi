use boltffi_binding::{BinderId, FieldKey, ValueRef, ValueRoot};

use crate::{
    core::Result,
    target::swift::{
        SwiftHost,
        name_style::Name,
        syntax::{Expression, Identifier},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueExpression {
    value: ValueRef,
    scope: ValueScope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueScope {
    Current {
        expression: Expression,
        positions: PositionAccess,
    },
    Fields(Vec<(FieldKey, Expression)>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionAccess {
    RecordFields,
    TupleElements,
}

impl ValueExpression {
    pub fn new(value: &ValueRef, scope: impl Into<ValueScope>) -> Self {
        Self {
            value: value.clone(),
            scope: scope.into(),
        }
    }

    pub fn binder(binder: BinderId) -> Result<Identifier> {
        Identifier::parse(format!("boltffiValue{}", binder.raw()))
    }

    pub fn render(self) -> Result<Expression> {
        match self.value.root() {
            ValueRoot::SelfValue => self.scope.render(self.value.path()),
            ValueRoot::Named(name) | ValueRoot::Local(name) => Self::render_path(
                Expression::identifier(Name::new(name).parameter()?),
                self.value.path(),
                PositionAccess::TupleElements,
            ),
            ValueRoot::Binder(binder) => Self::render_path(
                Expression::identifier(Self::binder(*binder)?),
                self.value.path(),
                PositionAccess::TupleElements,
            ),
            _ => Err(SwiftHost::unsupported("unknown codec value root")),
        }
    }

    fn render_path(
        expression: Expression,
        path: &[FieldKey],
        positions: PositionAccess,
    ) -> Result<Expression> {
        path.iter()
            .enumerate()
            .try_fold(expression, |expression, (index, field)| {
                Self::field(expression, field, positions, index)
            })
    }

    fn field(
        expression: Expression,
        field: &FieldKey,
        positions: PositionAccess,
        index: usize,
    ) -> Result<Expression> {
        match field {
            FieldKey::Named(name) => Ok(Expression::member(expression, Name::new(name).field()?)),
            FieldKey::Position(position) => Ok(Expression::member(
                expression,
                Self::position_field(*position, positions, index),
            )),
            _ => Err(SwiftHost::unsupported("unknown codec value field")),
        }
    }

    fn position_field(position: u32, positions: PositionAccess, index: usize) -> String {
        match (positions, index) {
            (PositionAccess::RecordFields, 0) => format!("field{position}"),
            _ => position.to_string(),
        }
    }
}

impl ValueScope {
    pub fn record(expression: Expression) -> Self {
        Self::Current {
            expression,
            positions: PositionAccess::RecordFields,
        }
    }

    pub fn fields(fields: Vec<(FieldKey, Expression)>) -> Self {
        Self::Fields(fields)
    }

    fn render(self, path: &[FieldKey]) -> Result<Expression> {
        match self {
            Self::Current {
                expression,
                positions,
            } => ValueExpression::render_path(expression, path, positions),
            Self::Fields(fields) => Self::render_field(fields, path),
        }
    }

    fn render_field(fields: Vec<(FieldKey, Expression)>, path: &[FieldKey]) -> Result<Expression> {
        match path.split_first() {
            Some((field, rest)) => fields
                .into_iter()
                .find_map(|(key, value)| (key == *field).then_some(value))
                .ok_or(SwiftHost::unsupported("unknown codec payload field"))
                .and_then(|value| {
                    ValueExpression::render_path(value, rest, PositionAccess::TupleElements)
                }),
            None => Err(SwiftHost::unsupported("whole payload value")),
        }
    }
}

impl From<Expression> for ValueScope {
    fn from(expression: Expression) -> Self {
        Self::Current {
            expression,
            positions: PositionAccess::TupleElements,
        }
    }
}
