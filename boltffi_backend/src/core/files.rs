use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use boltffi_binding::{CanonicalName, DeclarationRef, Surface};

use crate::core::{CoverageReport, DeclarationLabel, Error, Result};

/// Path of one generated backend output file.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct FilePath(PathBuf);

impl FilePath {
    /// Creates a generated file path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            Err(Error::EmptyFilePath)
        } else {
            Ok(Self(path))
        }
    }

    /// Returns the path as a standard path value.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Rendered source text before it is assigned to a file.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct TextChunk {
    text: String,
}

impl TextChunk {
    /// Creates an empty text chunk.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates a text chunk from rendered source.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// Returns the rendered source text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns whether the chunk contains no source text.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Splits this chunk into its source text.
    pub fn into_string(self) -> String {
        self.text
    }
}

impl From<String> for TextChunk {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for TextChunk {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

/// Import or include directive emitted outside a declaration body.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct ImportDirective {
    text: TextChunk,
}

impl ImportDirective {
    /// Creates an import directive.
    pub fn new(text: impl Into<TextChunk>) -> Self {
        Self { text: text.into() }
    }

    /// Returns the directive source text.
    pub fn text(&self) -> &TextChunk {
        &self.text
    }
}

/// Identity of a helper emitted by a backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct HelperId {
    name: CanonicalName,
}

impl HelperId {
    /// Creates a helper identity from a canonical name.
    pub fn new(name: CanonicalName) -> Self {
        Self { name }
    }

    /// Returns the canonical helper name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }
}

/// Secondary source attached to an emitted declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AuxChunk {
    /// Import or include directive.
    Import(ImportDirective),
    /// Forward declaration text.
    ForwardDecl(TextChunk),
    /// Helper source identified for deduplication.
    Helper {
        /// Stable helper identity.
        id: HelperId,
        /// Helper source text.
        text: TextChunk,
    },
}

/// Diagnostic emitted while rendering generated output.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Diagnostic {
    message: String,
}

impl Diagnostic {
    /// Creates a diagnostic message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Rendered backend output before final file assembly.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Emitted {
    primary: TextChunk,
    aux: Vec<AuxChunk>,
    diagnostics: Vec<Diagnostic>,
}

impl Emitted {
    /// Creates rendered output from primary source text.
    pub fn primary(primary: impl Into<TextChunk>) -> Self {
        Self {
            primary: primary.into(),
            aux: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Creates rendered output from one diagnostic.
    pub fn diagnostic(diagnostic: Diagnostic) -> Self {
        Self {
            primary: TextChunk::empty(),
            aux: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }

    /// Returns the primary declaration source.
    pub fn primary_chunk(&self) -> &TextChunk {
        &self.primary
    }

    /// Returns auxiliary source chunks.
    pub fn aux_chunks(&self) -> &[AuxChunk] {
        &self.aux
    }

    /// Returns diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns whether the value carries no source chunks.
    pub fn is_empty(&self) -> bool {
        self.primary.is_empty() && self.aux.is_empty()
    }

    /// Adds one auxiliary source chunk.
    pub fn with_aux(mut self, aux: AuxChunk) -> Self {
        self.aux.push(aux);
        self
    }

    /// Adds diagnostics to this output.
    pub fn with_diagnostics(mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) -> Self {
        self.diagnostics.extend(diagnostics);
        self
    }

    /// Appends another rendered output value.
    pub fn append(&mut self, other: Self) {
        self.primary.text.push_str(other.primary.as_str());
        self.aux.extend(other.aux);
        self.diagnostics.extend(other.diagnostics);
    }

    /// Splits this value into primary source, auxiliary source, and diagnostics.
    pub fn into_parts(self) -> (TextChunk, Vec<AuxChunk>, Vec<Diagnostic>) {
        (self.primary, self.aux, self.diagnostics)
    }
}

/// Rendered output tied to the binding declaration that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RenderedDeclaration<'decl, S: Surface> {
    declaration: DeclarationRef<'decl, S>,
    emitted: Emitted,
}

impl<'decl, S: Surface> RenderedDeclaration<'decl, S> {
    /// Creates rendered output for a binding declaration.
    pub fn new(declaration: DeclarationRef<'decl, S>, emitted: Emitted) -> Self {
        Self {
            declaration,
            emitted,
        }
    }

    /// Returns the binding declaration that produced the output.
    pub fn declaration(&self) -> DeclarationRef<'decl, S> {
        self.declaration
    }

    /// Returns the rendered declaration output.
    pub fn emitted(&self) -> &Emitted {
        &self.emitted
    }

    /// Splits this value into declaration identity and emitted output.
    pub fn into_parts(self) -> (DeclarationRef<'decl, S>, Emitted) {
        (self.declaration, self.emitted)
    }
}

/// Declaration grouping rule for a generated file.
pub trait FileGroup {
    /// Returns whether a declaration belongs in the file.
    fn matches<'decl, S: Surface>(&self, declaration: DeclarationRef<'decl, S>) -> bool;
}

