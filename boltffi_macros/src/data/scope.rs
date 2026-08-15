use std::fs;
use std::path::{Path, PathBuf};

use boltffi_ast::{EnumId, RecordId, SourceContract, SourceFile, SourceSpan};
use proc_macro2::LineColumn;
use syn::visit::Visit;

#[derive(Clone, Copy, Eq, PartialEq)]
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
        let source_text = fs::read_to_string(&source).map_err(|error| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("read data source `{}`: {error}", source.display()),
            )
        })?;
        let invocation_offset = Self::source_offset(&source_text, location).ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "locate data declaration `{name}` at {}:{} in `{}`",
                    location.line,
                    location.column,
                    source.display()
                ),
            )
        })?;
        let target_ordinal =
            Self::declaration_ordinal(&source_text, &name, kind, invocation_offset).ok_or_else(
                || {
                    syn::Error::new(
                        proc_macro2::Span::call_site(),
                        format!(
                            "locate data declaration `{name}` at {}:{} in `{}`",
                            location.line,
                            location.column,
                            source.display()
                        ),
                    )
                },
            )?;
        let syntax = syn::parse_file(&source_text)?;
        let scope = ScopeFinder::find(&syntax, &name, kind, target_ordinal).ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "locate data declaration `{name}` at {}:{} in `{}`",
                    location.line,
                    location.column,
                    source.display()
                ),
            )
        })?;
        let (module_path, local_scope) = match scope {
            Scope::Module(module_path) => (module_path, None),
            Scope::Block(items) => (
                Vec::new(),
                Some(syn::File {
                    shebang: None,
                    attrs: Vec::new(),
                    items,
                }),
            ),
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

    /// Byte offset of a `proc_macro::Span` location.
    ///
    /// The compiler counts both fields from one, and counts the column in
    /// characters while everything downstream works in bytes. Adding the
    /// column to a byte offset is only correct for a line that is entirely
    /// ASCII up to the declaration; `pub /* \u{3b1} */ struct S` is off by the
    /// extra byte, and the identifier is missed.
    fn source_offset(source: &str, location: LineColumn) -> Option<usize> {
        let line_start = std::iter::once(0)
            .chain(
                source
                    .bytes()
                    .enumerate()
                    .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
            )
            .nth(location.line.checked_sub(1)?)?;
        let line = source[line_start..]
            .split_once('\n')
            .map_or(&source[line_start..], |(line, _)| line);
        let column = line
            .char_indices()
            .nth(location.column.checked_sub(1)?)
            .map(|(column, _)| column)?;
        Some(line_start + column)
    }

    fn declaration_ordinal(
        source: &str,
        name: &str,
        kind: DeclarationKind,
        target_offset: usize,
    ) -> Option<usize> {
        let keyword = match kind {
            DeclarationKind::Record => "struct",
            DeclarationKind::Enumeration => "enum",
        };
        let expected_name = name.strip_prefix("r#").unwrap_or(name);
        let mut expects_name = false;
        let mut ordinal = 0;

        rustc_lexer::tokenize(source)
            .scan(0, |offset, token| {
                let start = *offset;
                *offset += token.len;
                Some((token.kind, start, &source[start..*offset]))
            })
            .find_map(|(token, offset, spelling)| match token {
                rustc_lexer::TokenKind::Whitespace
                | rustc_lexer::TokenKind::LineComment
                | rustc_lexer::TokenKind::BlockComment { .. } => None,
                rustc_lexer::TokenKind::Ident if spelling == keyword => {
                    expects_name = true;
                    None
                }
                rustc_lexer::TokenKind::Ident | rustc_lexer::TokenKind::RawIdent
                    if expects_name =>
                {
                    expects_name = false;
                    let declaration_name = spelling.strip_prefix("r#").unwrap_or(spelling);
                    if declaration_name != expected_name {
                        return None;
                    }
                    let current = ordinal;
                    ordinal += 1;
                    (offset <= target_offset && target_offset < offset + spelling.len())
                        .then_some(current)
                }
                _ => {
                    expects_name = false;
                    None
                }
            })
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
    use super::{Declaration, DeclarationKind, Scope, ScopeFinder};
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
            let offset = Declaration::source_offset(source, LineColumn { line, column })
                .unwrap_or_else(|| panic!("`{name}` has an offset"));
            assert_eq!(
                &source[offset..offset + name.len()],
                name,
                "offset for `{name}` should land on the name",
            );
            assert_eq!(
                Declaration::declaration_ordinal(source, name, kind, offset),
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
