use askama::Template as AskamaTemplate;
use boltffi_binding::{CustomTypeDecl, Wasm32};

use crate::core::{Emitted, RenderContext, Result};

use super::super::{name_style::Name, syntax::TypeName};
use super::Type;

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/custom_type.ts", escape = "none")]
pub struct CustomType {
    name: TypeName,
    representation: TypeName,
}

impl CustomType {
    pub fn from_declaration(
        declaration: &CustomTypeDecl,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Ok(Self {
            name: Name::new(declaration.name()).type_name(),
            representation: Type::from_ref(declaration.representation(), context)?,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?))
    }
}
