use boltffi_binding::{DefaultValue, FloatValue, Primitive as BindingPrimitive, TypeRef};

use crate::{
    core::Result,
    target::java::{
        JavaHost, JavaVersion,
        primitive::Primitive,
        syntax::{Expression, Identifier, StringLiteral, TypeIdentifier, TypeName},
    },
};

pub struct DefaultExpression;

impl DefaultExpression {
    pub fn render(ty: &TypeRef, value: &DefaultValue, version: JavaVersion) -> Result<Expression> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Self::optional("empty", None, version)),
                _ => Self::render(inner, value, version)
                    .map(|value| Self::optional("of", Some(value), version)),
            };
        }
        match value {
            DefaultValue::Bool(value) => match ty {
                TypeRef::Primitive(BindingPrimitive::Bool) => Ok(Expression::boolean(*value)),
                _ => Err(JavaHost::unsupported("boolean default type")),
            },
            DefaultValue::Integer(value) => Self::integer(ty, value.get()),
            DefaultValue::Float(value) => Self::float(ty, *value),
            DefaultValue::String(value) => {
                Ok(Expression::string(StringLiteral::new(value.clone())))
            }
            DefaultValue::EnumVariant { .. } => Err(JavaHost::unsupported("enum default value")),
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(JavaHost::unsupported("unknown default value")),
        }
    }

    fn integer(ty: &TypeRef, value: i128) -> Result<Expression> {
        let primitive = match ty {
            TypeRef::Primitive(primitive) => *primitive,
            _ => return Err(JavaHost::unsupported("integer default type")),
        };
        match primitive {
            BindingPrimitive::I8 => i8::try_from(value)
                .map(i128::from)
                .map(Expression::signed_integer)
                .map(|value| Expression::cast(Primitive::Byte, value)),
            BindingPrimitive::U8 => u8::try_from(value)
                .map(|value| i128::from(value as i8))
                .map(Expression::signed_integer)
                .map(|value| Expression::cast(Primitive::Byte, value)),
            BindingPrimitive::I16 => i16::try_from(value)
                .map(i128::from)
                .map(Expression::signed_integer)
                .map(|value| Expression::cast(Primitive::Short, value)),
            BindingPrimitive::U16 => u16::try_from(value)
                .map(|value| i128::from(value as i16))
                .map(Expression::signed_integer)
                .map(|value| Expression::cast(Primitive::Short, value)),
            BindingPrimitive::I32 => i32::try_from(value)
                .map(i128::from)
                .map(Expression::signed_integer),
            BindingPrimitive::U32 => u32::try_from(value)
                .map(|value| i128::from(value as i32))
                .map(Expression::signed_integer),
            BindingPrimitive::I64 | BindingPrimitive::ISize => {
                i64::try_from(value).map(Expression::long)
            }
            BindingPrimitive::U64 | BindingPrimitive::USize => {
                u64::try_from(value).map(|value| Expression::long(value as i64))
            }
            BindingPrimitive::Bool | BindingPrimitive::F32 | BindingPrimitive::F64 => {
                return Err(JavaHost::unsupported("integer default type"));
            }
            _ => return Err(JavaHost::unsupported("integer default type")),
        }
        .map_err(|_| JavaHost::unsupported("integer default range"))
    }

    fn float(ty: &TypeRef, value: FloatValue) -> Result<Expression> {
        match ty {
            TypeRef::Primitive(BindingPrimitive::F32) => {
                Ok(Expression::float32(value.to_f64() as f32))
            }
            TypeRef::Primitive(BindingPrimitive::F64) => Ok(Expression::float64(value.to_f64())),
            _ => Err(JavaHost::unsupported("float default type")),
        }
    }

    fn optional(
        method: &'static str,
        value: Option<Expression>,
        version: JavaVersion,
    ) -> Expression {
        Expression::static_call(
            TypeName::qualified(
                [Identifier::known("java"), Identifier::known("util")].into(),
                TypeIdentifier::known("Optional", version),
            ),
            Identifier::known(method),
            value.into_iter().collect(),
        )
    }
}