/// File group that accepts every declaration.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct AllDeclarations;

impl FileGroup for AllDeclarations {
    fn matches<'decl, S: Surface>(&self, _declaration: DeclarationRef<'decl, S>) -> bool {
        true
    }
}

/// Helper placement policy for one generated file.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum HelperPolicy {
    /// Deduplicate helpers inside each file.
    #[default]
    PerFile,
    /// Deduplicate helpers across the full generated output.
    PerOutput,
}

/// Fallback behavior for emitted declarations that match no file plan.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FallbackPolicy {
    /// Report an error for unmatched declarations.
    #[default]
    ErrorOnUnmatched,
    /// Emit unmatched declarations to one fallback file.
    EmitTo(FilePath),
}

/// Plan for one generated file.
pub struct FilePlan<G = AllDeclarations> {
    path: FilePath,
    group: G,
    preamble: TextChunk,
    postamble: TextChunk,
    helpers: HelperPolicy,
}

impl<G> FilePlan<G>
where
    G: FileGroup,
{
    /// Creates a file plan from a path and declaration group.
    pub fn new(path: FilePath, group: G) -> Self {
        Self {
            path,
            group,
            preamble: TextChunk::empty(),
            postamble: TextChunk::empty(),
            helpers: HelperPolicy::default(),
        }
    }

    /// Returns the generated file path.
    pub fn path(&self) -> &FilePath {
        &self.path
    }

    /// Returns the helper placement policy.
    pub fn helper_policy(&self) -> HelperPolicy {
        self.helpers
    }

    /// Sets source emitted before assembled chunks.
    pub fn with_preamble(mut self, preamble: impl Into<TextChunk>) -> Self {
        self.preamble = preamble.into();
        self
    }

    /// Sets source emitted after assembled chunks.
    pub fn with_postamble(mut self, postamble: impl Into<TextChunk>) -> Self {
        self.postamble = postamble.into();
        self
    }

    /// Sets the helper placement policy.
    pub fn with_helper_policy(mut self, helpers: HelperPolicy) -> Self {
        self.helpers = helpers;
        self
    }
}

impl FilePlan<AllDeclarations> {
    /// Creates a file plan that accepts every declaration.
    pub fn all(path: FilePath) -> Self {
        Self::new(path, AllDeclarations)
    }
}

/// Layout policy for generated backend files.
pub struct FileLayout<G = AllDeclarations> {
    files: Vec<FilePlan<G>>,
    fallback: FallbackPolicy,
}

impl<G> Default for FileLayout<G> {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            fallback: FallbackPolicy::default(),
        }
    }
}

impl<G> FileLayout<G>
where
    G: FileGroup,
{
    /// Creates an empty file layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a generated file plan.
    pub fn with_file(mut self, file: FilePlan<G>) -> Self {
        self.files.push(file);
        self
    }

    /// Sets fallback behavior for unmatched declarations.
    pub fn with_fallback(mut self, fallback: FallbackPolicy) -> Self {
        self.fallback = fallback;
        self
    }

    /// Returns generated file plans.
    pub fn files(&self) -> &[FilePlan<G>] {
        &self.files
    }

    /// Returns fallback behavior.
    pub fn fallback(&self) -> &FallbackPolicy {
        &self.fallback
    }

    /// Assembles anonymous emitted chunks into generated output.
    pub fn assemble(self, emitted: impl IntoIterator<Item = Emitted>) -> Result<GeneratedOutput> {
        FileAssembler::new(self).assemble(emitted)
    }

    /// Assembles declaration-bound chunks into generated output.
    pub fn assemble_declarations<'decl, S>(
        self,
        emitted: impl IntoIterator<Item = RenderedDeclaration<'decl, S>>,
    ) -> Result<GeneratedOutput>
    where
        S: Surface,
    {
        FileAssembler::new(self).assemble_declarations(emitted)
    }
}

impl FileLayout<AllDeclarations> {
    /// Creates a single-file layout.
    pub fn single(path: FilePath) -> Self {
        Self::new().with_file(FilePlan::all(path))
    }
}

