use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt,
};

use askama::Template as AskamaTemplate;
use boltffi_binding::{DeclarationRef, Native};

use crate::{
    bridge::jni::{JniBridgeContract, JvmClassPath},
    core::{
        AuxChunk, Diagnostic, FilePath, GeneratedFile, GeneratedOutput, HelperId, ImportDirective,
        RenderContext, RenderedDeclaration, Result, TextChunk,
    },
    target::java::{JavaFile, JavaHost, JavaPackage, runtime::Loader, syntax::TypeIdentifier},
};

use super::{Callback, Class, Closures, Enumeration, Record, ResultClass};

#[derive(AskamaTemplate)]
#[template(path = "target/java/module.java", escape = "none")]
struct ModuleTemplate<'module> {
    package: &'module JavaPackage,
    file: &'module JavaFile,
    native_owner: &'module TypeIdentifier,
    imports: Vec<MemberSource>,
    loader: MemberSource,
    native_methods: Vec<MemberSource>,
    helpers: Vec<MemberSource>,
    declarations: Vec<MemberSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MemberSource(TextChunk);

pub struct Module<'host, 'bridge, 'decl> {
    host: &'host JavaHost,
    bridge: &'bridge JniBridgeContract,
    context: &'decl RenderContext<'decl, Native>,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

#[derive(Default)]
struct ModuleChunks {
    imports: BTreeSet<ImportDirective>,
    forwards: Vec<TextChunk>,
    seen_forwards: BTreeSet<TextChunk>,
    helpers: BTreeMap<HelperId, TextChunk>,
    primary: Vec<TextChunk>,
    diagnostics: Vec<Diagnostic>,
}

impl<'host, 'bridge, 'decl> Module<'host, 'bridge, 'decl> {
    pub fn new(
        host: &'host JavaHost,
        bridge: &'bridge JniBridgeContract,
        context: &'decl RenderContext<'decl, Native>,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self {
            host,
            bridge,
            context,
            declarations,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let host = self.host;
        let native_owner = host.native_owner();
        let runtime_owner = host.runtime_owner();
        self.validate_bridge_owner(&native_owner)?;
        let closures = Closures::from_declarations(
            &self.declarations,
            self.bridge,
            host.java_version(),
            self.context,
        )?;
        let uses_result = self
            .declarations
            .iter()
            .any(|declaration| declaration.declaration().uses_result_codec());
        let (mut chunks, mut declaration_files) = ModuleChunks::from_declarations(
            self.declarations,
            host.package(),
            host.java_version(),
        )?;
        closures
            .render(host.package(), host.java_version())?
            .into_iter()
            .try_for_each(|(file, emitted)| {
                let (primary, aux, diagnostics) = emitted.into_parts();
                declaration_files.push(GeneratedFile::new(
                    FilePath::new(file.path(host.package()))?,
                    primary.as_str(),
                ));
                chunks.diagnostics.extend(diagnostics);
                aux.into_iter().try_for_each(|aux| chunks.push_aux(aux))
            })?;
        let loader = Loader::new(
            native_owner.identifier().clone(),
            runtime_owner.identifier().clone(),
            host.native_libraries(),
        );
        let contents = ModuleTemplate {
            package: host.package(),
            file: host.file(),
            native_owner: &native_owner,
            imports: chunks.imports(),
            loader: MemberSource::rendered(loader.render()?),
            native_methods: chunks.forwards(),
            helpers: chunks.helpers(),
            declarations: chunks.primary(),
        }
        .render()?;
        let mut files = vec![GeneratedFile::new(
            FilePath::new(host.file().path(host.package()))?,
            contents,
        )];
        files.append(&mut declaration_files);
        if let Some(runtime) = loader.desktop_source(host.package())? {
            files.push(GeneratedFile::new(
                FilePath::new(host.runtime_file().path(host.package()))?,
                runtime,
            ));
        }
        if uses_result {
            ResultClass::append(&mut files, host.package(), host.java_version())?;
        }

        Ok(GeneratedOutput::new(files, chunks.diagnostics))
    }

    fn validate_bridge_owner(&self, native_owner: &TypeIdentifier) -> Result<()> {
        let expected = JvmClassPath::new(self.host.package().to_string(), native_owner.as_str())?;
        match self.bridge.class() == &expected {
            true => Ok(()),
            false => Err(JavaHost::broken_bridge_contract(
                "JNI owner class matches the Java native owner",
            )),
        }
    }
}

impl ModuleChunks {
    fn from_declarations(
        declarations: Vec<RenderedDeclaration<'_, Native>>,
        package: &JavaPackage,
        version: crate::target::java::JavaVersion,
    ) -> Result<(Self, Vec<GeneratedFile>)> {
        declarations.into_iter().try_fold(
            (Self::default(), Vec::new()),
            |(mut chunks, mut files), declaration| {
                let (declaration, emitted) = declaration.into_parts();
                let (primary, aux, diagnostics) = emitted.into_parts();
                match declaration {
                    DeclarationRef::Record(record) if !primary.is_empty() => {
                        files.push(GeneratedFile::new(
                            FilePath::new(Record::file_for(record, version)?.path(package))?,
                            primary.as_str(),
                        ));
                    }
                    DeclarationRef::Enum(enumeration) if !primary.is_empty() => {
                        files.push(GeneratedFile::new(
                            FilePath::new(
                                Enumeration::file_for(enumeration, version)?.path(package),
                            )?,
                            primary.as_str(),
                        ));
                    }
                    DeclarationRef::Class(class) if !primary.is_empty() => {
                        files.push(GeneratedFile::new(
                            FilePath::new(Class::file_for(class, version)?.path(package))?,
                            primary.as_str(),
                        ));
                    }
                    DeclarationRef::Callback(callback) if !primary.is_empty() => {
                        files.push(GeneratedFile::new(
                            FilePath::new(Callback::file_for(callback, version)?.path(package))?,
                            primary.as_str(),
                        ));
                    }
                    _ if !primary.is_empty() => chunks.primary.push(primary),
                    _ => {}
                }
                chunks.diagnostics.extend(diagnostics);
                aux.into_iter().try_for_each(|aux| chunks.push_aux(aux))?;
                Ok((chunks, files))
            },
        )
    }

