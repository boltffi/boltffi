use std::path::{Path, PathBuf};

use crate::path::SpanMap;
use crate::{ModulePath, ModuleScope, ScanError};

#[derive(Clone)]
pub struct SourceTree {
    modules: Vec<SourceModule>,
}

impl SourceTree {
    pub fn load(root: &Path, crate_name: &str) -> Result<Self, ScanError> {
        walk(
            ModulePath::root(crate_name),
            &module_dir(root),
            parse(root)?,
            SourceMode::Files,
        )
        .map(|modules| Self { modules })
    }

    pub fn inline(crate_name: &str, file: syn::File) -> Result<Self, ScanError> {
        walk(
            ModulePath::root(crate_name),
            Path::new("."),
            ParsedFile::inline(file.items),
            SourceMode::Inline,
        )
        .map(|modules| Self { modules })
    }

    pub fn combine(trees: impl IntoIterator<Item = Self>) -> Self {
        Self {
            modules: trees.into_iter().flat_map(|tree| tree.modules).collect(),
        }
    }

    #[cfg(test)]
    pub fn in_memory(crate_name: &str, items: Vec<syn::Item>) -> Result<Self, ScanError> {
        walk(
            ModulePath::root(crate_name),
            Path::new("."),
            ParsedFile::inline(items),
            SourceMode::Inline,
        )
        .map(|modules| Self { modules })
    }

    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }
}

#[derive(Clone)]
pub struct SourceModule {
    scope: ModuleScope,
    items: Vec<syn::Item>,
}

impl SourceModule {
    fn new(path: ModulePath, items: Vec<syn::Item>, spans: Option<SpanMap>) -> Self {
        Self {
            scope: ModuleScope::with_spans(path, &items, spans),
            items,
        }
    }

    pub fn scope(&self) -> &ModuleScope {
        &self.scope
    }

    pub fn items(&self) -> &[syn::Item] {
        &self.items
    }
}

fn walk(
    module: ModulePath,
    dir: &Path,
    file: ParsedFile,
    source_mode: SourceMode,
) -> Result<Vec<SourceModule>, ScanError> {
    let spans = file.spans;
    let (own_items, mut child_modules) = file.items.into_iter().try_fold(
        (Vec::new(), Vec::new()),
        |(mut own_items, mut child_modules), item| {
            match item {
                syn::Item::Mod(item_mod) => {
                    child_modules.extend(descend(
                        &module,
                        dir,
                        item_mod.clone(),
                        spans.clone(),
                        source_mode,
                    )?);
                    own_items.push(syn::Item::Mod(item_mod));
                }
                item => own_items.push(item),
            }
            Ok::<_, ScanError>((own_items, child_modules))
        },
    )?;
    child_modules.push(SourceModule::new(module, own_items, spans));
    Ok(child_modules)
}

