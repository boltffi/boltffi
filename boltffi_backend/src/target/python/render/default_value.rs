use boltffi_binding::{BuiltinType, CustomTypeId, DefaultValue, FieldKey, TypeRef};

use crate::{
    core::{
        Error, Result,
        default_value::{Field as RepresentationField, Representation},
    },
    target::python::{
        name_style::Name,
        syntax::{CallExpression, Expression, Identifier, Literal},
    },
};

use super::Package;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefaultExpression {
    Immediate(Expression),
    Deferred(Expression),
}

impl DefaultExpression {
    pub fn new(ty: &TypeRef, value: &DefaultValue, package: &Package) -> Result<Self> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Self::Immediate(Expression::literal(Literal::none()))),
                _ => Self::new(inner, value, package),
            };
        }
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, package);
        }
        if matches!(ty, TypeRef::Builtin(BuiltinType::Uuid)) {
            let DefaultValue::String(value) = value else {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "UUID default requires a string literal",
                });
            };
            return Ok(Self::Deferred(Expression::call(
                CallExpression::new(Expression::identifier(Identifier::parse("_UUID")?))
                    .positional(Expression::literal(Literal::string(value))),
            )));
        }
        let expression = match value {
            DefaultValue::Bool(value) => Expression::literal(Literal::bool(*value)),
            DefaultValue::Integer(value) => Expression::literal(Literal::integer(value.get())),
            DefaultValue::Float(value) => Literal::float(value.to_f64()),
            DefaultValue::String(value) => Expression::literal(Literal::string(value)),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => {
                return package
                    .enum_variant_expression(enum_name, variant_name)
                    .map(Self::Deferred);
            }
            DefaultValue::Null => Expression::literal(Literal::none()),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown constant literal",
                });
            }
        };
        Ok(Self::Immediate(expression))
    }

    fn custom(custom_type: CustomTypeId, value: &DefaultValue, package: &Package) -> Result<Self> {
        match Representation::resolve(custom_type, package.context)? {
            Representation::Transparent(representation) => {
                Self::new(representation, value, package)
            }
            Representation::Record(record) => {
                let value = match record.field() {
                    RepresentationField::Direct(field) => {
                        Self::new(&TypeRef::Primitive(field.ty().primitive()), value, package)?
                    }
                    RepresentationField::Encoded(field) => Self::new(field.ty(), value, package)?,
                };
                let field = match record.field().key() {
                    FieldKey::Named(name) => Name::new(name).function()?,
                    FieldKey::Position(position) => Name::position_field(*position)?,
                    _ => {
                        return Err(Error::UnsupportedTarget {
                            target: "python",
                            shape: "custom type default field name",
                        });
                    }
                };
                Ok(Self::Deferred(Expression::call(
                    CallExpression::new(Expression::identifier(Identifier::parse(
                        Name::new(record.name()).class(),
                    )?))
                    .keyword(field, value.into_expression()),
                )))
            }
        }
    }

    pub fn into_expression(self) -> Expression {
        match self {
            Self::Immediate(expression) | Self::Deferred(expression) => expression,
        }
    }

    pub fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred(_))
    }
}
