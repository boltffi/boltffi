use boltffi_ffi_rules::naming;
use syn::{Item, ItemTrait};

use crate::index::source::IndexedSource;

#[derive(Clone, Default)]
pub struct CallbackTraitRegistry {
    entries: Vec<CallbackTraitEntry>,
}

#[derive(Clone)]
pub struct CallbackTraitResolution {
    pub foreign_path: syn::Path,
    pub is_object_safe: bool,
    pub supports_local_handle: bool,
    pub local_handle_path: syn::Path,
}

#[derive(Clone)]
struct CallbackTraitEntry {
    root_path: Vec<String>,
    module_path: Vec<String>,
    trait_name: String,
    is_object_safe: bool,
    has_async_methods: bool,
}

struct CallbackTraitDescriptor<'a> {
    item_trait: &'a ItemTrait,
}

impl CallbackTraitRegistry {
    pub fn resolve(&self, trait_path: &syn::Path) -> Option<CallbackTraitResolution> {
        let path = Self::path(trait_path);
        let (trait_name, module_path) = path.split_last()?;
        let matches = self
            .entries
            .iter()
            .filter(|entry| entry.trait_name == *trait_name)
            .filter(|entry| module_path.is_empty() || entry.resolution_path() == module_path)
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [entry] => Some(CallbackTraitResolution {
                foreign_path: entry.foreign_path(),
                is_object_safe: entry.is_object_safe,
                supports_local_handle: entry.supports_local_handle(),
                local_handle_path: entry.local_handle_path(),
            }),
            _ => None,
        }
    }

    fn path(path: &syn::Path) -> Vec<String> {
        let mut segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        if segments
            .first()
            .is_some_and(|segment| matches!(segment.as_str(), "crate" | "self" | "super"))
        {
            segments.remove(0);
        }
        segments
    }
}

pub(super) fn build_callback_trait_registry(
    sources: &[IndexedSource],
) -> syn::Result<CallbackTraitRegistry> {
    let mut entries = Vec::new();

    sources
        .iter()
        .try_for_each(|source| collect_source_callbacks(source, &mut entries))?;

    Ok(CallbackTraitRegistry { entries })
}

fn collect_source_callbacks(
    source: &IndexedSource,
    entries: &mut Vec<CallbackTraitEntry>,
) -> syn::Result<()> {
    source.modules().iter().try_for_each(|source_module| {
        let mut collector = CallbackTraitCollector {
            root_path: source.root_path().to_vec(),
            module_path: source.module_path(source_module).to_vec(),
            entries,
        };
        source_module
            .items()
            .iter()
            .try_for_each(|item| collector.collect_item(item))
    })
}

struct CallbackTraitCollector<'a> {
    root_path: Vec<String>,
    module_path: Vec<String>,
    entries: &'a mut Vec<CallbackTraitEntry>,
}

impl<'a> CallbackTraitCollector<'a> {
    fn collect_item(&mut self, item: &Item) -> syn::Result<()> {
        match item {
            Item::Trait(item_trait) => self.collect_trait(item_trait),
            _ => Ok(()),
        }
    }

    fn collect_trait(&mut self, item_trait: &ItemTrait) -> syn::Result<()> {
        let trait_descriptor = CallbackTraitDescriptor::new(item_trait);
        if !trait_descriptor.is_exported_callback() {
            return Ok(());
        }

        let entry = CallbackTraitEntry {
            root_path: self.root_path.clone(),
            module_path: self.module_path.clone(),
            trait_name: item_trait.ident.to_string(),
            is_object_safe: trait_descriptor.is_object_safe(),
            has_async_methods: trait_descriptor.has_async_methods(),
        };
        self.entries.push(entry);
        Ok(())
    }
}

impl<'a> CallbackTraitDescriptor<'a> {
    fn new(item_trait: &'a ItemTrait) -> Self {
        Self { item_trait }
    }

    fn is_exported_callback(&self) -> bool {
        self.item_trait.attrs.iter().any(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "export" || segment.ident == "ffi_trait")
        })
    }

    fn has_async_methods(&self) -> bool {
        self.item_trait.items.iter().any(|item| {
            matches!(
                item,
                syn::TraitItem::Fn(method) if method.sig.asyncness.is_some()
            )
        })
    }

    fn has_async_trait_attr(&self) -> bool {
        self.item_trait.attrs.iter().any(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "async_trait")
        })
    }

    fn is_object_safe(&self) -> bool {
        !self.has_async_methods() || self.has_async_trait_attr()
    }
}

impl CallbackTraitEntry {
    fn resolution_path(&self) -> Vec<String> {
        self.root_path
            .iter()
            .cloned()
            .chain(self.module_path.iter().cloned())
            .collect()
    }

    fn helper_path_root(&self) -> Vec<String> {
        if self.root_path.is_empty() {
            vec!["crate".to_string()]
        } else {
            self.root_path.clone()
        }
    }

    fn callback_handle_helper_name(&self) -> String {
        format!(
            "__boltffi_local_{}_handle",
            naming::to_snake_case(&self.trait_name)
        )
    }

    fn foreign_path(&self) -> syn::Path {
        let mut segments = self.helper_path_root();
        segments.extend(self.module_path.iter().cloned());
        segments.push(format!("Foreign{}", self.trait_name));
        syn::parse_str(&segments.join("::")).unwrap_or_else(|_| syn::Path {
            leading_colon: None,
            segments: syn::punctuated::Punctuated::new(),
        })
    }

    fn local_handle_path(&self) -> syn::Path {
        let mut segments = self.helper_path_root();
        segments.extend(self.module_path.iter().cloned());
        segments.push(self.callback_handle_helper_name());
        syn::parse_str(&segments.join("::")).unwrap_or_else(|_| syn::Path {
            leading_colon: None,
            segments: syn::punctuated::Punctuated::new(),
        })
    }

    fn supports_local_handle(&self) -> bool {
        self.is_object_safe && !self.has_async_methods
    }
}
