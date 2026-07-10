use askama::Template;
use boltffi_binding::{CStyleEnumDecl, EnumDecl, Native, Primitive};

use crate::{
    bridge::c::{CBridgeContract, Type as CBridgeType},
    core::{Emitted, Error, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::{Identifier, TypeFragment},
};
use super::primitive_type;

#[derive(Template)]
#[template(path = "target/csharp/enumeration.cs", escape = "none")]
struct EnumerationTemplate<'enumeration> {
    enumeration: &'enumeration Enumeration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) struct Enumeration {
    namespace: Namespace,
    name: Identifier,
    underlying_type: TypeFragment,
    variants: Vec<Variant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Variant {
    name: Identifier,
    discriminant: i128,
}

impl Enumeration {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &EnumDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, namespace, bridge),
            EnumDecl::Data(_) => Err(Error::UnsupportedTarget {
                target: "csharp",
                shape: "data enums",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: "csharp enum",
                shape: "unknown enum declaration",
            }),
        }
    }

    fn from_c_style(
        declaration: &CStyleEnumDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
    ) -> Result<Self> {
        let primitive = declaration.repr().primitive();
        if matches!(primitive, Primitive::ISize | Primitive::USize) {
            return Err(Error::UnsupportedTarget {
                target: "csharp",
                shape: "pointer-width enum representation",
            });
        }
        let c_enum =
            bridge
                .source_c_style_enum(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "C-style enum is missing from the C bridge",
                })?;
        if c_enum.repr() != &CBridgeType::primitive(primitive)?
            || c_enum.variants().len() != declaration.variants().len()
        {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "C-style enum does not match the C bridge",
            });
        }
        let variants = declaration
            .variants()
            .iter()
            .zip(c_enum.variants())
            .map(|(variant, c_variant)| {
                if variant.discriminant().get() != c_variant.value() {
                    return Err(Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "C-style enum discriminant does not match the C bridge",
                    });
                }
                Ok(Variant {
                    name: Name::new(variant.name()).pascal()?,
                    discriminant: variant.discriminant().get(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            namespace,
            name: Name::new(declaration.name()).pascal()?,
            underlying_type: primitive_type(primitive),
            variants,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(
            EnumerationTemplate { enumeration: self }.render()?,
        ))
    }
}
