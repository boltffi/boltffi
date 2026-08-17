use boltffi_binding::{CustomTypeId, DefaultValue, EnumDecl, Primitive, TypeRef, Wasm32};

use crate::core::{
    Error, RenderContext, Result,
    default_value::{Field as RepresentationField, Representation},
};

use super::super::{
    name_style::Name,
    syntax::{Expression, Identifier, IntegerLiteral, PropertyKey, StringLiteral},
};

pub struct DefaultExpression;

impl DefaultExpression {
    pub fn render(
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Wasm32>,
    ) -> Result<Expression> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Expression::null()),
                _ => Self::render(inner, value, context),
            };
        }
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, context);
        }
        match value {
            DefaultValue::Bool(value) => Ok(Expression::boolean(*value)),
            DefaultValue::Integer(value)
                if matches!(ty, TypeRef::Primitive(Primitive::I64 | Primitive::U64)) =>
            {
                Ok(Expression::integer_literal(IntegerLiteral::bigint(
                    value.get(),
                )))
            }
            DefaultValue::Integer(value) => Ok(Expression::integer_literal(
                IntegerLiteral::number(value.get()),
            )),
            DefaultValue::Float(value) => Ok(Expression::floating(value.to_f64())),
            DefaultValue::String(value) => Ok(Expression::string(StringLiteral::new(value))),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => match ty {
                TypeRef::Enum(id) => match context.enumeration(*id) {
                    Some(EnumDecl::CStyle(_)) => Ok(Expression::property(
                        Expression::identifier(Identifier::parse(
                            Name::new(enum_name).type_name().to_string(),
                        )?),
                        Name::new(variant_name).variant_identifier()?,
                    )),
                    Some(EnumDecl::Data(_)) => Ok(Expression::object([(
                        PropertyKey::Named(Identifier::known("tag")),
                        Expression::string(StringLiteral::new(
                            &Name::new(variant_name).variant_identifier()?.to_string(),
                        )),
                    )])),
                    _ => Err(Self::unsupported("default enum declaration")),
                },
                _ => Err(Self::unsupported("default enum type")),
            },
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(Self::unsupported("default value")),
        }
    }

    fn custom(
        custom_type: CustomTypeId,
        value: &DefaultValue,
        context: &RenderContext<Wasm32>,
    ) -> Result<Expression> {
        match Representation::resolve(custom_type, context)? {
            Representation::Transparent(representation) => {
                Self::render(representation, value, context)
            }
            Representation::Record(record) => {
                let value = match record.field() {
                    RepresentationField::Direct(field) => {
                        Self::render(&TypeRef::Primitive(field.ty().primitive()), value, context)?
                    }
                    RepresentationField::Encoded(field) => {
                        Self::render(field.ty(), value, context)?
                    }
                };
                Ok(Expression::object([(
                    PropertyKey::from_field(record.field().key())?,
                    value,
                )]))
            }
        }
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}
