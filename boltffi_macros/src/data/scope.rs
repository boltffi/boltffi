use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use boltffi_ast::{EnumId, RecordId, SourceContract, SourceFile, SourceSpan};
use proc_macro2::LineColumn;
use syn::visit::Visit;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum DeclarationKind {
    Record,
    Enumeration,
}

pub enum DataId {
    Record(RecordId),
    Enumeration(EnumId),
}

pub struct Declaration {
    name: String,
    kind: DeclarationKind,
    source: PathBuf,
    module_path: Vec<String>,
    local_scope: Option<syn::File>,
}

#[derive(Clone)]
enum Scope {
    Module(Vec<String>),
    Block(Vec<syn::Item>),
}

struct ScopeFinder<'target> {
    name: &'target str,
    kind: DeclarationKind,
    target_ordinal: usize,
    observed: usize,
    current: Scope,
    scope: Option<Scope>,
}

/// Where each declaration in one source file sits, without anything the
/// proc-macro bridge owns.
///
/// A `syn::File` cannot outlive the `#[data]` invocation that parsed it: its
/// `Ident`s borrow symbols the bridge frees when the invocation ends, so
/// keeping one is a use-after-free, not merely a lifetime inconvenience. What
/// survives is what `from_macro_input` actually needs out of the parse — the
/// text, and every declaration's module path as plain `String`s.
struct FileIndex {
    text: String,
    /// `(name, kind)` -> the scope of each declaration of that name, in visit
    /// order, so a lookup by ordinal answers what `ScopeFinder` would. `None`
    /// marks one declared inside a block: its items are token-backed, so that
    /// case re-parses instead.
    scopes: HashMap<(String, DeclarationKind), Vec<Option<Vec<String>>>>,
    /// Byte offset of the start of every line, so `offset_of` can reach the
    /// line a span names without counting newlines from the top each time.
    line_starts: Vec<usize>,
    /// `(name, kind)` -> the byte range of each declaration's name, in lexical
    /// order. `ordinal_of` finds the range holding a span's offset; its position
    /// in the list is the ordinal `scopes` is keyed by. Names are stored with
    /// any `r#` stripped, as the lookup strips it too.
    declarations: HashMap<(String, DeclarationKind), Vec<(usize, usize)>>,
}

impl FileIndex {
    fn new(text: String) -> syn::Result<Self> {
        // The parse is dropped at the end of this scope, inside the invocation
        // that made it. Everything kept owns its data outright.
        let scopes = ScopeIndexer::index(&syn::parse_file(&text)?);
        let line_starts = std::iter::once(0)
            .chain(
                text.bytes()
                    .enumerate()
                    .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
            )
            .collect();
        let declarations = Self::lex_declarations(&text);
        Ok(Self {
            text,
            scopes,
            line_starts,
            declarations,
        })
    }

    /// Every `struct`/`enum` name in `text`, by name and kind, in lexical order.
    ///
    /// One pass answers every declaration in the file; the alternative is a
    /// lex of the whole file per `#[data]`, which is quadratic in a generated
    /// file holding hundreds of them.
    fn lex_declarations(text: &str) -> HashMap<(String, DeclarationKind), Vec<(usize, usize)>> {
        let mut declarations: HashMap<(String, DeclarationKind), Vec<(usize, usize)>> =
            HashMap::new();
        let mut declares: Option<DeclarationKind> = None;
        let mut offset = 0;
        for token in rustc_lexer::tokenize(text) {
            let start = offset;
            offset += token.len;
            let spelling = &text[start..offset];
            match token.kind {
                rustc_lexer::TokenKind::Whitespace
                | rustc_lexer::TokenKind::LineComment
                | rustc_lexer::TokenKind::BlockComment { .. } => {}
                rustc_lexer::TokenKind::Ident if spelling == "struct" => {
                    declares = Some(DeclarationKind::Record);
                }
                rustc_lexer::TokenKind::Ident if spelling == "enum" => {
                    declares = Some(DeclarationKind::Enumeration);
                }
                rustc_lexer::TokenKind::Ident | rustc_lexer::TokenKind::RawIdent => {
                    if let Some(kind) = declares.take() {
                        let name = spelling.strip_prefix("r#").unwrap_or(spelling).to_owned();
                        declarations
                            .entry((name, kind))
                            .or_default()
                            .push((start, spelling.len()));
                    }
                }
                _ => declares = None,
            }
        }
        declarations
    }

