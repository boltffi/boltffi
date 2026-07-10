use std::collections::{HashMap, HashSet};

use syn::{Attribute, Item, ItemImpl, Type};

use super::reexports::ReExport;
use crate::index::PathResolver;
use crate::index::source::IndexedSource;
use crate::index::type_paths::TypePathKey;

#[derive(Default, Clone)]
pub struct ClassTypeRegistry {
    paths: HashSet<Vec<String>>,
    unique_names: HashSet<String>,
    path_resolver: PathResolver,
}

pub enum ClassParam {
    SharedRef { rust_type: Type, nullable: bool },
    MutableRef { rust_type: Type, nullable: bool },
}

impl ClassTypeRegistry {
    fn insert(&mut self, qualified_path: Vec<String>) {
        self.paths.insert(qualified_path);
    }

    fn contains_path_segments(&self, path_segments: &[String]) -> bool {
        if path_segments.len() == 1 {
            return path_segments
                .first()
                .is_some_and(|name| self.unique_names.contains(name));
        }

        self.paths
            .iter()
            .any(|registered_path| registered_path.as_slice() == path_segments)
            || self
                .paths
                .iter()
                .filter(|registered_path| registered_path.ends_with(path_segments))
                .count()
                == 1
    }

    fn finalize_unique_names(&mut self) {
        let name_counts = self.paths.iter().fold(
            HashMap::<String, usize>::new(),
            |mut counts, qualified_path| {
                if let Some(name) = qualified_path.last() {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
                counts
            },
        );

        self.unique_names = self
            .paths
            .iter()
            .filter_map(|qualified_path| {
                qualified_path
                    .last()
                    .filter(|name| name_counts.get(*name).copied() == Some(1))
                    .cloned()
            })
            .collect();
    }

    pub fn contains(&self, ty: &Type) -> bool {
        self.type_path_key(ty).is_some_and(|type_path_key| {
            if type_path_key.is_single_segment() {
                return type_path_key
                    .first_segment()
                    .is_some_and(|name| self.unique_names.contains(name));
            }

            self.paths
                .iter()
                .any(|registered_path| registered_path.as_slice() == type_path_key.segments())
                || self
                    .paths
                    .iter()
                    .any(|registered_path| type_path_key.has_suffix(registered_path))
        })
    }

    pub fn is_class_type(&self, ty: &Type) -> bool {
        self.contains(ty)
    }

    pub fn class_param(&self, ty: &Type) -> Option<ClassParam> {
        self.required_class_param(ty)
            .or_else(|| self.optional_class_param(ty))
    }

    fn required_class_param(&self, ty: &Type) -> Option<ClassParam> {
        let Type::Reference(reference) = ty else {
            return None;
        };
        self.contains(reference.elem.as_ref()).then(|| {
            let rust_type = (*reference.elem).clone();
            if reference.mutability.is_some() {
                ClassParam::MutableRef {
                    rust_type,
                    nullable: false,
                }
            } else {
                ClassParam::SharedRef {
                    rust_type,
                    nullable: false,
                }
            }
        })
    }

    fn optional_class_param(&self, ty: &Type) -> Option<ClassParam> {
        let Type::Path(type_path) = ty else {
            return None;
        };
        let segment = type_path.path.segments.last()?;
        if segment.ident != "Option" {
            return None;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        let syn::GenericArgument::Type(inner_type) = arguments.args.first()? else {
            return None;
        };
        match self.required_class_param(inner_type)? {
            ClassParam::SharedRef { rust_type, .. } => Some(ClassParam::SharedRef {
                rust_type,
                nullable: true,
            }),
            ClassParam::MutableRef { rust_type, .. } => Some(ClassParam::MutableRef {
                rust_type,
                nullable: true,
            }),
        }
    }

    fn type_path_key(&self, ty: &Type) -> Option<TypePathKey> {
        match ty {
            Type::Path(type_path) if type_path.qself.is_none() => {
                let resolved_path = self.path_resolver.resolve(&type_path.path).into_path();
                Some(TypePathKey::from_path(&resolved_path))
            }
            Type::Group(group) => self.type_path_key(group.elem.as_ref()),
            Type::Paren(paren) => self.type_path_key(paren.elem.as_ref()),
            _ => None,
        }
    }
}

#[cfg(test)]
impl ClassTypeRegistry {
    pub fn with_entries(entries: &[&str]) -> Self {
        let mut registry = Self::default();
        entries
            .iter()
            .map(|entry| entry.split("::").map(str::to_string).collect::<Vec<_>>())
            .for_each(|segments| registry.insert(segments));
        registry.finalize_unique_names();
        registry
    }

    pub fn with_entries_and_use_aliases(entries: &[&str], aliases: &[(&str, &str)]) -> Self {
        let mut registry = Self::with_entries(entries);
        registry.path_resolver = PathResolver::with_use_aliases(aliases);
        registry
    }

    pub fn with_paths(paths: &[&[&str]]) -> Self {
        let mut registry = Self::default();
        paths
            .iter()
            .map(|path| path.iter().map(|segment| segment.to_string()).collect())
            .for_each(|segments| registry.insert(segments));
        registry.finalize_unique_names();
        registry
    }
}

pub fn build_class_type_registry(
    sources: &[IndexedSource],
    path_resolver: PathResolver,
) -> syn::Result<ClassTypeRegistry> {
    let mut registry = ClassTypeRegistry {
        path_resolver,
        ..ClassTypeRegistry::default()
    };
    sources
        .iter()
        .try_for_each(|source| collect_root_types(source, &mut registry))?;
    registry.finalize_unique_names();
    sources
        .iter()
        .try_for_each(|source| collect_root_reexports(source, &mut registry))?;
    registry.finalize_unique_names();
    Ok(registry)
}

fn collect_root_types(source: &IndexedSource, registry: &mut ClassTypeRegistry) -> syn::Result<()> {
    source.modules().iter().try_for_each(|source_module| {
        let module_path = source.path(source_module);
        let mut collector = ClassTypeCollector {
            module_path,
            registry,
        };
        source_module
            .items()
            .iter()
            .try_for_each(|item| collector.collect_item(item))
    })
}

fn collect_root_reexports(
    source: &IndexedSource,
    registry: &mut ClassTypeRegistry,
) -> syn::Result<()> {
    source.modules().iter().try_for_each(|source_module| {
        let module_path = source.path(source_module);
        let mut collector = ClassTypeCollector {
            module_path,
            registry,
        };
        source_module
            .items()
            .iter()
            .try_for_each(|item| collector.collect_reexport_item(item))
    })
}

struct ClassTypeCollector<'a> {
    module_path: Vec<String>,
    registry: &'a mut ClassTypeRegistry,
}

impl<'a> ClassTypeCollector<'a> {
    fn collect_item(&mut self, item: &Item) -> syn::Result<()> {
        match item {
            Item::Impl(item_impl) => {
                self.collect_impl(item_impl);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn collect_reexport_item(&mut self, item: &Item) -> syn::Result<()> {
        match item {
            Item::Use(_) => {
                ReExport::from_item(item)
                    .into_iter()
                    .for_each(|reexport| self.collect_reexport(reexport));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn collect_impl(&mut self, item_impl: &ItemImpl) {
        if !Self::is_class_export_impl(item_impl) {
            return;
        }

        let Some(type_path_key) = TypePathKey::from_type(item_impl.self_ty.as_ref()) else {
            return;
        };

        self.registry
            .insert(self.qualified_class_path(type_path_key));
    }

    fn collect_reexport(&mut self, reexport: ReExport) {
        if !self.registry.contains_path_segments(reexport.target()) {
            return;
        }
        self.registry.insert(
            self.module_path
                .iter()
                .cloned()
                .chain(std::iter::once(reexport.alias().to_string()))
                .collect(),
        );
    }

    fn qualified_class_path(&self, type_path_key: TypePathKey) -> Vec<String> {
        let segments = type_path_key.into_segments();
        if segments.len() == 1 {
            return self.module_path.iter().cloned().chain(segments).collect();
        }
        segments
    }

    fn is_class_export_impl(item_impl: &ItemImpl) -> bool {
        Self::has_boltffi_attribute(&item_impl.attrs, "export")
    }

    fn has_boltffi_attribute(attributes: &[Attribute], name: &str) -> bool {
        attributes
            .iter()
            .any(|attribute| Self::is_boltffi_attribute(attribute, name))
    }

    fn is_boltffi_attribute(attribute: &Attribute, name: &str) -> bool {
        let path = attribute.path();
        if path.is_ident(name) {
            return true;
        }

        path.segments.len() == 2
            && path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "boltffi")
            && path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn renamed_import_resolves_to_registered_class_path() {
        let registry = ClassTypeRegistry::with_entries_and_use_aliases(
            &["map::Marker"],
            &[("Pin", "crate::map::Marker")],
        );

        assert!(registry.contains(&parse_quote!(Pin)));
    }

    #[test]
    fn unrelated_namespaced_export_attribute_is_ignored() {
        let item_impl: ItemImpl = syn::parse_quote! {
            #[other::export]
            impl Marker {}
        };
        let mut registry = ClassTypeRegistry::default();
        let mut collector = ClassTypeCollector {
            module_path: vec!["map".to_string()],
            registry: &mut registry,
        };

        collector.collect_impl(&item_impl);
        collector.registry.finalize_unique_names();

        assert!(!collector.registry.contains(&parse_quote!(Marker)));
    }
}
