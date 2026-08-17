use boltffi_binding::{CustomTypeId, DefaultValue, EnumDecl, Native, TypeRef};

use crate::core::{
    RenderContext, Result,
    default_value::{Field as RepresentationField, Representation},
};

use super::{
    name_style::Name,
    render::{declaration_name, field_name},
    syntax::{Expression, Literal},
    unsupported,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefaultExpression {
    Constant(Literal),
    Runtime(Expression),
}

impl DefaultExpression {
    pub fn render(
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Self::Constant(Literal::new("null"))),
                _ => match Self::render(inner, value, context)? {
                    Self::Constant(value) => Ok(Self::Constant(value)),
                    Self::Runtime(_) => unsupported("optional custom-record default"),
                },
            };
        }
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, context);
        }
        if let (
            TypeRef::Enum(enumeration),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            },
        ) = (ty, value)
            && matches!(context.enumeration(*enumeration), Some(EnumDecl::Data(_)))
        {
            return Ok(Self::Constant(Literal::new(format!(
                "{}${}()",
                Name::new(enum_name).upper_camel()?,
                Name::new(variant_name).upper_camel()?
            ))));
        }
        Self::literal(value).map(Self::Constant)
    }

    pub fn into_constant(self) -> Result<Literal> {
        match self {
            Self::Constant(value) => Ok(value),
            Self::Runtime(_) => unsupported("runtime constant default"),
        }
    }

    fn custom(
        custom_type: CustomTypeId,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
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
                let value = match value {
                    Self::Constant(value) => value.to_string(),
                    Self::Runtime(value) => value.to_string(),
                };
                Ok(Self::Runtime(Expression::new(format!(
                    "{}({}: {value})",
                    declaration_name(record.name())?,
                    field_name(record.field().key())?,
                ))))
            }
        }
    }

    fn literal(value: &DefaultValue) -> Result<Literal> {
        let source = match value {
            DefaultValue::Bool(value) => value.to_string(),
            DefaultValue::Integer(value) => value.get().to_string(),
            DefaultValue::Float(value) => value.to_f64().to_string(),
            DefaultValue::String(value) => format!("{value:?}"),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => format!(
                "{}.{}",
                Name::new(enum_name).upper_camel()?,
                Name::new(variant_name).lower_camel()?,
            ),
            DefaultValue::Null => "null".to_owned(),
            _ => return unsupported("unknown default value"),
        };
        Ok(Literal::new(source))
    }
}
