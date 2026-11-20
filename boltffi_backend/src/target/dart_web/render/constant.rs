use super::*;

// Accessor constants need a module-init hook this target doesn't have yet.
pub struct Constant {
    source: String,
}

impl Constant {
    pub fn from_declaration(
        decl: &ConstantDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        if decl.owner().is_some() {
            return Err(unsupported("associated constant"));
        }
        let name = Name::new(decl.name()).dart_constant_name();
        let source = match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                let dart_type = interop::dart_type(ty, context)?;
                let literal = web_default(ty, value, context)?
                    .into_constant()?
                    .to_string();
                let literal = literal.strip_prefix("const ").unwrap_or(&literal);
                format!("const {dart_type} {name} = {literal};\n\n")
            }
            ConstantValueDecl::Accessor { .. } => return Err(unsupported("accessor constant")),
            _ => return Err(unsupported("constant value shape")),
        };
        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}
