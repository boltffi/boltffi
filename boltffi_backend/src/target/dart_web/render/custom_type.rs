use super::*;

pub struct CustomType {
    source: String,
}

impl CustomType {
    pub fn from_declaration(
        decl: &CustomTypeDecl,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(decl.name()).dart_type_name();
        let representation = interop::dart_type(decl.representation(), context)?;
        Ok(Self {
            source: format!("typedef {name} = {representation};\n\n"),
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}
