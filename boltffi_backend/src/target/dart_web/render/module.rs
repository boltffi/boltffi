use super::*;

pub struct Module<'m> {
    name: &'m str,
    namespace: &'m str,
}

impl<'m> Module<'m> {
    pub fn new(name: &'m str, namespace: &'m str) -> Self {
        Self { name, namespace }
    }

    // Kept as one function so the loader script and this renderer can't
    // drift on the global name independently.
    pub fn ready_global(namespace: &str) -> String {
        format!("{namespace}_ready")
    }

    pub fn render<'decl>(
        &self,
        declarations: Vec<RenderedDeclaration<'decl, Wasm32>>,
    ) -> Result<GeneratedOutput> {
        let ready_global = Self::ready_global(self.namespace);
        let namespace = self.namespace;
        let preamble = include_str!("../../../../templates/target/dart_web/prelude.dart")
            .replace(
                "__SHARED_VALUES__",
                include_str!("../../../../templates/target/dart/shared_values.dart"),
            )
            .replace(
                "__SHARED_COMPARE__",
                include_str!("../../../../templates/target/dart/shared_compare.dart"),
            )
            .replace("__READY_GLOBAL__", &ready_global)
            .replace("__NAMESPACE__", namespace);
        FileLayout::new()
            .with_file(
                FilePlan::all(FilePath::new(format!("{}.dart", self.name))?)
                    .with_preamble(preamble),
            )
            .assemble_declarations(declarations)
    }
}