/// Generated backend output assembled from bridge and host render chunks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct GeneratedOutput {
    files: Vec<GeneratedFile>,
    diagnostics: Vec<Diagnostic>,
    coverage: CoverageReport,
}

impl GeneratedOutput {
    /// Creates generated backend output.
    pub fn new(files: Vec<GeneratedFile>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            files,
            diagnostics,
            coverage: CoverageReport::new(),
        }
    }

    /// Creates empty generated backend output.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns generated files.
    pub fn files(&self) -> &[GeneratedFile] {
        &self.files
    }

    /// Returns non-fatal render diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns unsupported declarations found during rendering.
    pub const fn coverage(&self) -> &CoverageReport {
        &self.coverage
    }

    /// Attaches a coverage report to this output.
    pub fn with_coverage(mut self, coverage: CoverageReport) -> Self {
        self.coverage.append(coverage);
        self
    }

    /// Appends another generated output value.
    pub fn append(&mut self, other: Self) {
        let (files, diagnostics, coverage) = other.into_parts();
        files.into_iter().for_each(|file| self.insert_file(file));
        self.diagnostics.extend(diagnostics);
        self.coverage.append(coverage);
    }

    /// Combines multiple generated output values.
    pub fn combine(outputs: impl IntoIterator<Item = Self>) -> Self {
        outputs
            .into_iter()
            .fold(Self::empty(), |mut combined, output| {
                combined.append(output);
                combined
            })
    }

    /// Splits this output into generated files, diagnostics, and coverage.
    pub fn into_parts(self) -> (Vec<GeneratedFile>, Vec<Diagnostic>, CoverageReport) {
        (self.files, self.diagnostics, self.coverage)
    }

    fn insert_file(&mut self, file: GeneratedFile) {
        if let Some(existing) = self
            .files
            .iter_mut()
            .find(|existing| existing.path == file.path)
        {
            existing.contents.push_str(&file.contents);
        } else {
            self.files.push(file);
        }
    }
}

/// One complete generated backend file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct GeneratedFile {
    path: FilePath,
    contents: String,
}

impl GeneratedFile {
    /// Creates a generated file.
    pub fn new(path: FilePath, contents: impl Into<String>) -> Self {
        Self {
            path,
            contents: contents.into(),
        }
    }

    /// Returns the generated file path.
    pub const fn path(&self) -> &FilePath {
        &self.path
    }

    /// Returns the generated file contents.
    pub fn contents(&self) -> &str {
        &self.contents
    }
}

/// Assembler for emitted chunks and file layout.
pub struct FileAssembler<G = AllDeclarations> {
    layout: FileLayout<G>,
}

impl<G> FileAssembler<G>
where
    G: FileGroup,
{
    /// Creates a file assembler.
    pub fn new(layout: FileLayout<G>) -> Self {
        Self { layout }
    }

    /// Assembles anonymous emitted chunks.
    pub fn assemble(self, emitted: impl IntoIterator<Item = Emitted>) -> Result<GeneratedOutput> {
        let mut assembly = FileAssembly::from_plans(self.layout.files);
        let diagnostics = emitted
            .into_iter()
            .try_fold(Vec::new(), |mut diagnostics, item| {
                diagnostics.extend(item.diagnostics().iter().cloned());
                if !item.is_empty() {
                    assembly.push_anonymous(item)?;
                }
                Ok::<_, Error>(diagnostics)
            })?;
        Ok(GeneratedOutput::new(assembly.finish(), diagnostics))
    }

    /// Assembles declaration-bound emitted chunks.
    pub fn assemble_declarations<'decl, S>(
        self,
        emitted: impl IntoIterator<Item = RenderedDeclaration<'decl, S>>,
    ) -> Result<GeneratedOutput>
    where
        S: Surface,
    {
        let fallback = self.layout.fallback;
        let mut assembly = FileAssembly::from_plans(self.layout.files);
        let diagnostics = emitted
            .into_iter()
            .try_fold(Vec::new(), |mut diagnostics, item| {
                let (declaration, emitted) = item.into_parts();
                diagnostics.extend(emitted.diagnostics().iter().cloned());
                let path = assembly
                    .matching_path(declaration)
                    .or_else(|| fallback.path())
                    .cloned()
                    .ok_or_else(|| Error::UnmatchedFilePlan {
                        declaration: DeclarationLabel::from_ref(declaration).kind(),
                    })?;
                assembly.push(path, emitted);
                Ok::<_, Error>(diagnostics)
            })?;
        Ok(GeneratedOutput::new(assembly.finish(), diagnostics))
    }
}