    /// Byte offset of a `proc_macro::Span` location.
    ///
    /// The compiler counts both fields from one, and counts the column in
    /// characters while everything downstream works in bytes. Adding the
    /// column to a byte offset is only correct for a line that is entirely
    /// ASCII up to the declaration; `pub /* \u{3b1} */ struct S` is off by the
    /// extra byte, and the identifier is missed.
    fn offset_of(&self, location: LineColumn) -> Option<usize> {
        let line_start = *self.line_starts.get(location.line.checked_sub(1)?)?;
        let line = self.text[line_start..]
            .split_once('\n')
            .map_or(&self.text[line_start..], |(line, _)| line);
        let column = line
            .char_indices()
            .nth(location.column.checked_sub(1)?)
            .map(|(column, _)| column)?;
        Some(line_start + column)
    }

    /// Which declaration of `name` the span at `target_offset` is, counting the
    /// ones `scopes` counts.
    fn ordinal_of(&self, name: &str, kind: DeclarationKind, target_offset: usize) -> Option<usize> {
        let name = name.strip_prefix("r#").unwrap_or(name);
        self.declarations
            .get(&(name.to_owned(), kind))?
            .iter()
            .position(|&(offset, len)| offset <= target_offset && target_offset < offset + len)
    }
}

thread_local! {
    /// Files already indexed, by path, each holding the text it was built
    /// from so `file_index` can tell whether it still describes the file.
    ///
    /// `#[data]` expands once per mirrored type, and every expansion in a file
    /// wants the same scope information out of it, so a file is parsed on the
    /// first one and answered from here for the rest.
    ///
    /// Thread-local because it is written from the macro's own thread and needs
    /// no sharing; the entries themselves own their data outright.
    static FILE_INDEXES: RefCell<HashMap<PathBuf, Rc<FileIndex>>> =
        RefCell::new(HashMap::new());
}

/// `path`, read and indexed, reusing the cached index while the file is
/// unchanged.
///
/// The text is read on every call and compared, because a macro host can
/// outlive the compilation that filled this cache: rust-analyzer keeps the
/// expander loaded across edits, and an index matched on path alone would
/// answer a post-edit expansion with pre-edit text and scopes — a declaration
/// resolved into the module it used to be in, or an unlocatable one. Reading is
/// what every invocation did before there was a cache; the parse is what it
/// saves.
///
/// A failure is not cached: it is reported once per declaration either way.
fn file_index(path: &Path) -> syn::Result<Rc<FileIndex>> {
    let text = fs::read_to_string(path).map_err(|error| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("read data source `{}`: {error}", path.display()),
        )
    })?;
    if let Some(cached) = FILE_INDEXES.with(|files| files.borrow().get(path).cloned())
        && cached.text == text
    {
        return Ok(cached);
    }
    let index = Rc::new(FileIndex::new(text)?);
    FILE_INDEXES.with(|files| {
        files
            .borrow_mut()
            .insert(path.to_path_buf(), Rc::clone(&index))
    });
    Ok(index)
}

/// The scope an indexed declaration is in. `ScopeFinder`'s `Scope`, minus the
/// block's items, which is exactly the part that cannot be cached.
#[derive(Clone)]
enum IndexScope {
    Module(Vec<String>),
    Block,
}

/// Records every declaration's scope in one walk, counting names the way
/// `ScopeFinder` counts the one it is looking for.
struct ScopeIndexer {
    current: IndexScope,
    scopes: HashMap<(String, DeclarationKind), Vec<Option<Vec<String>>>>,
}

impl ScopeIndexer {
    fn index(syntax: &syn::File) -> HashMap<(String, DeclarationKind), Vec<Option<Vec<String>>>> {
        let mut indexer = Self {
            current: IndexScope::Module(Vec::new()),
            scopes: HashMap::new(),
        };
        indexer.visit_file(syntax);
        indexer.scopes
    }

