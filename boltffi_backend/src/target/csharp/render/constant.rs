use boltffi_binding::{
    ConstantDecl, ConstantOwner, ConstantValueDecl, DefaultValue, EnumDecl, FloatValue, Native,
    Primitive, TypeRef,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, RenderContext, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::Literal,
    type_name,
};
use super::{Documentation, Function};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) enum Constant {
    Inline(String),
    Accessor(Box<Function>),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::target::csharp) struct AssociatedConstants(Vec<Constant>);

impl Constant {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &ConstantDecl<Native>,
        namespace: &Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                let name = Name::new(declaration.name()).pascal()?;
                let ty = type_name::type_ref_qualified(ty, namespace, context)?;
                let value = render_value(declaration.value(), value, namespace, context)?;
                let modifier = if is_compile_time_constant(declaration.value(), context) {
                    "public const"
                } else {
                    "public static readonly"
                };
                let documentation = Documentation::summary(declaration.meta().doc(), "        ");
                Ok(Self::Inline(format!(
                    "{documentation}        {modifier} {ty} {name} = {value};\n"
                )))
            }
            ConstantValueDecl::Accessor { symbol, callable } => Function::from_constant_accessor(
                declaration.name(),
                symbol,
                callable,
                bridge,
                context,
            )
            .map(|function| function.with_documentation(declaration.meta().doc()))
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

    fn render_member(&self) -> Result<String> {
        match self {
            Self::Inline(source) => Ok(source.clone()),
            Self::Accessor(function) => function.render_source(),
        }
    }
}

impl AssociatedConstants {
    pub fn from_owner(
        owner: ConstantOwner,
        namespace: &Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        context
            .associated_constants(owner)
            .map(|constant| Constant::from_declaration(constant, namespace, bridge, context))
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }

    pub fn members(&self) -> Result<Vec<String>> {
        self.0.iter().map(Constant::render_member).collect()
    }

    pub fn add_support(&self, emitted: Emitted) -> Result<Emitted> {
        self.0
            .iter()
            .try_fold(emitted, |emitted, constant| match constant {
                Constant::Inline(_) => Ok(emitted),
                Constant::Accessor(function) => function.add_support(emitted),
            })
    }
}

fn is_compile_time_constant(
    declaration: &ConstantValueDecl<Native>,
    context: &RenderContext<Native>,
) -> bool {
    let ConstantValueDecl::Inline { ty, value, .. } = declaration else {
        return false;
    };
    match value {
        DefaultValue::Bool(_) | DefaultValue::Float(_) | DefaultValue::String(_) => true,
        DefaultValue::Integer(_) => {
            !matches!(ty, TypeRef::Primitive(Primitive::ISize | Primitive::USize))
        }
        DefaultValue::EnumVariant { .. } => matches!(
            ty,
            TypeRef::Enum(id) if matches!(context.enumeration(*id), Some(EnumDecl::CStyle(_)))
        ),
        DefaultValue::Null => false,
        _ => false,
    }
}

fn render_value(
    declaration: &ConstantValueDecl<Native>,
    value: &DefaultValue,
    namespace: &Namespace,
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
        DefaultValue::EnumVariant { variant_name, .. } => {
            render_enum_variant(ty, variant_name, namespace, context)
        }
        DefaultValue::Null => Ok("null".to_owned()),
        _ => super::super::unsupported("unknown constant literal"),
    }
}

fn render_enum_variant(
    ty: &TypeRef,
    variant_name: &boltffi_binding::CanonicalName,
    namespace: &Namespace,
    context: &RenderContext<Native>,
) -> Result<String> {
    let TypeRef::Enum(id) = ty else {
        return super::super::unsupported("enum constant type");
    };
    let Some(enumeration) = context.enumeration(*id) else {
        return super::super::unsupported("missing enum constant declaration");
    };
    let ty = type_name::type_ref_qualified(ty, namespace, context)?;
    let variant = Name::new(variant_name).pascal()?;
    match enumeration {
        EnumDecl::CStyle(_) => Ok(format!("{ty}.{variant}")),
        EnumDecl::Data(_) => Ok(format!("new {ty}.{variant}()")),
        _ => super::super::unsupported("unknown enum constant type"),
    }
}

fn render_integer(ty: &TypeRef, value: i128) -> Result<String> {
    let TypeRef::Primitive(primitive) = ty else {
        return super::super::unsupported("integer constant type");
    };
    Ok(match primitive {
        Primitive::U32 => format!("{value}U"),
        Primitive::I64 => format!("{value}L"),
        Primitive::U64 => format!("{value}UL"),
        Primitive::ISize => format!("unchecked((nint){value}L)"),
        Primitive::USize => format!("unchecked((nuint){value}UL)"),
        _ => value.to_string(),
    })
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