fn descend(
    parent: &ModulePath,
    dir: &Path,
    item_mod: syn::ItemMod,
    parent_spans: Option<SpanMap>,
    source_mode: SourceMode,
) -> Result<Vec<SourceModule>, ScanError> {
    if has_cfg(&item_mod.attrs) {
        return Ok(Vec::new());
    }
    let name = item_mod.ident.to_string();
    let lookup_name = module_file_name(&name);
    let child = parent.child(&name);
    match item_mod.content {
        Some((_, items)) => walk(
            child,
            &dir.join(lookup_name),
            ParsedFile {
                items,
                spans: parent_spans,
            },
            source_mode,
        ),
        None if source_mode == SourceMode::Files => {
            let path = resolve(parent, dir, &name, lookup_name, &item_mod.attrs)?;
            let file = parse(&path)?;
            walk(child, &module_dir(&path), file, source_mode)
        }
        None => Err(ScanError::ModuleNotFound {
            module: parent.qualified(&name),
            searched: Vec::new(),
        }),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SourceMode {
    Files,
    Inline,
}

fn module_dir(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs" | "main.rs" | "mod.rs") => parent.to_path_buf(),
        _ => match file.file_stem().and_then(|stem| stem.to_str()) {
            Some(stem) => parent.join(stem),
            None => parent.to_path_buf(),
        },
    }
}

fn resolve(
    parent: &ModulePath,
    dir: &Path,
    diagnostic_name: &str,
    lookup_name: &str,
    attrs: &[syn::Attribute],
) -> Result<PathBuf, ScanError> {
    if let Some(path) = path_attr(attrs) {
        let candidate = dir.join(path);
        return candidate
            .is_file()
            .then_some(candidate.clone())
            .ok_or_else(|| ScanError::ModuleNotFound {
                module: parent.qualified(diagnostic_name),
                searched: vec![candidate.display().to_string()],
            });
    }
    let flat = dir.join(format!("{lookup_name}.rs"));
    let nested = dir.join(lookup_name).join("mod.rs");
    if flat.is_file() {
        Ok(flat)
    } else if nested.is_file() {
        Ok(nested)
    } else {
        Err(ScanError::ModuleNotFound {
            module: parent.qualified(diagnostic_name),
            searched: vec![flat.display().to_string(), nested.display().to_string()],
        })
    }
}

fn module_file_name(identifier: &str) -> &str {
    identifier.strip_prefix("r#").unwrap_or(identifier)
}

fn has_cfg(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "cfg")
    })
}

fn path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(value) = &attr.meta else {
            return None;
        };
        let syn::Expr::Lit(expr) = &value.value else {
            return None;
        };
        let syn::Lit::Str(path) = &expr.lit else {
            return None;
        };
        Some(path.value())
    })
}

struct ParsedFile {
    items: Vec<syn::Item>,
    spans: Option<SpanMap>,
}

impl ParsedFile {
    fn inline(items: Vec<syn::Item>) -> Self {
        Self { items, spans: None }
    }
}

