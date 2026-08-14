use std::collections::BTreeSet;

use boltffi_binding::{Bindings, DeclarationId, Native};

use crate::{
    bridge::c::CBridgeContract,
    core::{
        FileLayout, FilePath, FilePlan, GeneratedFile, GeneratedOutput, RenderedDeclaration, Result,
    },
};

use super::super::{DartHost, native};

const PRELUDE: &str = include_str!("../../../../templates/target/dart/prelude.dart");
const PUBSPEC: &str = include_str!("../../../../templates/target/dart/pubspec.yaml");
const BUILD_HOOK: &str = include_str!("../../../../templates/target/dart/build.dart");

pub struct Module<'host, 'bridge, 'decl> {
    host: &'host DartHost,
    bridge: &'bridge CBridgeContract,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

impl<'host, 'bridge, 'decl> Module<'host, 'bridge, 'decl> {
    pub fn new(
        host: &'host DartHost,
        bridge: &'bridge CBridgeContract,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self {
            host,
            bridge,
            declarations,
        }
    }

    pub fn render(self, bindings: &Bindings<Native>) -> Result<GeneratedOutput> {
        let rendered_declarations = self
            .declarations
            .iter()
            .map(|declaration| declaration.declaration().id())
            .collect::<BTreeSet<DeclarationId>>();
        let native_functions = self
            .bridge
            .support()
            .functions()
            .iter()
            .chain(self.bridge.functions().iter().filter(|function| {
                function
                    .source_declaration()
                    .is_some_and(|id| rendered_declarations.contains(&id))
            }))
            .map(native::declaration)
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        let mut preamble = PRELUDE.trim_end().to_owned();
        preamble.push_str("\n\n");
        preamble.push_str(&native_functions);
        preamble.push('\n');

        let package = self.host.package_for(bindings)?;
        let source_path = FilePath::new(format!("{package}/lib/{package}.dart"))?;
        let source = FileLayout::new()
            .with_file(FilePlan::all(source_path).with_preamble(preamble))
            .assemble_declarations(self.declarations)?;
        let artifact = self.host.artifact_for(bindings);
        let mut package_generated_files = vec![
            GeneratedFile::new(
                FilePath::new(format!("{package}/pubspec.yaml"))?,
                PUBSPEC.replace("{{ artifact_name }}", &package),
            ),
            GeneratedFile::new(
                FilePath::new(format!("{package}/hook/build.dart"))?,
                BUILD_HOOK.replace("{{ artifact_name }}", &artifact),
            ),
        ];
        // Always written, even when empty: generation never clears stale
        // files a previous run left behind, so a conditional write here
        // could leave an old `dart_shims.rs` compiled into the next build.
        let shim_source = super::shim::render_module_shim(self.bridge)?
            .unwrap_or_else(|| "// no qualifying Dart callback shims generated\n".to_string());
        package_generated_files.push(GeneratedFile::new(
            FilePath::new(format!("{package}/native/dart_shims.rs"))?,
            shim_source,
        ));
        let package_files = GeneratedOutput::new(package_generated_files, Vec::new());
        Ok(GeneratedOutput::combine([source, package_files]))
    }
}