struct FileAssembly<G> {
    files: Vec<FileAssemblyEntry<G>>,
    output_helpers: BTreeMap<HelperId, TextChunk>,
}

impl<G> FileAssembly<G>
where
    G: FileGroup,
{
    fn from_plans(plans: Vec<FilePlan<G>>) -> Self {
        Self {
            files: plans.into_iter().map(FileAssemblyEntry::new).collect(),
            output_helpers: BTreeMap::new(),
        }
    }

    fn matching_path<'decl, S: Surface>(
        &self,
        declaration: DeclarationRef<'decl, S>,
    ) -> Option<&FilePath> {
        self.files
            .iter()
            .find(|file| file.matches(declaration))
            .map(FileAssemblyEntry::path)
    }

    fn push_anonymous(&mut self, emitted: Emitted) -> Result<()> {
        match self.files.len() {
            1 => {
                let path = self.files[0].path().clone();
                self.push(path, emitted);
                Ok(())
            }
            _ => Err(Error::AnonymousOutputNeedsSingleFile),
        }
    }

    fn push(&mut self, path: FilePath, emitted: Emitted) {
        let index = self.entry_index(path);
        self.files[index].push(emitted, &mut self.output_helpers);
    }

    fn finish(self) -> Vec<GeneratedFile> {
        self.files
            .into_iter()
            .filter_map(FileAssemblyEntry::finish)
            .collect()
    }

    fn entry_index(&mut self, path: FilePath) -> usize {
        self.files
            .iter()
            .position(|file| file.path == path)
            .unwrap_or_else(|| {
                let index = self.files.len();
                self.files.push(FileAssemblyEntry::fallback(path));
                index
            })
    }
}

struct FileAssemblyEntry<G> {
    group: Option<G>,
    path: FilePath,
    preamble: TextChunk,
    postamble: TextChunk,
    helper_policy: HelperPolicy,
    imports: BTreeSet<ImportDirective>,
    forwards: Vec<TextChunk>,
    primary: Vec<TextChunk>,
    helper_chunks: BTreeMap<HelperId, TextChunk>,
}

impl<G> FileAssemblyEntry<G>
where
    G: FileGroup,
{
    fn new(plan: FilePlan<G>) -> Self {
        Self {
            group: Some(plan.group),
            path: plan.path,
            preamble: plan.preamble,
            postamble: plan.postamble,
            helper_policy: plan.helpers,
            imports: BTreeSet::new(),
            forwards: Vec::new(),
            primary: Vec::new(),
            helper_chunks: BTreeMap::new(),
        }
    }

    fn fallback(path: FilePath) -> Self {
        Self {
            group: None,
            path,
            preamble: TextChunk::empty(),
            postamble: TextChunk::empty(),
            helper_policy: HelperPolicy::default(),
            imports: BTreeSet::new(),
            forwards: Vec::new(),
            primary: Vec::new(),
            helper_chunks: BTreeMap::new(),
        }
    }

    fn path(&self) -> &FilePath {
        &self.path
    }

    fn matches<'decl, S: Surface>(&self, declaration: DeclarationRef<'decl, S>) -> bool {
        self.group
            .as_ref()
            .is_some_and(|group| group.matches(declaration))
    }

    fn push(&mut self, emitted: Emitted, output_helpers: &mut BTreeMap<HelperId, TextChunk>) {
        let (primary, aux, _) = emitted.into_parts();
        aux.into_iter()
            .for_each(|aux| self.push_aux(aux, output_helpers));
        if !primary.is_empty() {
            self.primary.push(primary);
        }
    }

    fn finish(self) -> Option<GeneratedFile> {
        let path = self.path.clone();
        let contents = self.contents();
        (!contents.is_empty()).then(|| GeneratedFile::new(path, contents))
    }

    fn push_aux(&mut self, aux: AuxChunk, output_helpers: &mut BTreeMap<HelperId, TextChunk>) {
        match aux {
            AuxChunk::Import(import) => {
                self.imports.insert(import);
            }
            AuxChunk::ForwardDecl(forward) => {
                self.forwards.push(forward);
            }
            AuxChunk::Helper { id, text } => match self.helper_policy {
                HelperPolicy::PerFile => {
                    self.helper_chunks.entry(id).or_insert(text);
                }
                HelperPolicy::PerOutput => {
                    if !output_helpers.contains_key(&id) {
                        output_helpers.insert(id.clone(), text.clone());
                        self.helper_chunks.insert(id, text);
                    }
                }
            },
        }
    }

    fn contents(self) -> String {
        [
            vec![self.preamble],
            self.imports
                .into_iter()
                .map(|import| import.text)
                .collect::<Vec<_>>(),
            self.forwards,
            self.primary,
            self.helper_chunks.into_values().collect::<Vec<_>>(),
            vec![self.postamble],
        ]
        .into_iter()
        .flatten()
        .filter(|chunk| !chunk.is_empty())
        .map(TextChunk::into_string)
        .collect::<Vec<_>>()
        .join("")
    }
}

