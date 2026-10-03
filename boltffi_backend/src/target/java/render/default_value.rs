use boltffi_binding::{
    BuiltinType, CustomTypeId, DefaultValue, FloatValue, HandlePresence, IntoRust, Native,
    ParamDecl, Primitive as BindingPrimitive, TypeRef,
};

use crate::{
    core::{
        RenderContext, Result,
        default_value::{
            Field as RepresentationField, Record as RepresentationRecord, Representation,
            UuidLiteral,
        },
    },
    target::java::{
        JavaHost, JavaVersion,
        name_style::Name,
        primitive::Primitive,
        render::{Enumeration, VariantInitialization, signature::ValueType},
        syntax::{Expression, Identifier, StringLiteral, TypeIdentifier, TypeName},
    },
};

pub struct DefaultExpression;

impl DefaultExpression {
    pub fn parameter(
        parameter: &ParamDecl<Native, IntoRust>,
        value: &DefaultValue,
        java_type: &ValueType,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        match parameter
            .payload()
            .as_value()
            .and_then(|plan| plan.value_type())
        {
            Some(TypeRef::Optional(inner)) if matches!(*inner, TypeRef::Class(_)) => match value {
                DefaultValue::Null => {
                    Ok(Expression::cast(java_type.type_name(), Expression::null()))
                }
                _ => Err(JavaHost::unsupported("class parameter default")),
            },
            Some(ty) => Self::render(&ty, value, version, context),
            None => match (parameter.payload().as_closure(), value) {
                (Some(closure), DefaultValue::Null)
                    if closure.presence() == HandlePresence::Nullable =>
                {
                    Ok(Expression::cast(java_type.type_name(), Expression::null()))
                }
                _ => Err(JavaHost::unsupported(
                    "parameter default without a value type",
                )),
            },
        }
    }

    pub fn render(
        ty: &TypeRef,
        value: &DefaultValue,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Self::optional("empty", None, version)),
                _ => Self::render(inner, value, version, context)
                    .map(|value| Self::optional("of", Some(value), version)),
            };
        }
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, version, context);
        }
        if let TypeRef::Record(record) = ty {
            return Self::record(
                Representation::record(*record, context)?,
                value,
                version,
                context,
            );
        }
        if let TypeRef::Builtin(BuiltinType::Uuid) = ty {
            let DefaultValue::String(value) = value else {
                return Err(JavaHost::unsupported(
                    "UUID default requires a string literal",
                ));
            };
            let uuid = UuidLiteral::parse(value)
                .ok_or_else(|| JavaHost::unsupported("invalid UUID default"))?;
            return Ok(Expression::construct(
                TypeName::qualified(
                    ["java", "util"]
                        .into_iter()
                        .map(Identifier::known)
                        .collect(),
                    TypeIdentifier::known("UUID", version),
                ),
                [
                    Expression::hexadecimal_long(uuid.high_bits()),
                    Expression::hexadecimal_long(uuid.low_bits()),
                ]
                .into_iter()
                .collect(),
            ));
        }
        match value {
            DefaultValue::Bool(value) => match ty {
                TypeRef::Primitive(BindingPrimitive::Bool) => Ok(Expression::boolean(*value)),
                _ => Err(JavaHost::unsupported("boolean default type")),
            },
            DefaultValue::Integer(value) => Self::integer(ty, value.get()),
            DefaultValue::Float(value) => Self::float(ty, *value),
            DefaultValue::String(value) => {
                let literal = Expression::string(StringLiteral::new(value.clone()));
                match ty {
                    TypeRef::String => Ok(literal),
                    TypeRef::Builtin(BuiltinType::Url) => Ok(Expression::static_call(
                        TypeName::qualified(
                            ["java", "net"].into_iter().map(Identifier::known).collect(),
                            TypeIdentifier::known("URI", version),
                        ),
                        Identifier::known("create"),
                        [literal].into_iter().collect(),
                    )),
                    _ => Err(JavaHost::unsupported("string default type")),
                }
            }
            DefaultValue::EnumVariant { variant_name, .. } => match ty {
                TypeRef::Enum(id) => Enumeration::unit_variant_expression(
                    *id,
                    variant_name,
                    VariantInitialization::External,
                    version,
                    context,
                ),
                _ => Err(JavaHost::unsupported("enum default type")),
            },
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(JavaHost::unsupported("unknown default value")),
        }
    }

    fn custom(
        custom_type: CustomTypeId,
        value: &DefaultValue,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        match Representation::resolve(custom_type, context)? {
            Representation::Transparent(representation) => {
                Self::render(representation, value, version, context)
            }
            Representation::Record(record) => Self::record(record, value, version, context),
        }
    }

    fn record(
        record: RepresentationRecord<'_>,
        value: &DefaultValue,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let value = match record.field() {
            RepresentationField::Direct(field) => Self::render(
                &TypeRef::Primitive(field.ty().primitive()),
                value,
                version,
                context,
            )?,
            RepresentationField::Encoded(field) => {
                Self::render(field.ty(), value, version, context)?
            }
        };
        Ok(Expression::construct(
            TypeName::named(Name::new(record.name()).type_name(version)?),
            [value].into_iter().collect(),
        ))
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
