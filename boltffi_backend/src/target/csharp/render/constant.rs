use boltffi_binding::{
    ConstantDecl, ConstantOwner, ConstantValueDecl, DefaultValue, EnumDecl, Native, Primitive,
    TypeRef,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, RenderContext, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    type_name,
};
use super::{Documentation, Function, default_value::DefaultExpression};

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
                let rendered_type = type_name::type_ref_qualified(ty, namespace, context)?;
                let value =
                    DefaultExpression::render(ty, value, Some(namespace), context)?.to_string();
                let modifier = if is_compile_time_constant(declaration.value(), context) {
                    "public const"
                } else {
                    "public static readonly"
                };
                let documentation = Documentation::summary(declaration.meta().doc(), "        ");
                Ok(Self::Inline(format!(
                    "{documentation}        {modifier} {rendered_type} {name} = {value};\n"
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