    fn observe(&mut self, kind: DeclarationKind, name: &syn::Ident) {
        let scope = match &self.current {
            IndexScope::Module(path) => Some(path.clone()),
            IndexScope::Block => None,
        };
        self.scopes
            .entry((name.to_string(), kind))
            .or_default()
            .push(scope);
    }
}

impl<'syntax> Visit<'syntax> for ScopeIndexer {
    fn visit_file(&mut self, syntax: &'syntax syn::File) {
        self.current = IndexScope::Module(Vec::new());
        syntax.items.iter().for_each(|item| self.visit_item(item));
    }

    fn visit_item_mod(&mut self, module: &'syntax syn::ItemMod) {
        let Some((_, items)) = &module.content else {
            return;
        };
        let nested = match &self.current {
            IndexScope::Module(path) => IndexScope::Module(
                path.iter()
                    .cloned()
                    .chain(std::iter::once(module.ident.to_string()))
                    .collect(),
            ),
            IndexScope::Block => IndexScope::Block,
        };
        let enclosing = std::mem::replace(&mut self.current, nested);
        items.iter().for_each(|item| self.visit_item(item));
        self.current = enclosing;
    }

    fn visit_block(&mut self, block: &'syntax syn::Block) {
        let enclosing = std::mem::replace(&mut self.current, IndexScope::Block);
        block
            .stmts
            .iter()
            .for_each(|statement| self.visit_stmt(statement));
        self.current = enclosing;
    }

    fn visit_item_struct(&mut self, item: &'syntax syn::ItemStruct) {
        self.observe(DeclarationKind::Record, &item.ident);
    }

    fn visit_item_enum(&mut self, item: &'syntax syn::ItemEnum) {
        self.observe(DeclarationKind::Enumeration, &item.ident);
    }
}

impl Declaration {
    pub fn from_macro_input(item: &proc_macro::TokenStream) -> syn::Result<Self> {
        let parsed = syn::parse::<syn::Item>(item.clone())?;
        let (name, kind, invocation) = match parsed {
            syn::Item::Struct(item) => (
                item.ident.to_string(),
                DeclarationKind::Record,
                item.ident.span().unwrap(),
            ),
            syn::Item::Enum(item) => (
                item.ident.to_string(),
                DeclarationKind::Enumeration,
                item.ident.span().unwrap(),
            ),
            item => {
                return Err(syn::Error::new_spanned(
                    item,
                    "data runtime requires a struct or enum declaration",
                ));
            }
        };
        let source = invocation.local_file().ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "data source file is unavailable",
            )
        })?;
        let location = LineColumn {
            line: invocation.line(),
            column: invocation.column(),
        };
        let unlocatable = || {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "locate data declaration `{name}` at {}:{} in `{}`",
                    location.line,
                    location.column,
                    source.display()
                ),
            )
        };
        let index = file_index(&source)?;
        let invocation_offset = index.offset_of(location).ok_or_else(unlocatable)?;
        let target_ordinal = index
            .ordinal_of(&name, kind, invocation_offset)
            .ok_or_else(unlocatable)?;
        let indexed = index
            .scopes
            .get(&(name.clone(), kind))
            .and_then(|scopes| scopes.get(target_ordinal));
        let (module_path, local_scope) = match indexed {
            Some(Some(module_path)) => (module_path.clone(), None),
            // Block-scoped, or not in the index at all: the items a block scope
            // carries are token-backed and cannot outlive this invocation, so
            // that one file is parsed again here. `ScopeFinder` also owns the
            // not-found case, which keeps the error identical either way.
            Some(None) | None => {
                let syntax = syn::parse_file(&index.text)?;
                match ScopeFinder::find(&syntax, &name, kind, target_ordinal)
                    .ok_or_else(unlocatable)?
                {
                    Scope::Module(module_path) => (module_path, None),
                    Scope::Block(items) => (
                        Vec::new(),
                        Some(syn::File {
                            shebang: None,
                            attrs: Vec::new(),
                            items,
                        }),
                    ),
                }
            }
        };
        Ok(Self {
            name,
            kind,
            source,
            module_path,
            local_scope,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    pub fn local_scope(&self) -> Option<&syn::File> {
        self.local_scope.as_ref()
    }

    pub fn resolve<'source>(
        &self,
        contract: &SourceContract,
        source_file: impl Fn(&str) -> Option<&'source SourceFile>,
    ) -> Option<DataId> {
        match self.kind {
            DeclarationKind::Record => contract
                .records
                .iter()
                .find(|record| {
                    self.matches_contract_declaration(
                        record.id.as_str(),
                        record.name.spelling(),
                        record.source_span.as_ref(),
                        source_file(record.id.as_str()),
                    )
                })
                .map(|record| DataId::Record(record.id.clone())),
            DeclarationKind::Enumeration => contract
                .enums
                .iter()
                .find(|enumeration| {
                    self.matches_contract_declaration(
                        enumeration.id.as_str(),
                        enumeration.name.spelling(),
                        enumeration.source_span.as_ref(),
                        source_file(enumeration.id.as_str()),
                    )
                })
                .map(|enumeration| DataId::Enumeration(enumeration.id.clone())),
        }
    }

    fn matches_contract_declaration(
        &self,
        id: &str,
        name: &str,
        span: Option<&SourceSpan>,
        source_file: Option<&SourceFile>,
    ) -> bool {
        if name != self.name {
            return false;
        }
        self.local_scope.is_some()
            || self.matches_module(id)
                && (span.is_some_and(|span| self.matches_source(span))
                    || source_file.is_some_and(|source_file| self.matches_source_file(source_file)))
    }

    fn matches_source(&self, span: &SourceSpan) -> bool {
        self.matches_source_file(&span.file)
    }

    fn matches_source_file(&self, source_file: &SourceFile) -> bool {
        Self::canonical(Path::new(source_file.as_str())) == Self::canonical(&self.source)
    }

    fn matches_module(&self, id: &str) -> bool {
        let mut candidate = id.rsplit("::");
        self.module_path
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(self.name.as_str()))
            .rev()
            .all(|segment| candidate.next() == Some(segment))
    }

    fn canonical(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}

