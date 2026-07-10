use askama::Template;
use boltffi_binding::{
    CStyleEnumDecl, CanonicalName, DirectValueType, EnumDecl, Native, Primitive,
};

use crate::{
    bridge::c::{CBridgeContract, Type as CBridgeType},
    core::{Diagnostic, Emitted, Error, RenderContext, Result},
};

use super::super::{
    name_style::{Name, Namespace},
    syntax::{Identifier, TypeFragment},
};
use super::{Function, primitive_type};

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
    methods: Vec<Function>,
    diagnostics: Vec<Diagnostic>,
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
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => {
                Self::from_c_style(enumeration, namespace, bridge, context)
            }
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
        context: &RenderContext<Native>,
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
        let name = Name::new(declaration.name()).pascal()?;
        let owner = DirectValueType::Enum(declaration.id());
        let mut methods = Vec::new();
        let mut diagnostics = Vec::new();
        for initializer in declaration.initializers() {
            collect_associated(
                &mut methods,
                &mut diagnostics,
                "initializer",
                initializer.name(),
                Function::from_initializer(
                    initializer,
                    owner.clone(),
                    &name,
                    true,
                    bridge,
                    context,
                ),
            )?;
        }
        for method in declaration.methods() {
            collect_associated(
                &mut methods,
                &mut diagnostics,
                "method",
                method.name(),
                Function::from_method(method, owner.clone(), &name, true, bridge, context),
            )?;
        }
        Ok(Self {
            namespace,
            name,
            underlying_type: primitive_type(primitive),
            variants,
            methods,
            diagnostics,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let mut emitted = Emitted::primary(EnumerationTemplate { enumeration: self }.render()?)
            .with_diagnostics(self.diagnostics.iter().cloned());
        for method in &self.methods {
            let (_, aux, diagnostics) = method.render()?.into_parts();
            for chunk in aux {
                emitted = emitted.with_aux(chunk);
            }
            emitted = emitted.with_diagnostics(diagnostics);
        }
        Ok(emitted)
    }
}

fn collect_associated(
    methods: &mut Vec<Function>,
    diagnostics: &mut Vec<Diagnostic>,
    kind: &'static str,
    name: &CanonicalName,
    result: Result<Function>,
) -> Result<()> {
    match result {
        Ok(function) => methods.push(function),
        Err(Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape }) => {
            diagnostics.push(Diagnostic::new(format!(
                "{kind} {}: {shape}",
                Name::new(name).pascal()?
            )));
        }
        Err(error) => return Err(error),
    }
    Ok(())
}