fn parse(path: &Path) -> Result<ParsedFile, ScanError> {
    let source = std::fs::read_to_string(path).map_err(|error| ScanError::read(path, &error))?;
    syn::parse_file(&source)
        .map(|file| ParsedFile {
            items: file.items,
            spans: Some(SpanMap::new(path.display().to_string(), &source)),
        })
        .map_err(|error| ScanError::parse(path, &error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_items(source: &str) -> Vec<syn::Item> {
        syn::parse_str::<syn::File>(source)
            .expect("valid source")
            .items
    }

    fn module_paths(tree: &SourceTree) -> Vec<String> {
        tree.modules()
            .iter()
            .map(|module| module.scope().path().qualified(""))
            .collect()
    }

    #[test]
    fn inline_modules_flatten_with_qualified_paths() {
        let tree = SourceTree::in_memory(
            "demo",
            parse_items(
                "pub struct Root; \
                 pub mod geometry { pub struct Inner; pub mod shapes { pub struct Deep; } }",
            ),
        )
        .expect("walk");

        assert_eq!(
            module_paths(&tree),
            vec![
                "demo::geometry::shapes::".to_owned(),
                "demo::geometry::".to_owned(),
                "demo::".to_owned(),
            ]
        );
    }

    #[test]
    fn mod_declarations_remain_in_their_module_items() {
        let tree = SourceTree::in_memory(
            "demo",
            parse_items("pub struct Root; pub mod inner { pub struct Inner; }"),
        )
        .expect("walk");
        let root = tree
            .modules()
            .iter()
            .find(|module| module.scope().path() == &ModulePath::root("demo"))
            .expect("root module");

        assert_eq!(root.items().len(), 2);
        assert!(matches!(root.items()[0], syn::Item::Struct(_)));
        assert!(matches!(root.items()[1], syn::Item::Mod(_)));
    }

    #[test]
    fn external_module_in_memory_is_reported_as_not_found() {
        let result = SourceTree::in_memory("demo", parse_items("pub mod missing;"));

        assert!(matches!(result, Err(ScanError::ModuleNotFound { .. })));
    }

    #[test]
    fn loads_external_modules_from_files() {
        let dir = std::env::temp_dir().join("boltffi_scan_tree_fixture");
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "pub mod geometry; pub struct Root;").expect("write root");
        std::fs::write(dir.join("geometry.rs"), "pub struct Point;").expect("write geometry");

        let tree = SourceTree::load(&root, "demo").expect("load tree");
        let paths = module_paths(&tree);

        std::fs::remove_dir_all(&dir).ok();
        assert!(paths.contains(&"demo::geometry::".to_owned()));
        assert!(paths.contains(&"demo::".to_owned()));
    }

    #[test]
    fn loads_external_modules_through_mod_rs_directories() {
        let dir = std::env::temp_dir().join("boltffi_scan_tree_modrs");
        let geometry = dir.join("geometry");
        std::fs::create_dir_all(&geometry).expect("create fixture dirs");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "pub mod geometry;").expect("write root");
        std::fs::write(geometry.join("mod.rs"), "pub struct Point;").expect("write geometry/mod");

        let tree = SourceTree::load(&root, "demo").expect("load tree");

        std::fs::remove_dir_all(&dir).ok();
        assert!(module_paths(&tree).contains(&"demo::geometry::".to_owned()));
    }

    #[test]
    fn loads_external_modules_from_path_attribute() {
        let dir = std::env::temp_dir().join("boltffi_scan_tree_path_attr");
        std::fs::create_dir_all(&dir).expect("create fixture dirs");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "#[path = \"other.rs\"] pub mod geometry;").expect("write root");
        std::fs::write(dir.join("other.rs"), "pub struct Point;").expect("write module");

        let tree = SourceTree::load(&root, "demo").expect("load tree");

        std::fs::remove_dir_all(&dir).ok();
        assert!(module_paths(&tree).contains(&"demo::geometry::".to_owned()));
    }

    #[test]
    fn raw_identifier_modules_resolve_unraw_file_names() {
        let dir = std::env::temp_dir().join("boltffi_scan_tree_raw_ident");
        std::fs::create_dir_all(&dir).expect("create fixture dirs");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "pub mod r#as;").expect("write root");
        std::fs::write(dir.join("as.rs"), "pub struct Keyword;").expect("write module");

        let tree = SourceTree::load(&root, "demo").expect("load tree");

        std::fs::remove_dir_all(&dir).ok();
        assert!(module_paths(&tree).contains(&"demo::r#as::".to_owned()));
    }

    #[test]
    fn raw_identifier_modules_resolve_unraw_mod_rs_directories() {
        let dir = std::env::temp_dir().join("boltffi_scan_tree_raw_ident_modrs");
        let module_dir = dir.join("as");
        std::fs::create_dir_all(&module_dir).expect("create fixture dirs");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "pub mod r#as;").expect("write root");
        std::fs::write(module_dir.join("mod.rs"), "pub struct Keyword;").expect("write module");

        let tree = SourceTree::load(&root, "demo").expect("load tree");

        std::fs::remove_dir_all(&dir).ok();
        assert!(module_paths(&tree).contains(&"demo::r#as::".to_owned()));
    }

    #[test]
    fn raw_identifier_path_attribute_still_uses_explicit_path() {
        let dir = std::env::temp_dir().join("boltffi_scan_tree_raw_ident_path_attr");
        std::fs::create_dir_all(&dir).expect("create fixture dirs");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "#[path = \"where.rs\"] pub mod r#where;").expect("write root");
        std::fs::write(dir.join("where.rs"), "pub struct Keyword;").expect("write module");

        let tree = SourceTree::load(&root, "demo").expect("load tree");

        std::fs::remove_dir_all(&dir).ok();
        assert!(module_paths(&tree).contains(&"demo::r#where::".to_owned()));
    }

    #[test]
    fn cfg_gated_modules_are_not_loaded_without_cfg_evaluation() {
        let tree = SourceTree::in_memory(
            "demo",
            parse_items("#[cfg(feature = \"ffi\")] pub mod gated;"),
        )
        .expect("cfg-gated module is skipped");

        assert_eq!(module_paths(&tree), vec!["demo::".to_owned()]);
    }
}
