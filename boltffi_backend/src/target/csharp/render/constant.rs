use boltffi_binding::{
    ConstantDecl, ConstantValueDecl, DefaultValue, FloatValue, Native, Primitive, TypeRef,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, RenderContext, Result},
};

use super::super::{name_style::Name, syntax::Literal, type_name};
use super::Function;

pub(in crate::target::csharp) enum Constant {
    Inline(String),
    Accessor(Box<Function>),
}

impl Constant {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &ConstantDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                let name = Name::new(declaration.name()).pascal()?;
                let ty = type_name::type_ref(ty, context)?;
                let value = render_value(declaration.value(), value, context)?;
                let declaration = if matches!(value.as_str(), "null") {
                    "public static readonly"
                } else {
                    "public const"
                };
                Ok(Self::Inline(format!(
                    "        {declaration} {ty} {name} = {value};\n"
                )))
            }
            ConstantValueDecl::Accessor { symbol, callable } => Function::from_constant_accessor(
                declaration.name(),
                symbol,
                callable,
                bridge,
                context,
            )
            .map(Box::new)
            .map(Self::Accessor),
            _ => super::super::unsupported("unknown constant value"),
        }
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        match self {
            Self::Inline(source) => Ok(Emitted::primary(source.clone())),
            Self::Accessor(function) => function.render(),
        }
    }
}

fn render_value(
    declaration: &ConstantValueDecl<Native>,
    value: &DefaultValue,
    context: &RenderContext<Native>,
) -> Result<String> {
    let ConstantValueDecl::Inline { ty, .. } = declaration else {
        return super::super::unsupported("constant accessor literal");
    };
    match value {
        DefaultValue::Bool(value) => Ok(value.to_string().to_lowercase()),
        DefaultValue::Integer(value) => render_integer(ty, value.get()),
        DefaultValue::Float(value) => render_float(ty, *value),
        DefaultValue::String(value) => Ok(Literal::string(value).to_string()),
        DefaultValue::EnumVariant { variant_name, .. } => Ok(format!(
            "{}.{}",
            type_name::type_ref(ty, context)?,
            Name::new(variant_name).pascal()?
        )),
        DefaultValue::Null => Ok("null".to_owned()),
        _ => super::super::unsupported("unknown constant literal"),
    }
}

fn render_integer(ty: &TypeRef, value: i128) -> Result<String> {
    let TypeRef::Primitive(primitive) = ty else {
        return super::super::unsupported("integer constant type");
    };
    let suffix = match primitive {
        Primitive::U32 => "U",
        Primitive::I64 | Primitive::ISize => "L",
        Primitive::U64 | Primitive::USize => "UL",
        _ => "",
    };
    Ok(format!("{value}{suffix}"))
}

fn render_float(ty: &TypeRef, value: FloatValue) -> Result<String> {
    let TypeRef::Primitive(primitive) = ty else {
        return super::super::unsupported("float constant type");
    };
    let value = value.to_f64();
    if !value.is_finite() {
        return Ok(
            match (primitive, value.is_nan(), value.is_sign_positive()) {
                (Primitive::F32, true, _) => "float.NaN".to_owned(),
                (Primitive::F32, false, true) => "float.PositiveInfinity".to_owned(),
                (Primitive::F32, false, false) => "float.NegativeInfinity".to_owned(),
                (Primitive::F64, true, _) => "double.NaN".to_owned(),
                (Primitive::F64, false, true) => "double.PositiveInfinity".to_owned(),
                (Primitive::F64, false, false) => "double.NegativeInfinity".to_owned(),
                _ => return super::super::unsupported("float constant primitive"),
            },
        );
    }
    Ok(match primitive {
        Primitive::F32 => format!("{}F", value as f32),
        Primitive::F64 => {
            let rendered = value.to_string();
            if rendered.contains(['.', 'E', 'e']) {
                rendered
            } else {
                format!("{rendered}.0")
            }
        }
        _ => return super::super::unsupported("float constant primitive"),
    })
}