    fn imports(&self) -> Vec<MemberSource> {
        self.imports
            .iter()
            .map(ImportDirective::text)
            .cloned()
            .map(MemberSource)
            .collect()
    }

    fn forwards(&self) -> Vec<MemberSource> {
        self.forwards.iter().cloned().map(MemberSource).collect()
    }

    fn helpers(&self) -> Vec<MemberSource> {
        self.helpers.values().cloned().map(MemberSource).collect()
    }

    fn primary(&self) -> Vec<MemberSource> {
        self.primary.iter().cloned().map(MemberSource).collect()
    }

    fn push_aux(&mut self, aux: AuxChunk) -> Result<()> {
        match aux {
            AuxChunk::Import(import) => {
                self.imports.insert(import);
                Ok(())
            }
            AuxChunk::ForwardDecl(forward) => {
                if self.seen_forwards.insert(forward.clone()) {
                    self.forwards.push(forward);
                }
                Ok(())
            }
            AuxChunk::Helper { id, text } => match self.helpers.entry(id) {
                Entry::Vacant(entry) => {
                    entry.insert(text);
                    Ok(())
                }
                Entry::Occupied(entry) if entry.get() == &text => Ok(()),
                Entry::Occupied(_) => Err(JavaHost::broken_bridge_contract(
                    "Java helper identities have one source definition",
                )),
            },
        }
    }
}

impl MemberSource {
    fn rendered(source: String) -> Self {
        Self(TextChunk::new(source))
    }
}

impl fmt::Display for MemberSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{CanonicalName, DeclarationRef, Native, lower};

    use super::ModuleChunks;
    use crate::core::{
        AuxChunk, Diagnostic, Emitted, HelperId, ImportDirective, RenderedDeclaration, TextChunk,
    };
    use crate::target::java::{JavaPackage, JavaVersion};

    #[test]
    fn consumes_and_orders_every_emitted_chunk() {
        let file = syn::parse_str("#[export] pub fn value() -> i32 { 1 }")
            .expect("valid Java module fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("Java module fixture scans");
        let bindings = lower::<Native>(&source).expect("Java module fixture lowers");
        let declaration = DeclarationRef::from(
            bindings
                .decls()
                .first()
                .expect("Java module fixture declaration"),
        );
        let helper_a = HelperId::new(CanonicalName::single("a_helper"));
        let helper_z = HelperId::new(CanonicalName::single("z_helper"));
        let first = Emitted::primary("first\n")
            .with_aux(AuxChunk::Import(ImportDirective::new("import z;\n")))
            .with_aux(AuxChunk::ForwardDecl(TextChunk::new("native z;\n")))
            .with_aux(AuxChunk::Helper {
                id: helper_z,
                text: TextChunk::new("helper z\n"),
            })
            .with_diagnostics([Diagnostic::new("first diagnostic")]);
        let second = Emitted::primary("second\n")
            .with_aux(AuxChunk::Import(ImportDirective::new("import a;\n")))
            .with_aux(AuxChunk::Import(ImportDirective::new("import z;\n")))
            .with_aux(AuxChunk::ForwardDecl(TextChunk::new("native a;\n")))
            .with_aux(AuxChunk::ForwardDecl(TextChunk::new("native z;\n")))
            .with_aux(AuxChunk::Helper {
                id: helper_a,
                text: TextChunk::new("helper a\n"),
            })
            .with_diagnostics([Diagnostic::new("second diagnostic")]);
        let (chunks, files) = ModuleChunks::from_declarations(
            vec![
                RenderedDeclaration::new(declaration, first),
                RenderedDeclaration::new(declaration, second),
            ],
            &JavaPackage::parse("com.boltffi.demo").unwrap(),
            JavaVersion::JAVA_8,
        )
        .expect("Java module chunks assemble");
        assert!(files.is_empty());

        assert_eq!(
            chunks
                .imports
                .iter()
                .map(ImportDirective::text)
                .map(TextChunk::as_str)
                .collect::<Vec<_>>(),
            ["import a;\n", "import z;\n"]
        );
        assert_eq!(
            chunks
                .forwards
                .iter()
                .map(TextChunk::as_str)
                .collect::<Vec<_>>(),
            ["native z;\n", "native a;\n"]
        );
        assert_eq!(
            chunks
                .helpers
                .values()
                .map(TextChunk::as_str)
                .collect::<Vec<_>>(),
            ["helper a\n", "helper z\n"]
        );
        assert_eq!(
            chunks
                .primary
                .iter()
                .map(TextChunk::as_str)
                .collect::<Vec<_>>(),
            ["first\n", "second\n"]
        );
        assert_eq!(
            chunks
                .diagnostics
                .iter()
                .map(Diagnostic::message)
                .collect::<Vec<_>>(),
            ["first diagnostic", "second diagnostic"]
        );
    }
}
