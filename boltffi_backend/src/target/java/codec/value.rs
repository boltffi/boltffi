use boltffi_binding::{BinderId, FieldKey, ValueRef, ValueRoot};

use crate::{
    core::Result,
    target::java::{
        JavaHost, JavaVersion,
        name_style::Name,
        syntax::{ArgumentList, Expression, Identifier},
    },
};

pub struct ValueExpression {
    value: ValueRef,
    current: Expression,
    member_access: ValueMemberAccess,
    version: JavaVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueMemberAccess {
    Accessor,
    RecordField,
    VariantField,
}

impl ValueExpression {
    pub fn new(value: &ValueRef, version: JavaVersion) -> Self {
        Self {
            value: value.clone(),
            current: Expression::identifier(Identifier::known("value")),
            member_access: ValueMemberAccess::Accessor,
            version,
        }
    }

    pub fn binder(binder: BinderId, version: JavaVersion) -> Result<Identifier> {
        Identifier::parse_for(format!("__boltffi_value_{}", binder.raw()), version)
    }

    pub fn current(mut self, current: Expression) -> Self {
        self.current = current;
        self
    }

    pub fn member_access(mut self, member_access: ValueMemberAccess) -> Self {
        self.member_access = member_access;
        self
    }

    pub fn render(self) -> Result<Expression> {
        let self_value = matches!(self.value.root(), ValueRoot::SelfValue);
        let root = match self.value.root() {
            ValueRoot::SelfValue => self.current,
            ValueRoot::Named(name) | ValueRoot::Local(name) => Name::new(name)
                .parameter(self.version)
                .map(Expression::identifier)?,
            ValueRoot::Binder(binder) => {
                Expression::identifier(Self::binder(*binder, self.version)?)
            }
            _ => return Err(JavaHost::unsupported("unknown codec value root")),
        };
        self.value
            .path()
            .iter()
            .enumerate()
            .try_fold(root, |value, (depth, field)| {
                let field = self.member_access.field(field, self.version)?;
                match self_value && depth == 0 && self.member_access.is_field() {
                    true => Ok(value.member(field)),
                    false => Ok(value.call(field, ArgumentList::default())),
                }
            })
    }
}

impl ValueMemberAccess {
    pub fn field(self, field: &FieldKey, version: JavaVersion) -> Result<Identifier> {
        match field {
            FieldKey::Named(name) => Name::new(name).parameter(version),
            FieldKey::Position(position) => Identifier::parse_for(
                format!(
                    "{}{position}",
                    match self {
                        Self::VariantField => "value",
                        Self::Accessor | Self::RecordField => "field",
                    }
                ),
                version,
            ),
            _ => Err(JavaHost::unsupported("unknown codec value field")),
        }
    }

    fn is_field(self) -> bool {
        matches!(self, Self::RecordField | Self::VariantField)
    }
}