trait FallbackPath {
    fn path(&self) -> Option<&FilePath>;
}

impl FallbackPath for FallbackPolicy {
    fn path(&self) -> Option<&FilePath> {
        match self {
            Self::ErrorOnUnmatched => None,
            Self::EmitTo(path) => Some(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, CanonicalName, Decl, DeclarationRef, Native, Surface, lower};

    use super::{
        AuxChunk, Emitted, FileGroup, FileLayout, FilePath, FilePlan, HelperId, HelperPolicy,
        RenderedDeclaration, TextChunk,
    };
    use crate::core::Error;

    #[derive(Clone, Copy)]
    enum OutputFile {
        Records,
        Functions,
    }

    impl FileGroup for OutputFile {
        fn matches<'decl, S: Surface>(&self, declaration: DeclarationRef<'decl, S>) -> bool {
            matches!(
                (self, declaration),
                (Self::Records, DeclarationRef::Record(_))
                    | (Self::Functions, DeclarationRef::Function(_))
            )
        }
    }

    fn demo_bindings() -> Bindings<Native> {
        let file = syn::parse_str(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[export]
            pub fn origin() -> Point {
                Point { x: 0.0, y: 0.0 }
            }
            "#,
        )
        .expect("valid source fixture");
        let source = boltffi_test_source::scan_file(file, PackageInfo::new("demo", None))
            .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn rendered<'decl>(
        declarations: impl IntoIterator<Item = &'decl Decl<Native>>,
    ) -> Vec<RenderedDeclaration<'decl, Native>> {
        declarations
            .into_iter()
            .map(|declaration| {
                let declaration = DeclarationRef::from(declaration);
                let text = match declaration {
                    DeclarationRef::Record(_) => "record\n",
                    DeclarationRef::Function(_) => "function\n",
                    DeclarationRef::Enum(_)
                    | DeclarationRef::Class(_)
                    | DeclarationRef::Callback(_)
                    | DeclarationRef::Stream(_)
                    | DeclarationRef::Constant(_)
                    | DeclarationRef::CustomType(_) => "",
                };
                RenderedDeclaration::new(declaration, Emitted::primary(text))
            })
            .collect()
    }

    #[test]
    fn declaration_layout_routes_each_decl_to_its_file() {
        let bindings = demo_bindings();
        let output = FileLayout::new()
            .with_file(FilePlan::new(
                FilePath::new("records.swift").expect("file path"),
                OutputFile::Records,
            ))
            .with_file(FilePlan::new(
                FilePath::new("functions.swift").expect("file path"),
                OutputFile::Functions,
            ))
            .assemble_declarations(rendered(bindings.decls()))
            .expect("layout should assemble");

        let files = output.files();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path().as_path(), Path::new("records.swift"));
        assert_eq!(files[0].contents(), "record\n");
        assert_eq!(files[1].path().as_path(), Path::new("functions.swift"));
        assert_eq!(files[1].contents(), "function\n");
    }

    #[test]
    fn unmatched_declaration_reports_file_plan_error() {
        let bindings = demo_bindings();
        let error = FileLayout::new()
            .with_file(FilePlan::new(
                FilePath::new("records.swift").expect("file path"),
                OutputFile::Records,
            ))
            .assemble_declarations(rendered(bindings.decls()))
            .expect_err("function has no matching file plan");

        assert!(matches!(
            error,
            Error::UnmatchedFilePlan {
                declaration: "function"
            }
        ));
    }

    #[test]
    fn single_file_layout_deduplicates_helpers() {
        let helper = HelperId::new(CanonicalName::single("decode_helper"));
        let file = FilePath::new("bridge.c").expect("file path");
        let output = FileLayout::new()
            .with_file(FilePlan::all(file).with_helper_policy(HelperPolicy::PerFile))
            .assemble([
                Emitted::primary("first\n").with_aux(AuxChunk::Helper {
                    id: helper.clone(),
                    text: TextChunk::new("helper\n"),
                }),
                Emitted::primary("second\n").with_aux(AuxChunk::Helper {
                    id: helper,
                    text: TextChunk::new("helper\n"),
                }),
            ])
            .expect("layout should assemble");

        let files = output.files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].contents(), "first\nsecond\nhelper\n");
    }
}
