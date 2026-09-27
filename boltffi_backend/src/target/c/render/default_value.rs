use boltffi_binding::{DefaultValue, EnumDecl, Native, Primitive, TypeRef};

use crate::{
    bridge::c,
    core::{
        Error, RenderContext, Result,
        default_value::{Field, Representation},
    },
    target::c::name_style::Name,
};

use super::{
    prefix::PackagePrefix,
    surface::{self, ValueUse},
};

pub fn render(
    ty: &TypeRef,
    value: &DefaultValue,
    usage: ValueUse,
    context: &RenderContext<Native>,
) -> Result<c::Expression> {
    let value_type = surface::value_type(ty, context, usage)?;
    if let TypeRef::Optional(inner) = ty {
        return Ok(c::Expression::new(match value {
            DefaultValue::Null => format!("(({value_type}){{0}})"),
            _ => {
                let value = render(inner, value, usage, context)?;
                format!("(({value_type}){{.has_value = true, .value = {value}}})")
            }
        }));
    }
    if let TypeRef::Custom(custom) = ty {
        return match Representation::resolve(*custom, context)? {
            Representation::Transparent(representation) => {
                render(representation, value, usage, context)
            }
            Representation::Record(record) => {
                let field = Name::field(record.field().key())?;
                let value = match record.field() {
                    Field::Direct(field) => render(
                        &TypeRef::Primitive(field.ty().primitive()),
                        value,
                        usage,
                        context,
                    )?,
                    Field::Encoded(field) => render(field.ty(), value, usage, context)?,
                };
                Ok(c::Expression::new(format!(
                    "(({value_type}){{.{field} = {value}}})"
                )))
            }
        };
    }
    let expression = match value {
        DefaultValue::Bool(value) => value.to_string(),
        DefaultValue::Integer(value) => match ty {
            TypeRef::Primitive(Primitive::U64 | Primitive::USize) => {
                format!("UINT64_C({})", value.get())
            }
            TypeRef::Primitive(Primitive::I64 | Primitive::ISize)
                if value.get() == i64::MIN as i128 =>
            {
                "(-INT64_C(9223372036854775807) - 1)".to_owned()
            }
            TypeRef::Primitive(Primitive::I64 | Primitive::ISize) => {
                format!("INT64_C({})", value.get())
            }
            _ => value.get().to_string(),
        },
        DefaultValue::Float(value) => {
            let value = value.to_f64();
            if value.is_nan() {
                "NAN".to_owned()
            } else if value.is_infinite() {
                if value.is_sign_negative() {
                    "(-INFINITY)"
                } else {
                    "INFINITY"
                }
                .to_owned()
            } else {
                let literal = value.to_string();
                if literal.contains(['.', 'e', 'E']) {
                    literal
                } else {
                    format!("{literal}.0")
                }
            }
        }
        DefaultValue::String(value) => {
            let literal = c::Literal::byte_string(value.as_bytes());
            format!(
                "(({value_type}){{.ptr = {literal}, .len = {}}})",
                value.len()
            )
        }
        DefaultValue::EnumVariant {
            enum_name,
            variant_name,
        } => {
            let variant = PackagePrefix::from_context(context).constant(&format!(
                "{}_{}",
                Name::new(enum_name).constant(),
                Name::new(variant_name).constant()
            ));
            if matches!(ty, TypeRef::Enum(id) if matches!(context.enumeration(*id), Some(EnumDecl::Data(_))))
            {
                format!("(({value_type}){{.tag = {variant}}})")
            } else {
                variant
            }
        }
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "c",
                shape: "record default value",
            });
        }
    };
    Ok(c::Expression::new(expression))
}
