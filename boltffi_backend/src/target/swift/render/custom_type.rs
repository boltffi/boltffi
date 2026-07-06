use askama::Template;
use boltffi_binding::{CustomTypeDecl, Native};

use crate::{
    core::{Emitted, RenderContext, Result},
    target::swift::{
        name_style::Name,
        render::{Documentation, SwiftType},
        syntax::TypeName,
    },
};

#[derive(Template)]
#[template(path = "target/swift/custom_type.swift", escape = "none")]
struct CustomTypeTemplate<'a> {
    custom_type: &'a CustomType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomType {
    documentation: Documentation,
    name: TypeName,
    representation: Option<TypeName>,
}

impl CustomType {
    pub fn from_declaration(
        declaration: &CustomTypeDecl,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), ""),
            name: Name::new(declaration.name()).type_name(),
            representation: context
                .custom_type_mapping(declaration.id())
                .is_none()
                .then(|| SwiftType::type_ref(declaration.representation(), context))
                .transpose()?,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        if self.representation.is_none() {
            return Ok(Emitted::primary(String::new()));
        }

        let mut source = CustomTypeTemplate { custom_type: self }
            .render()?
            .trim()
            .to_owned();
        source.push_str("\n\n");
        Ok(Emitted::primary(source))
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &TypeName {
        &self.name
    }

    fn representation(&self) -> &TypeName {
        self.representation
            .as_ref()
            .expect("unmapped custom type has a representation alias")
    }

    fn has_representation_alias(&self) -> bool {
        self.representation.is_some()
    }
}
