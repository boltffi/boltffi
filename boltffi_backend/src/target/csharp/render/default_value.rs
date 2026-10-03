use boltffi_binding::{
    CanonicalName, CustomTypeId, DefaultValue, EnumDecl, FloatValue, Native, Primitive, TypeRef,
};

use crate::core::{
    RenderContext, Result,
    default_value::{Field as RepresentationField, Representation},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::{Expression, Literal},
    type_name,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefaultExpression {
    Constant(Expression),
    Runtime(Expression),
    RuntimeReference(Expression),
}

impl DefaultExpression {
    pub fn render(
        ty: &TypeRef,
        value: &DefaultValue,
        namespace: Option<&Namespace>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Self::Constant(Expression::new("null"))),
                _ => Self::render(inner, value, namespace, context),
            };
        }
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, namespace, context);
        }
        match value {
            DefaultValue::Bool(value) => Ok(Self::Constant(Expression::new(value.to_string()))),
            DefaultValue::Integer(value) => Self::integer(ty, value.get()),
            DefaultValue::Float(value) => Self::float(ty, *value),
            DefaultValue::String(value) => Ok(Self::Constant(Expression::new(
                Literal::string(value).to_string(),
            ))),
            DefaultValue::EnumVariant { variant_name, .. } => {
                Self::enum_variant(ty, variant_name, namespace, context)
            }
            DefaultValue::Null => Ok(Self::Constant(Expression::new("null"))),
            _ => super::super::unsupported("unknown default literal"),
        }
    }

    pub fn constant(&self) -> Option<&Expression> {
        match self {
            Self::Constant(value) => Some(value),
            Self::Runtime(_) | Self::RuntimeReference(_) => None,
        }
    }

    pub fn runtime(&self) -> Option<&Expression> {
        match self {
            Self::Runtime(value) | Self::RuntimeReference(value) => Some(value),
            Self::Constant(_) => None,
        }
    }

    pub fn into_expression(self) -> Expression {
        match self {
            Self::Constant(value) | Self::Runtime(value) | Self::RuntimeReference(value) => value,
        }
    }

    pub fn constructs_reference(&self) -> bool {
        matches!(self, Self::RuntimeReference(_))
    }

    fn custom(
        custom_type: CustomTypeId,
        value: &DefaultValue,
        namespace: Option<&Namespace>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match Representation::resolve(custom_type, context)? {
            Representation::Transparent(representation) => {
                Self::render(representation, value, namespace, context)
            }
            Representation::Record(record) => {
                let value = match record.field() {
                    RepresentationField::Direct(field) => Self::render(
                        &TypeRef::Primitive(field.ty().primitive()),
                        value,
                        namespace,
                        context,
                    )?,
                    RepresentationField::Encoded(field) => {
                        Self::render(field.ty(), value, namespace, context)?
                    }
                };
                let name = Name::new(record.name()).pascal()?;
                let name = namespace.map_or_else(
                    || name.to_string(),
                    |namespace| format!("global::{namespace}.{name}"),
                );
                Ok(Self::Runtime(Expression::new(format!(
                    "new {name}({})",
                    value.into_expression()
                ))))
            }
        }
    }

    fn enum_variant(
        ty: &TypeRef,
        variant_name: &CanonicalName,
        namespace: Option<&Namespace>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let TypeRef::Enum(id) = ty else {
            return super::super::unsupported("enum default type");
        };
        let Some(enumeration) = context.enumeration(*id) else {
            return super::super::unsupported("missing enum default declaration");
        };
        let ty = match namespace {
            Some(namespace) => type_name::type_ref_qualified(ty, namespace, context)?,
            None => type_name::type_ref(ty, context)?,
        };
        let variant = Name::new(variant_name).pascal()?;
        match enumeration {
            EnumDecl::CStyle(_) => Ok(Self::Constant(Expression::new(format!("{ty}.{variant}")))),
            EnumDecl::Data(_) => Ok(Self::RuntimeReference(Expression::new(format!(
                "new {ty}.{variant}()"
            )))),
            _ => super::super::unsupported("unknown enum default type"),
        }
    }

    fn integer(ty: &TypeRef, value: i128) -> Result<Self> {
        let TypeRef::Primitive(primitive) = ty else {
            return super::super::unsupported("integer default type");
        };
        match primitive {
            Primitive::ISize if i32::try_from(value).is_err() => {
                return Ok(Self::Runtime(Expression::new(format!(
                    "unchecked((nint)({value}L))"
                ))));
            }
            Primitive::USize if u32::try_from(value).is_err() => {
                return Ok(Self::Runtime(Expression::new(format!(
                    "unchecked((nuint)({value}UL))"
                ))));
            }
            _ => {}
        }
        Ok(Self::Constant(Expression::new(match primitive {
            Primitive::I8 => format!("(sbyte){value}"),
            Primitive::U8 => format!("(byte){value}"),
            Primitive::I16 => format!("(short){value}"),
            Primitive::U16 => format!("(ushort){value}"),
            Primitive::U32 => format!("{value}U"),
            Primitive::I64 => format!("{value}L"),
            Primitive::U64 => format!("{value}UL"),
            Primitive::ISize => value.to_string(),
            Primitive::USize => format!("{value}U"),
            _ => value.to_string(),
        })))
    }

    fn float(ty: &TypeRef, value: FloatValue) -> Result<Self> {
        let TypeRef::Primitive(primitive) = ty else {
            return super::super::unsupported("float default type");
        };
        let value = value.to_f64();
        if !value.is_finite() {
            return Ok(Self::Constant(Expression::new(
                match (primitive, value.is_nan(), value.is_sign_positive()) {
                    (Primitive::F32, true, _) => "float.NaN",
                    (Primitive::F32, false, true) => "float.PositiveInfinity",
                    (Primitive::F32, false, false) => "float.NegativeInfinity",
                    (Primitive::F64, true, _) => "double.NaN",
                    (Primitive::F64, false, true) => "double.PositiveInfinity",
                    (Primitive::F64, false, false) => "double.NegativeInfinity",
                    _ => return super::super::unsupported("float default primitive"),
                },
            )));
        }
        Ok(Self::Constant(Expression::new(match primitive {
            Primitive::F32 => format!("{}F", value as f32),
            Primitive::F64 => {
                let rendered = value.to_string();
                if rendered.contains(['.', 'E', 'e']) {
                    rendered
                } else {
                    format!("{rendered}.0")
                }
            }
            _ => return super::super::unsupported("float default primitive"),
        })))
    }
}
