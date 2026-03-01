use proc_macro2::Span;
use std::{
    cell::RefCell,
    env, fs,
    path::{Path, PathBuf},
};
use syn::{Item, ItemMod, ItemTrait};

#[derive(Clone)]
pub struct CallbackTraitRegistry {
    entries: Vec<CallbackTraitEntry>,
}

#[derive(Clone)]
pub struct CallbackTraitResolution {
    pub foreign_path: syn::Path,
    pub is_object_safe: bool,
}

#[derive(Clone)]
struct CallbackTraitEntry {
    module_path: Vec<String>,
    trait_name: String,
    is_object_safe: bool,
}

thread_local! {
    static REGISTRY_CACHE: RefCell<Option<CallbackTraitRegistry>> = const { RefCell::new(None) };
}

pub fn registry_for_current_crate() -> syn::Result<CallbackTraitRegistry> {
    REGISTRY_CACHE.with(|cell| {
        if let Some(registry) = cell.borrow().clone() {
            return Ok(registry);
        }

        let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|_| syn::Error::new(Span::call_site(), "CARGO_MANIFEST_DIR not set"))?;
        let registry = build_registry(Path::new(&manifest_dir))?;
        *cell.borrow_mut() = Some(registry.clone());
        Ok(registry)
    })
}

impl CallbackTraitRegistry {
    pub fn resolve(&self, trait_path: &syn::Path) -> Option<CallbackTraitResolution> {
        let segments = normalize_segments(trait_path);
        let (trait_name, module_path) = segments.split_last()?;
        let matches = self
            .entries
            .iter()
            .filter(|entry| entry.trait_name == *trait_name)
            .filter(|entry| module_path.is_empty() || entry.module_path == module_path)
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [entry] => Some(CallbackTraitResolution {
                foreign_path: entry.foreign_path(),
                is_object_safe: entry.is_object_safe,
            }),
            _ => None,
        }
    }
}

fn build_registry(manifest_dir: &Path) -> syn::Result<CallbackTraitRegistry> {
    let src_root = manifest_dir.join("src");
    let files = list_rs_files(&src_root)?;
    let mut entries = Vec::new();

    files.iter().try_for_each(|file_path| {
        let module_path = module_path_for_rs_file(&src_root, file_path)?;
        let content = fs::read_to_string(file_path).map_err(|e| syn::Error::new(Span::call_site(), format!("read {}: {}", file_path.display(), e)))?;
        let syntax = syn::parse_file(&content)?;
        let mut collector = CallbackTraitCollector {
            module_path,
            entries: &mut entries,
        };
        syntax.items.iter().try_for_each(|item| collector.collect_item(item))
    })?;

    Ok(CallbackTraitRegistry { entries })
}

struct CallbackTraitCollector<'a> {
    module_path: Vec<String>,
    entries: &'a mut Vec<CallbackTraitEntry>,
}

impl<'a> CallbackTraitCollector<'a> {
    fn collect_item(&mut self, item: &Item) -> syn::Result<()> {
        match item {
            Item::Trait(item_trait) => self.collect_trait(item_trait),
            Item::Mod(item_mod) => self.collect_mod(item_mod),
            _ => Ok(()),
        }
    }

    fn collect_trait(&mut self, item_trait: &ItemTrait) -> syn::Result<()> {
        if !is_callback_trait(item_trait) {
            return Ok(());
        }

        let entry = CallbackTraitEntry {
            module_path: self.module_path.clone(),
            trait_name: item_trait.ident.to_string(),
            is_object_safe: is_object_safe(item_trait),
        };
        self.entries.push(entry);
        Ok(())
    }

    fn collect_mod(&mut self, item_mod: &ItemMod) -> syn::Result<()> {
        let Some((_, items)) = &item_mod.content else {
            return Ok(());
        };
        let mut next_path = self.module_path.clone();
        next_path.push(item_mod.ident.to_string());
        let mut nested = CallbackTraitCollector {
            module_path: next_path,
            entries: self.entries,
        };
        items.iter().try_for_each(|item| nested.collect_item(item))
    }
}

fn is_callback_trait(item_trait: &ItemTrait) -> bool {
    item_trait
        .attrs
        .iter()
        .any(|attr| attr.path().segments.last().is_some_and(|segment| segment.ident == "export" || segment.ident == "ffi_trait"))
}

fn is_object_safe(item_trait: &ItemTrait) -> bool {
    let has_async_methods = item_trait.items.iter().any(|item| {
        matches!(
            item,
            syn::TraitItem::Fn(method) if method.sig.asyncness.is_some()
        )
    });
    let has_async_trait_attr = item_trait
        .attrs
        .iter()
        .any(|attr| attr.path().segments.last().is_some_and(|segment| segment.ident == "async_trait"));
    !has_async_methods || has_async_trait_attr
}

fn normalize_segments(path: &syn::Path) -> Vec<String> {
    let mut segments = path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>();
    if let Some(first) = segments.first()
        && matches!(first.as_str(), "crate" | "self" | "super")
    {
        segments.remove(0);
    }
    segments
}

fn list_rs_files(src_root: &Path) -> syn::Result<Vec<PathBuf>> {
    let mut out = Vec::<PathBuf>::new();
    collect_rs_files(src_root, &mut out)?;
    Ok(out)
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> syn::Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| syn::Error::new(Span::call_site(), format!("read_dir {}: {}", dir.display(), e)))?;

    entries.filter_map(|entry| entry.ok()).map(|entry| entry.path()).try_for_each(|path| {
        if path.is_dir() {
            return collect_rs_files(&path, out);
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
        Ok(())
    })
}

fn module_path_for_rs_file(src_root: &Path, file_path: &Path) -> syn::Result<Vec<String>> {
    let relative = file_path.strip_prefix(src_root).map_err(|_| syn::Error::new(Span::call_site(), "path not under src"))?;
    let mut parts = relative.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>();

    let file_name = parts.pop().unwrap_or_default();
    let mut module_parts = parts;

    match file_name.as_str() {
        "lib.rs" => {}
        "mod.rs" => {}
        _ if file_name.ends_with(".rs") => {
            let base = file_name.trim_end_matches(".rs");
            module_parts.push(base.to_string());
        }
        _ => {}
    }

    Ok(module_parts.into_iter().filter(|p| !p.is_empty()).collect())
}

impl CallbackTraitEntry {
    fn foreign_path(&self) -> syn::Path {
        let mut segments = Vec::with_capacity(self.module_path.len() + 2);
        segments.push("crate".to_string());
        segments.extend(self.module_path.iter().cloned());
        segments.push(format!("Foreign{}", self.trait_name));
        syn::parse_str(&segments.join("::")).unwrap_or_else(|_| syn::Path {
            leading_colon: None,
            segments: syn::punctuated::Punctuated::new(),
        })
    }
}
