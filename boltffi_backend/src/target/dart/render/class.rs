use askama::Template;
use boltffi_binding::{ClassDecl, ConstantOwner, Native};

use crate::{
    bridge::c::CBridgeContract,
    core::{AuxChunk, Emitted, RenderContext, Result},
    target::dart::syntax::Identifier,
};

use super::function::{Placement, Receiver, associated_functions};
use super::{AssociatedConstants, Documentation, declaration_name, indent};

#[derive(Template)]
#[template(path = "target/dart/class.dart", escape = "none")]
struct ClassTemplate<'a> {
    class: &'a Class,
}

pub struct Class {
    documentation: Documentation,
    name: Identifier,
    release: Identifier,
    members: Vec<String>,
    helpers: Vec<(crate::core::HelperId, String)>,
}

impl Class {
    pub fn from_declaration(
        declaration: &ClassDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = declaration_name(declaration.name())?;
        let methods = associated_functions(
            declaration.initializers(),
            declaration.methods(),
            Placement::Initializer {
                owner: name.clone(),
                primary: true,
            },
            Receiver::Class,
            bridge,
            context,
        )?;
        let helpers = methods
            .iter()
            .flat_map(|method| method.helpers().iter().cloned())
            .collect();
        let methods = methods
            .iter()
            .map(|method| indent(&method.source(), 2))
            .collect::<Vec<_>>();
        let members = AssociatedConstants::from_owner(
            ConstantOwner::Class(declaration.id()),
            bridge,
            context,
        )?
        .iter()
        .map(|constant| indent(constant.source(), 2))
        .chain(methods)
        .collect();
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), 0),
            name,
            release: Identifier::parse(declaration.release().name().as_str())?,
            members,
            helpers,
        })
    }

    pub fn render(self) -> Emitted {
        let mut emitted = Emitted::primary(
            ClassTemplate { class: &self }
                .render()
                .expect("rendering an in-memory Dart class template cannot fail"),
        );
        for (id, text) in self.helpers {
            emitted = emitted.with_aux(AuxChunk::Helper {
                id,
                text: text.into(),
            });
        }
        emitted
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn release(&self) -> &Identifier {
        &self.release
    }

    fn members(&self) -> &[String] {
        &self.members
    }
}
