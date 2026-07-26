use boltffi_binding::{DefaultValue, EnumDecl, FloatValue, Native, Primitive, TypeRef};

use crate::{
    core::{RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        name_style::Name,
        primitive::KotlinPrimitive,
        syntax::{Expression, Literal},
    },
};

pub struct DefaultExpression;

impl DefaultExpression {
    pub fn render(
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        match value {
            DefaultValue::Bool(value) => Ok(Expression::bool(*value)),
            DefaultValue::Integer(value) => match ty {
                TypeRef::Primitive(primitive) => {
                    KotlinPrimitive::new(*primitive).integer_literal(*value)
                }
                _ => Err(KotlinHost::unsupported("integer default type")),
            },
            DefaultValue::Float(value) => Self::float(*value, ty),
            DefaultValue::String(value) => Ok(Expression::literal(Literal::string(value))),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => match ty {
                TypeRef::Enum(id) => context
                    .enumeration(*id)
                    .ok_or(KotlinHost::broken_bridge_contract(
                        "enum default type was not found",
                    ))
                    .and_then(|enumeration| {
                        let variant = match enumeration {
                            EnumDecl::CStyle(_) => Name::new(variant_name).enum_entry()?,
                            EnumDecl::Data(_) => Name::new(variant_name).variant()?,
                            _ => return Err(KotlinHost::unsupported("enum default declaration")),
                        };
                        Ok(Expression::property(
                            Name::new(enum_name).type_name(),
                            variant,
                        ))
                    }),
                _ => Err(KotlinHost::unsupported("enum default type")),
            },
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(KotlinHost::unsupported("unknown default literal")),
        }
    }

    fn float(value: FloatValue, ty: &TypeRef) -> Result<Expression> {
        match ty {
            TypeRef::Primitive(Primitive::F32) => Ok(Expression::float(value.to_f64(), true)),
            TypeRef::Primitive(Primitive::F64) => Ok(Expression::float(value.to_f64(), false)),
            _ => Err(KotlinHost::unsupported("float default type")),
        }
    }
}