impl<'target> ScopeFinder<'target> {
    fn find(
        syntax: &syn::File,
        name: &'target str,
        kind: DeclarationKind,
        target_ordinal: usize,
    ) -> Option<Scope> {
        let mut finder = Self {
            name,
            kind,
            target_ordinal,
            observed: 0,
            current: Scope::Module(Vec::new()),
            scope: None,
        };
        finder.visit_file(syntax);
        finder.scope
    }

    fn observe(&mut self, kind: DeclarationKind, name: &syn::Ident) {
        if self.scope.is_some() || self.kind != kind || name != self.name {
            return;
        }
        let ordinal = self.observed;
        self.observed += 1;
        if ordinal == self.target_ordinal {
            self.scope = Some(self.current.clone());
        }
    }
}

impl<'syntax> Visit<'syntax> for ScopeFinder<'_> {
    fn visit_file(&mut self, syntax: &'syntax syn::File) {
        self.current = Scope::Module(Vec::new());
        syntax.items.iter().for_each(|item| self.visit_item(item));
    }

    fn visit_item_mod(&mut self, module: &'syntax syn::ItemMod) {
        let Some((_, items)) = &module.content else {
            return;
        };
        let nested = match &self.current {
            Scope::Module(path) => Scope::Module(
                path.iter()
                    .cloned()
                    .chain(std::iter::once(module.ident.to_string()))
                    .collect(),
            ),
            Scope::Block(items) => Scope::Block(items.clone()),
        };
        let enclosing = std::mem::replace(&mut self.current, nested);
        items.iter().for_each(|item| self.visit_item(item));
        self.current = enclosing;
    }

    fn visit_block(&mut self, block: &'syntax syn::Block) {
        if self.scope.is_some() {
            return;
        }
        let items = block
            .stmts
            .iter()
            .filter_map(|statement| match statement {
                syn::Stmt::Item(item) => Some(item.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let enclosing = std::mem::replace(&mut self.current, Scope::Block(items));
        block
            .stmts
            .iter()
            .for_each(|statement| self.visit_stmt(statement));
        self.current = enclosing;
    }

    fn visit_item_struct(&mut self, item: &'syntax syn::ItemStruct) {
        self.observe(DeclarationKind::Record, &item.ident);
    }

    fn visit_item_enum(&mut self, item: &'syntax syn::ItemEnum) {
        self.observe(DeclarationKind::Enumeration, &item.ident);
    }
}

#[cfg(test)]
mod tests {
    use super::{DeclarationKind, FileIndex, Scope, ScopeFinder};
    use proc_macro2::LineColumn;

    /// `source_offset` feeds `declaration_ordinal`, and the two agree only if
    /// the offset lands inside the identifier.
    ///
    /// The compiler counts both fields from one, and counts the column in
    /// characters. Two separate things can put the offset outside the name:
    /// reading the column as 0-indexed moves it one byte forward, and adding a
    /// character column to a byte offset moves it back by one for every extra
    /// byte earlier in the line.
    ///
    /// Either slip stays hidden behind a name of two characters or more, which
    /// every declaration in this repository has. The one-character rows are
    /// what make the arithmetic observable.
    #[test]
    fn locates_a_declaration_from_a_one_indexed_character_column() {
        for (source, name, line, column) in [
            ("#[data]\npub struct S { pub a: u32 }\n", "S", 2, 12),
            ("#[data]\npub struct Point { pub a: u32 }\n", "Point", 2, 12),
            ("    #[data]\n    pub struct T;\n", "T", 2, 16),
            ("#[data]\npub enum E { A }\n", "E", 2, 10),
            // One multi-byte character before the name, then two: the column
            // and the byte offset drift apart by one byte each.
            ("#[data]\npub /* \u{3b1} */ struct S;\n", "S", 2, 20),
            ("#[data]\npub /* \u{3b1}\u{3b2} */ struct S;\n", "S", 2, 21),
            (
                "#[data]\npub /* \u{1f600} */ struct Point;\n",
                "Point",
                2,
                20,
            ),
        ] {
            let kind = match source.contains("enum") {
                true => DeclarationKind::Enumeration,
                false => DeclarationKind::Record,
            };
            let index = FileIndex::new(source.to_owned()).expect("fixture parses");
            let offset = index
                .offset_of(LineColumn { line, column })
                .unwrap_or_else(|| panic!("`{name}` has an offset"));
            assert_eq!(
                &source[offset..offset + name.len()],
                name,
                "offset for `{name}` should land on the name",
            );
            assert_eq!(
                index.ordinal_of(name, kind, offset),
                Some(0),
                "`{name}` should resolve to its own declaration",
            );
        }
    }

    #[test]
    fn finds_data_declarations_in_their_function_block() {
        let syntax = syn::parse_file(
            "fn roundtrip() {\n#[data]\nstruct Point { x: f64 }\n#[data]\nstruct Pair { point: Point }\n}\n",
        )
        .expect("source parses");
        let scope =
            ScopeFinder::find(&syntax, "Pair", DeclarationKind::Record, 0).expect("scope exists");

        assert!(matches!(scope, Scope::Block(items) if items.len() == 2));
    }

    #[test]
    fn distinguishes_module_data_from_local_data() {
        let syntax = syn::parse_file("#[data]\nstruct Point { x: f64 }\n").expect("source parses");
        let scope =
            ScopeFinder::find(&syntax, "Point", DeclarationKind::Record, 0).expect("scope exists");

        assert!(matches!(scope, Scope::Module(path) if path.is_empty()));
    }

    #[test]
    fn distinguishes_same_named_declarations_by_source_order() {
        let syntax = syn::parse_file(
            "fn first() { struct Point; }\nfn second() { struct Point; struct Pair(Point); }\n",
        )
        .expect("source parses");
        let scope =
            ScopeFinder::find(&syntax, "Point", DeclarationKind::Record, 1).expect("scope exists");

        assert!(matches!(scope, Scope::Block(items) if items.len() == 2));
    }

    #[test]
    fn distinguishes_same_named_declarations_by_inline_module() {
        let syntax = syn::parse_file("mod first { struct Point; }\nmod second { struct Point; }\n")
            .expect("source parses");
        let scope =
            ScopeFinder::find(&syntax, "Point", DeclarationKind::Record, 1).expect("scope exists");

        assert!(matches!(scope, Scope::Module(path) if path == ["second"]));
    }
}
