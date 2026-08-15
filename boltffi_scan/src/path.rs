use std::collections::HashMap;

use boltffi_ast::{SourceFile, SourceSpan};
use proc_macro2::{LineColumn, Span};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModulePath {
    segments: Vec<String>,
}

/// The module a package is reachable under.
///
/// Segments of a path are Rust identifiers, and a package name is not one:
/// cargo allows a hyphen where the module tree has an underscore. Ids are
/// compared against this spelling in several places, so it has one definition.
pub(crate) fn module_name(package: &str) -> String {
    package.replace('-', "_")
}

impl ModulePath {
    /// Root of a crate's module path.
    ///
    /// Dependencies arrive already normalised, through
    /// `ExportedPackage::module_name`, so only the root reaches this with a
    /// raw package name.
    pub(super) fn root(crate_name: impl Into<String>) -> Self {
        Self {
            segments: vec![module_name(&crate_name.into())],
        }
    }

    pub(super) fn child(&self, module: impl Into<String>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(module.into());
        Self { segments }
    }

    pub(super) fn qualified(&self, ident: &str) -> String {
        let mut path = self.segments.join("::");
        path.push_str("::");
        path.push_str(ident);
        path
    }

    pub(super) fn segments(&self) -> &[String] {
        &self.segments
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModuleScope {
    path: ModulePath,
    imports: Imports,
    spans: Option<SpanMap>,
}

pub(super) enum ImportLookup<'a> {
    None,
    Unique(&'a [String]),
    Ambiguous,
}

pub(super) enum PathExpansion {
    Relative(String),
    Imported { local: String, path: String },
    Qualified(String),
    Ambiguous,
    Unsupported,
}

impl PathExpansion {
    #[cfg(test)]
    pub(super) fn candidate(&self) -> Option<&str> {
        match self {
            Self::Relative(path) | Self::Imported { path, .. } | Self::Qualified(path) => {
                Some(path)
            }
            Self::Ambiguous | Self::Unsupported => None,
        }
    }
}

impl ModuleScope {
    #[cfg(test)]
    pub(super) fn new(path: ModulePath, items: &[syn::Item]) -> Self {
        Self::with_spans(path, items, None)
    }

    pub(super) fn with_spans(
        path: ModulePath,
        items: &[syn::Item],
        spans: Option<SpanMap>,
    ) -> Self {
        Self {
            imports: Imports::scan(&path, items),
            path,
            spans,
        }
    }

    #[cfg(test)]
    pub(super) fn root(crate_name: impl Into<String>) -> Self {
        Self::new(ModulePath::root(crate_name), &[])
    }

    pub(super) fn path(&self) -> &ModulePath {
        &self.path
    }

    pub(super) fn source_span(&self, span: Span) -> Option<SourceSpan> {
        self.spans.as_ref()?.source_span(span)
    }

    pub(super) fn source_file(&self) -> Option<&SourceFile> {
        self.spans.as_ref().map(SpanMap::source_file)
    }

    pub(super) fn expand(&self, path: &syn::Path) -> PathExpansion {
        if path.leading_colon.is_some() {
            return PathExpansion::Unsupported;
        }
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        self.expand_segments(&segments)
    }

    pub(super) fn imported(&self, name: &str) -> ImportLookup<'_> {
        self.imports.get(name)
    }

    pub(super) fn has_glob_imports(&self) -> bool {
        !self.imports.globs().is_empty()
    }

    pub(super) fn glob_candidates_for_segments(&self, segments: &[String]) -> Vec<String> {
        self.imports
            .globs()
            .iter()
            .map(|glob| {
                glob.iter()
                    .cloned()
                    .chain(segments.iter().cloned())
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect()
    }

    pub fn reexported(&self, name: &str) -> ImportLookup<'_> {
        self.imports.reexported(name)
    }

    pub fn reexport_glob_candidates_for_segments(&self, segments: &[String]) -> Vec<String> {
        self.imports
            .reexport_globs()
            .iter()
            .map(|glob| {
                glob.iter()
                    .cloned()
                    .chain(segments.iter().cloned())
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect()
    }

    fn expand_segments(&self, segments: &[String]) -> PathExpansion {
        let Some((first, rest)) = segments.split_first() else {
            return PathExpansion::Unsupported;
        };
        match first.as_str() {
            "crate" => {
                let mut resolved = self
                    .path
                    .segments
                    .first()
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>();
                resolved.extend(rest.iter().cloned());
                PathExpansion::Qualified(resolved.join("::"))
            }
            "self" => {
                let mut resolved = self.path.segments.clone();
                resolved.extend(rest.iter().cloned());
                PathExpansion::Qualified(resolved.join("::"))
            }
            "super" => {
                let super_count = segments
                    .iter()
                    .take_while(|segment| segment.as_str() == "super")
                    .count();
                let retained_segments = self.path.segments.len().saturating_sub(super_count).max(1);
                let mut resolved = self
                    .path
                    .segments
                    .iter()
                    .take(retained_segments)
                    .cloned()
                    .collect::<Vec<_>>();
                resolved.extend(segments.iter().skip(super_count).cloned());
                PathExpansion::Qualified(resolved.join("::"))
            }
            _ => match self.imports.get(first) {
                ImportLookup::Unique(imported) => {
                    let mut resolved = imported.to_vec();
                    resolved.extend(rest.iter().cloned());
                    PathExpansion::Imported {
                        local: first.clone(),
                        path: resolved.join("::"),
                    }
                }
                ImportLookup::Ambiguous => PathExpansion::Ambiguous,
                ImportLookup::None => {
                    let mut resolved = self.path.segments.clone();
                    resolved.extend(segments.iter().cloned());
                    PathExpansion::Relative(resolved.join("::"))
                }
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SpanMap {
    file: SourceFile,
    line_starts: Vec<usize>,
}

impl SpanMap {
    pub(super) fn new(file: impl Into<String>, source: &str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(
                source
                    .bytes()
                    .enumerate()
                    .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
            )
            .collect();
        Self {
            file: SourceFile::new(file),
            line_starts,
        }
    }

    fn source_span(&self, span: Span) -> Option<SourceSpan> {
        let start = self.offset(span.start())?;
        let end = self.offset(span.end())?;
        (end >= start).then(|| SourceSpan::new(self.file.clone(), start, end))
    }

    fn source_file(&self) -> &SourceFile {
        &self.file
    }

    fn offset(&self, location: LineColumn) -> Option<usize> {
        let line_start = self.line_starts.get(location.line.checked_sub(1)?)?;
        Some(line_start + location.column)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Imports {
    by_name: HashMap<String, ImportBinding>,
    globs: Vec<Vec<String>>,
    reexports_by_name: HashMap<String, ImportBinding>,
    reexport_globs: Vec<Vec<String>>,
}

impl Imports {
    fn scan(module: &ModulePath, items: &[syn::Item]) -> Self {
        let mut imports = Self::default();
        items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Use(item) => Some(item),
                _ => None,
            })
            .for_each(|item| {
                let prefix = UsePrefix::new(item.leading_colon.is_some());
                let public = matches!(item.vis, syn::Visibility::Public(_));
                imports.insert_tree(module, prefix, &item.tree, public);
            });
        imports
    }

    fn get(&self, name: &str) -> ImportLookup<'_> {
        match self.by_name.get(name) {
            Some(ImportBinding::Unique(path)) => ImportLookup::Unique(path),
            Some(ImportBinding::Ambiguous) => ImportLookup::Ambiguous,
            None => ImportLookup::None,
        }
    }

    fn globs(&self) -> &[Vec<String>] {
        &self.globs
    }

    fn reexported(&self, name: &str) -> ImportLookup<'_> {
        match self.reexports_by_name.get(name) {
            Some(ImportBinding::Unique(path)) => ImportLookup::Unique(path),
            Some(ImportBinding::Ambiguous) => ImportLookup::Ambiguous,
            None => ImportLookup::None,
        }
    }

    fn reexport_globs(&self) -> &[Vec<String>] {
        &self.reexport_globs
    }

    fn insert_tree(
        &mut self,
        module: &ModulePath,
        prefix: UsePrefix,
        tree: &syn::UseTree,
        public: bool,
    ) {
        match tree {
            syn::UseTree::Path(path) => {
                let next = prefix.join(path.ident.to_string());
                self.insert_tree(module, next, &path.tree, public);
            }
            syn::UseTree::Name(name) => {
                if name.ident == "self" {
                    self.insert_self(module, prefix, public);
                } else {
                    let imported = prefix.join(name.ident.to_string());
                    self.insert(module, name.ident.to_string(), imported, public);
                }
            }
            syn::UseTree::Rename(rename) => {
                let imported = if rename.ident == "self" {
                    prefix
                } else {
                    prefix.join(rename.ident.to_string())
                };
                self.insert(module, rename.rename.to_string(), imported, public);
            }
            syn::UseTree::Group(group) => group
                .items
                .iter()
                .for_each(|tree| self.insert_tree(module, prefix.clone(), tree, public)),
            syn::UseTree::Glob(_) => self.insert_glob(module, prefix, public),
        }
    }

    fn insert_self(&mut self, module: &ModulePath, prefix: UsePrefix, public: bool) {
        if let Some(local) = prefix.segments.last().cloned() {
            self.insert(module, local, prefix, public);
        }
    }

    fn insert(&mut self, module: &ModulePath, local: String, prefix: UsePrefix, public: bool) {
        let path = prefix.into_qualified_segments(module).collect::<Vec<_>>();
        Self::insert_binding(&mut self.by_name, local.clone(), path.clone());
        if public {
            Self::insert_binding(&mut self.reexports_by_name, local, path);
        }
    }

    fn insert_binding(
        bindings: &mut HashMap<String, ImportBinding>,
        local: String,
        path: Vec<String>,
    ) {
        match bindings.get(&local) {
            Some(ImportBinding::Unique(existing)) if existing == &path => {}
            Some(ImportBinding::Unique(_)) | Some(ImportBinding::Ambiguous) => {
                bindings.insert(local, ImportBinding::Ambiguous);
            }
            None => {
                bindings.insert(local, ImportBinding::Unique(path));
            }
        }
    }

    fn insert_glob(&mut self, module: &ModulePath, prefix: UsePrefix, public: bool) {
        let path = prefix.into_qualified_segments(module).collect::<Vec<_>>();
        if !self.globs.contains(&path) {
            self.globs.push(path.clone());
        }
        if public && !self.reexport_globs.contains(&path) {
            self.reexport_globs.push(path);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ImportBinding {
    Unique(Vec<String>),
    Ambiguous,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UsePrefix {
    leading_colon: bool,
    segments: Vec<String>,
}

impl UsePrefix {
    fn new(leading_colon: bool) -> Self {
        Self {
            leading_colon,
            segments: Vec::new(),
        }
    }

    fn join(mut self, segment: String) -> Self {
        self.segments.push(segment);
        self
    }

    fn into_qualified_segments(self, module: &ModulePath) -> impl Iterator<Item = String> {
        let segments = self.segments;
        let root = segments
            .first()
            .map(String::as_str)
            .filter(|_| !self.leading_colon);
        match root {
            Some("crate") => module
                .segments
                .first()
                .cloned()
                .into_iter()
                .chain(segments.into_iter().skip(1))
                .collect::<Vec<_>>()
                .into_iter(),
            Some("self") => module
                .segments
                .iter()
                .cloned()
                .chain(segments.into_iter().skip(1))
                .collect::<Vec<_>>()
                .into_iter(),
            Some("super") => {
                let super_count = segments
                    .iter()
                    .take_while(|segment| segment.as_str() == "super")
                    .count();
                let retained_segments = module.segments.len().saturating_sub(super_count).max(1);
                module
                    .segments
                    .iter()
                    .take(retained_segments)
                    .cloned()
                    .chain(segments.into_iter().skip(super_count))
                    .collect::<Vec<_>>()
                    .into_iter()
            }
            _ => segments.into_iter().collect::<Vec<_>>().into_iter(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_qualifies_items_under_the_crate_segment() {
        assert_eq!(ModulePath::root("demo").qualified("add"), "demo::add");
    }

    #[test]
    fn child_paths_preserve_all_ancestors_in_order() {
        let path = ModulePath::root("demo").child("geometry").child("point");

        assert_eq!(path.qualified("Point"), "demo::geometry::point::Point");
    }

    #[test]
    fn child_does_not_mutate_the_parent_path() {
        let parent = ModulePath::root("demo");
        let child = parent.child("geometry");

        assert_eq!(parent.qualified("Point"), "demo::Point");
        assert_eq!(child.qualified("Point"), "demo::geometry::Point");
    }

    #[test]
    fn expands_type_paths_from_module_context() {
        let module = ModulePath::root("demo").child("geometry").child("shape");
        let scope = ModuleScope::new(module, &[]);

        assert_eq!(
            scope
                .expand(&syn::parse_str("Point").expect("path"))
                .candidate(),
            Some("demo::geometry::shape::Point")
        );
        assert_eq!(
            scope
                .expand(&syn::parse_str("self::Point").expect("path"))
                .candidate(),
            Some("demo::geometry::shape::Point")
        );
        assert_eq!(
            scope
                .expand(&syn::parse_str("super::Point").expect("path"))
                .candidate(),
            Some("demo::geometry::Point")
        );
        assert_eq!(
            scope
                .expand(&syn::parse_str("crate::Point").expect("path"))
                .candidate(),
            Some("demo::Point")
        );
    }

    #[test]
    fn expands_explicit_import_aliases() {
        let file = syn::parse_str::<syn::File>(
            "use chrono::{DateTime as Date, Utc}; \
             use crate::domain::Money as Cash; \
             use super::shared::Id;",
        )
        .expect("source");
        let scope = ModuleScope::new(
            ModulePath::root("demo").child("api").child("v1"),
            &file.items,
        );

        assert_eq!(
            scope
                .expand(&syn::parse_str("Date").expect("path"))
                .candidate(),
            Some("chrono::DateTime")
        );
        assert_eq!(
            scope
                .expand(&syn::parse_str("Utc").expect("path"))
                .candidate(),
            Some("chrono::Utc")
        );
        assert_eq!(
            scope
                .expand(&syn::parse_str("Cash").expect("path"))
                .candidate(),
            Some("demo::domain::Money")
        );
        assert_eq!(
            scope
                .expand(&syn::parse_str("Id").expect("path"))
                .candidate(),
            Some("demo::api::shared::Id")
        );
    }
}
