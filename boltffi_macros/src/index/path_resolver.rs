use boltffi_scan::SourceModule;
use proc_macro2::Span;
use std::collections::HashMap;
use syn::punctuated::Punctuated;
use syn::{Item, Path, PathArguments, PathSegment, Type, UseTree};

#[derive(Default, Clone)]
pub(crate) struct PathResolver {
    use_aliases: HashMap<String, Vec<String>>,
    type_aliases: HashMap<String, Vec<String>>,
}

impl PathResolver {
    pub(super) fn build(source_modules: &[SourceModule]) -> Self {
        source_modules
            .iter()
            .map(|source_module| Self::from_items(source_module.items()))
            .fold(Self::default(), |mut resolver, next| {
                resolver.merge(next);
                resolver
            })
    }

    pub(crate) fn resolve(&self, path: &Path) -> ResolvedPath {
        let path_segments = PathSegments::from_path(path);
        let Some(first_segment) = path_segments.first() else {
            return ResolvedPath(path.clone());
        };
        let first_name = first_segment.ident.to_string();
        let is_single_segment = path_segments.len() == 1;

        let resolved_segments = self
            .use_aliases
            .get(&first_name)
            .map(|prefix| {
                let rest = path_segments.iter().skip(1).cloned();
                prefix
                    .iter()
                    .map(|segment| PathSegments::segment_from_name(segment))
                    .chain(rest)
                    .collect::<Vec<_>>()
            })
            .or_else(|| {
                is_single_segment
                    .then(|| self.type_aliases.get(&first_name).cloned())
                    .flatten()
                    .map(|segments| {
                        segments
                            .into_iter()
                            .map(|segment| PathSegments::segment_from_name(&segment))
                            .collect::<Vec<_>>()
                    })
            });

        let Some(mut resolved_segments) = resolved_segments else {
            return ResolvedPath(path.clone());
        };

        if is_single_segment && let Some(last_segment) = resolved_segments.last_mut() {
            last_segment.arguments = first_segment.arguments.clone();
        }

        ResolvedPath(PathSegments::into_path(resolved_segments))
    }

    pub(crate) fn resolve_foreign_path(&self, path: &Path) -> Path {
        self.resolve(path).with_foreign_leaf()
    }

    fn from_items(items: &[Item]) -> Self {
        let mut resolver = Self::default();

        items
            .iter()
            .filter_map(|item| match item {
                Item::Use(item_use) => Some(&item_use.tree),
                _ => None,
            })
            .for_each(|use_tree| resolver.collect_use_tree(Vec::new(), use_tree));

        items
            .iter()
            .filter_map(|item| match item {
                Item::Type(item_type) => Some(item_type),
                _ => None,
            })
            .filter_map(|item_type| {
                let target = match item_type.ty.as_ref() {
                    Type::Path(type_path) => Some(PathSegments::names_from_path(&type_path.path)),
                    _ => None,
                }?;
                Some((item_type.ident.to_string(), target))
            })
            .for_each(|(alias, target)| {
                resolver.type_aliases.insert(alias, target);
            });

        resolver
    }

    fn merge(&mut self, other: Self) {
        self.use_aliases.extend(other.use_aliases);
        self.type_aliases.extend(other.type_aliases);
    }

    fn collect_use_tree(&mut self, prefix: Vec<String>, use_tree: &UseTree) {
        match use_tree {
            UseTree::Path(path) => {
                let mut next_prefix = prefix;
                next_prefix.push(path.ident.to_string());
                self.collect_use_tree(next_prefix, &path.tree);
            }
            UseTree::Name(name) => {
                let mut target = prefix;
                target.push(name.ident.to_string());
                self.use_aliases.insert(name.ident.to_string(), target);
            }
            UseTree::Rename(rename) => {
                let mut target = prefix;
                target.push(rename.ident.to_string());
                self.use_aliases.insert(rename.rename.to_string(), target);
            }
            UseTree::Group(group) => group
                .items
                .iter()
                .for_each(|item| self.collect_use_tree(prefix.clone(), item)),
            UseTree::Glob(_) => {}
        }
    }
}

pub(crate) struct ResolvedPath(Path);

impl ResolvedPath {
    pub(crate) fn into_path(self) -> Path {
        self.0
    }

    pub(crate) fn with_foreign_leaf(mut self) -> Path {
        let foreign_ident = self
            .0
            .segments
            .last()
            .map(|segment| {
                syn::Ident::new(&format!("Foreign{}", segment.ident), segment.ident.span())
            })
            .unwrap_or_else(|| syn::Ident::new("Foreign", Span::call_site()));
        if let Some(last_segment) = self.0.segments.last_mut() {
            last_segment.ident = foreign_ident;
        }
        self.0
    }
}

#[cfg(test)]
impl PathResolver {
    pub(crate) fn with_use_aliases(aliases: &[(&str, &str)]) -> Self {
        let use_aliases = aliases
            .iter()
            .map(|(alias, target)| {
                (
                    (*alias).to_string(),
                    target.split("::").map(str::to_string).collect::<Vec<_>>(),
                )
            })
            .collect();

        Self {
            use_aliases,
            type_aliases: HashMap::new(),
        }
    }
}

struct PathSegments;

impl PathSegments {
    fn from_path(path: &Path) -> Vec<PathSegment> {
        path.segments.iter().cloned().collect()
    }

    fn names_from_path(path: &Path) -> Vec<String> {
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect()
    }

    fn segment_from_name(name: &str) -> PathSegment {
        PathSegment {
            ident: syn::Ident::new(name, Span::call_site()),
            arguments: PathArguments::None,
        }
    }

    fn into_path(segments: Vec<PathSegment>) -> Path {
        Path {
            leading_colon: None,
            segments: Punctuated::from_iter(segments),
        }
    }
}
