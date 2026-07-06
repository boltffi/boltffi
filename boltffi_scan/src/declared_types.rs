use std::collections::{HashMap, HashSet};

use boltffi_ast::{ClassId, CustomRemoteType, CustomTypeId, EnumId, RecordId, TraitId};

use crate::impl_target;
use crate::items;
use crate::marked::MarkedItems;
use crate::path::{ImportLookup, ModuleScope, PathExpansion};
use crate::source_tree::{SourceModule, SourceTree};
use crate::{ScanError, spelling};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DeclaredType {
    Record(RecordId),
    Enum(EnumId),
    Trait(TraitId),
    Class(ClassId),
    Custom(CustomTypeId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SourceType<'a> {
    Declared(&'a DeclaredType),
    Unregistered,
    External(String),
    Unknown,
}

#[derive(Default)]
pub(super) struct DeclaredTypes {
    by_path: HashMap<String, DeclaredType>,
    custom_by_remote_exact: HashMap<String, CustomTypeId>,
    custom_by_remote_shape: HashMap<String, CustomRemoteShapeMatch>,
    source_types: TypeNamespace,
}

impl DeclaredTypes {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn index(
        source_tree: &SourceTree,
        marked: &MarkedItems<'_>,
    ) -> Result<Self, ScanError> {
        let mut declared_types = Self {
            source_types: TypeNamespace::index(source_tree),
            ..Self::default()
        };
        marked.records().iter().try_for_each(|marked| {
            declared_types.register(DeclaredType::Record(RecordId::new(
                marked.module().qualified(&marked.item().ident.to_string()),
            )))
        })?;
        marked.enums().iter().try_for_each(|marked| {
            declared_types.register(DeclaredType::Enum(EnumId::new(
                marked.module().qualified(&marked.item().ident.to_string()),
            )))
        })?;
        marked.traits().iter().try_for_each(|marked| {
            declared_types.register(DeclaredType::Trait(TraitId::new(
                marked.module().qualified(&marked.item().ident.to_string()),
            )))
        })?;
        marked.classes().iter().try_for_each(|marked| {
            let target = impl_target::Target::class(marked.item())?;
            let id = declared_types
                .resolve_impl_target(marked.scope(), &target)?
                .map(ClassId::new)
                .ok_or_else(|| ScanError::UnsupportedClassImpl {
                    target: target.spelling().to_owned(),
                })?;
            declared_types.register(DeclaredType::Class(id))
        })?;
        marked.customs().iter().try_for_each(|marked| {
            let spec = items::custom_type::Spec::parse(marked)?;
            let id = CustomTypeId::new(marked.module().qualified(&spec.name().to_string()));
            // A custom_ffi impl proves a real type lives at the declared
            // path (the defining struct may be macro-generated and thus
            // missing from the item index), so re-export chains can verify
            // against it when picking a public spelling.
            if spec.declares_source_type() {
                declared_types.source_types.ensure_path(id.as_str());
            }
            declared_types.register_custom_type(marked.scope(), id, spec.remote_type())
        })?;
        Ok(declared_types)
    }

    #[cfg(test)]
    pub(super) fn register_record(&mut self, id: RecordId) {
        self.register(DeclaredType::Record(id))
            .expect("test declaration registration must not conflict");
    }

    #[cfg(test)]
    pub(super) fn register_enum(&mut self, id: EnumId) {
        self.register(DeclaredType::Enum(id))
            .expect("test declaration registration must not conflict");
    }

    #[cfg(test)]
    pub(super) fn register_trait(&mut self, id: TraitId) {
        self.register(DeclaredType::Trait(id))
            .expect("test declaration registration must not conflict");
    }

    #[cfg(test)]
    pub(super) fn register_class(&mut self, id: ClassId) {
        self.register(DeclaredType::Class(id))
            .expect("test declaration registration must not conflict");
    }

    pub(super) fn resolve(&self, path: &str) -> Option<&DeclaredType> {
        self.by_path.get(path)
    }

    pub(super) fn paths(&self) -> impl Iterator<Item = &str> {
        self.by_path.keys().map(String::as_str)
    }

    pub(super) fn resolve_type_in_scope(
        &self,
        scope: &ModuleScope,
        path: &syn::Path,
    ) -> Result<SourceType<'_>, ScanError> {
        let Some(path) = self.resolve_source_path(scope, path, || spelling::path(path))? else {
            return Ok(SourceType::Unknown);
        };
        Ok(match self.by_path.get(&path) {
            Some(declared_type) => SourceType::Declared(declared_type),
            None if self.source_types.contains_path(&path) => SourceType::Unregistered,
            None => SourceType::External(path),
        })
    }

    pub(super) fn root_visible_path(
        &self,
        id: &str,
        root_crate: &str,
        direct_dependencies: &[String],
    ) -> Option<String> {
        let segments = id.split("::").collect::<Vec<_>>();
        let root = segments.first().copied()?;
        if root == root_crate
            || direct_dependencies
                .iter()
                .any(|dependency| dependency == root)
        {
            return self.shallowest_public_path(&segments);
        }
        let leaf = segments.last().copied()?;
        let mut candidates = direct_dependencies
            .iter()
            .filter_map(|dependency| {
                let candidate = format!("{dependency}::{leaf}");
                match self.source_types.resolve_public_path(candidate.clone()) {
                    TypeResolution::Known(path) if path == id => Some(candidate),
                    TypeResolution::Known(_)
                    | TypeResolution::Ambiguous
                    | TypeResolution::Unknown => None,
                }
            })
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();
        match candidates.as_slice() {
            [candidate] => Some(candidate.clone()),
            [] | [_, ..] => None,
        }
    }

    fn shallowest_public_path(&self, segments: &[&str]) -> Option<String> {
        let (leaf, modules) = segments.split_last()?;
        let id = segments.join("::");
        (1..modules.len())
            .map(|depth| {
                modules[..depth]
                    .iter()
                    .copied()
                    .chain(std::iter::once(*leaf))
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .find(|candidate| {
                matches!(
                    self.source_types.resolve_public_path(candidate.clone()),
                    TypeResolution::Known(path) if path == id
                )
            })
            .or(Some(id))
    }

    pub(super) fn resolve_impl_target(
        &self,
        scope: &ModuleScope,
        target: &impl_target::Target<'_>,
    ) -> Result<Option<String>, ScanError> {
        let Some(path) = target.path() else {
            return Ok(None);
        };
        self.resolve_source_path(scope, path, || target.spelling().to_owned())
    }

    pub(super) fn resolve_custom_remote(
        &self,
        scope: &ModuleScope,
        ty: &syn::Type,
    ) -> Result<Option<&CustomTypeId>, ScanError> {
        self.resolve_custom_remote_with_aliases(scope, ty, &mut HashSet::new())
    }

    pub(super) fn resolves_type_alias(
        &self,
        scope: &ModuleScope,
        path: &syn::Path,
    ) -> Result<bool, ScanError> {
        match self.source_types.resolve_alias(scope, path) {
            AliasResolution::Known(_) => Ok(true),
            AliasResolution::Unknown => Ok(false),
            AliasResolution::Ambiguous => Err(ScanError::AmbiguousPath {
                path: spelling::path(path),
            }),
        }
    }

    fn resolve_custom_remote_with_aliases<'a>(
        &'a self,
        scope: &ModuleScope,
        ty: &syn::Type,
        visited: &mut HashSet<String>,
    ) -> Result<Option<&'a CustomTypeId>, ScanError> {
        if let Some(id) = self.resolve_custom_remote_direct(scope, ty)? {
            return Ok(Some(id));
        }
        let syn::Type::Path(type_path) = crate::type_expr::unwrapped(ty) else {
            return Ok(None);
        };
        match self.source_types.resolve_alias(scope, &type_path.path) {
            AliasResolution::Known(alias) if visited.insert(alias.path.clone()) => {
                self.resolve_custom_remote_with_aliases(&alias.scope, &alias.target, visited)
            }
            AliasResolution::Known(_) | AliasResolution::Unknown => Ok(None),
            AliasResolution::Ambiguous => Err(ScanError::AmbiguousPath {
                path: spelling::path(&type_path.path),
            }),
        }
    }

    fn resolve_custom_remote_direct(
        &self,
        scope: &ModuleScope,
        ty: &syn::Type,
    ) -> Result<Option<&CustomTypeId>, ScanError> {
        let remote = match items::custom_type::RemoteType::scan(ty) {
            Ok(remote) => remote,
            Err(_) => return Ok(None),
        };
        let identity = items::custom_type::RemoteIdentity::query(scope, &remote);
        if identity.ambiguous() {
            return Err(ScanError::AmbiguousPath {
                path: spelling::ty(ty),
            });
        }
        let mut exact_matches = identity
            .exact()
            .iter()
            .filter_map(|exact| self.custom_by_remote_exact.get(exact))
            .collect::<Vec<_>>();
        exact_matches.sort_by_key(|id| id.as_str());
        exact_matches.dedup();
        match exact_matches.as_slice() {
            [id] => Ok(Some(*id)),
            [] => Ok(match identity.shape() {
                Some(shape) => match self.custom_by_remote_shape.get(shape) {
                    Some(CustomRemoteShapeMatch::Unique(id)) => Some(id),
                    Some(CustomRemoteShapeMatch::Ambiguous) | None => None,
                },
                None => None,
            }),
            _ => Err(ScanError::AmbiguousPath {
                path: spelling::ty(ty),
            }),
        }
    }

    fn resolve_source_path(
        &self,
        scope: &ModuleScope,
        path: &syn::Path,
        ambiguous_path: impl FnOnce() -> String,
    ) -> Result<Option<String>, ScanError> {
        match self.source_types.resolve(scope, path) {
            TypeResolution::Known(path) => Ok(Some(path)),
            TypeResolution::Unknown => Ok(None),
            TypeResolution::Ambiguous => Err(ScanError::AmbiguousPath {
                path: ambiguous_path(),
            }),
        }
    }

    fn register_custom_type(
        &mut self,
        scope: &ModuleScope,
        id: CustomTypeId,
        remote: &CustomRemoteType,
    ) -> Result<(), ScanError> {
        self.register(DeclaredType::Custom(id.clone()))?;
        let identity = items::custom_type::RemoteIdentity::registered(scope, remote);
        if identity.ambiguous() || identity.exact().is_empty() {
            return Err(ScanError::InvalidCustomType {
                message: "ambiguous custom remote type".to_owned(),
            });
        }
        identity
            .exact()
            .iter()
            .try_for_each(|exact| self.insert_custom_remote_key(exact.clone(), id.clone()))?;
        if let Some(shape) = identity.shape() {
            self.insert_custom_remote_shape(shape.to_owned(), id);
        }
        Ok(())
    }

    fn insert_custom_remote_key(&mut self, key: String, id: CustomTypeId) -> Result<(), ScanError> {
        match self.custom_by_remote_exact.get(&key) {
            Some(existing) if existing == &id => Ok(()),
            Some(existing) => Err(ScanError::ConflictingDeclarations {
                path: key,
                first: format!("custom type {}", existing.as_str()),
                second: format!("custom type {}", id.as_str()),
            }),
            None => {
                self.custom_by_remote_exact.insert(key, id);
                Ok(())
            }
        }
    }

    fn insert_custom_remote_shape(&mut self, shape: String, id: CustomTypeId) {
        match self.custom_by_remote_shape.get(&shape) {
            Some(CustomRemoteShapeMatch::Unique(existing)) if existing == &id => {}
            Some(CustomRemoteShapeMatch::Unique(_)) | Some(CustomRemoteShapeMatch::Ambiguous) => {
                self.custom_by_remote_shape
                    .insert(shape, CustomRemoteShapeMatch::Ambiguous);
            }
            None => {
                self.custom_by_remote_shape
                    .insert(shape, CustomRemoteShapeMatch::Unique(id));
            }
        }
    }

    fn register(&mut self, declared_type: DeclaredType) -> Result<(), ScanError> {
        let path = declared_type.path().to_owned();
        match self.by_path.get(&path) {
            Some(existing)
                if existing.kind() == declared_type.kind()
                    && declared_type.kind().allows_redeclaration() =>
            {
                Ok(())
            }
            Some(existing) => Err(ScanError::ConflictingDeclarations {
                path,
                first: existing.kind().as_str().to_owned(),
                second: declared_type.kind().as_str().to_owned(),
            }),
            None => {
                if declared_type.kind().is_source_type() {
                    self.source_types.ensure_path(&path);
                }
                self.by_path.insert(path, declared_type);
                Ok(())
            }
        }
    }
}

impl DeclaredType {
    fn path(&self) -> &str {
        match self {
            Self::Record(id) => id.as_str(),
            Self::Enum(id) => id.as_str(),
            Self::Trait(id) => id.as_str(),
            Self::Class(id) => id.as_str(),
            Self::Custom(id) => id.as_str(),
        }
    }

    fn kind(&self) -> DeclaredKind {
        match self {
            Self::Record(_) => DeclaredKind::Record,
            Self::Enum(_) => DeclaredKind::Enum,
            Self::Trait(_) => DeclaredKind::Trait,
            Self::Class(_) => DeclaredKind::Class,
            Self::Custom(_) => DeclaredKind::Custom,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CustomRemoteShapeMatch {
    Unique(CustomTypeId),
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeclaredKind {
    Record,
    Enum,
    Trait,
    Class,
    Custom,
}

impl DeclaredKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Class => "class",
            Self::Custom => "custom type",
        }
    }

    const fn allows_redeclaration(self) -> bool {
        matches!(self, Self::Class)
    }

    const fn is_source_type(self) -> bool {
        !matches!(self, Self::Custom)
    }
}

#[derive(Default)]
struct TypeNamespace {
    by_path: HashMap<String, TypeBinding>,
    by_module: HashMap<String, HashMap<String, TypeBinding>>,
    aliases: HashMap<String, TypeAlias>,
    scopes: HashMap<String, ModuleScope>,
}

struct TypeAlias {
    path: String,
    scope: ModuleScope,
    target: syn::Type,
}

#[derive(Clone, Copy)]
enum AliasResolution<'a> {
    Known(&'a TypeAlias),
    Ambiguous,
    Unknown,
}

impl TypeNamespace {
    fn index(source_tree: &SourceTree) -> Self {
        source_tree
            .modules()
            .iter()
            .fold(Self::default(), |mut namespace, module| {
                namespace.insert_scope(module);
                module
                    .items()
                    .iter()
                    .for_each(|item| namespace.insert_item(module, item));
                namespace
            })
    }

    fn ensure_path(&mut self, path: &str) {
        self.by_path
            .entry(path.to_owned())
            .or_insert_with(|| TypeBinding::Unique(path.to_owned()));
    }

    fn contains_path(&self, path: &str) -> bool {
        self.by_path.contains_key(path)
    }

    fn resolve_alias(&self, scope: &ModuleScope, path: &syn::Path) -> AliasResolution<'_> {
        match self.resolve(scope, path) {
            TypeResolution::Known(path) => self
                .aliases
                .get(&path)
                .map(AliasResolution::Known)
                .unwrap_or(AliasResolution::Unknown),
            TypeResolution::Ambiguous => AliasResolution::Ambiguous,
            TypeResolution::Unknown => AliasResolution::Unknown,
        }
    }

    fn resolve(&self, scope: &ModuleScope, path: &syn::Path) -> TypeResolution {
        match scope.expand(path) {
            PathExpansion::Relative(path) => self.resolve_relative(scope, path),
            PathExpansion::Imported { local, path } => self.resolve_imported(scope, &local, path),
            PathExpansion::Qualified(path) => self.resolve_qualified(path),
            PathExpansion::Ambiguous => TypeResolution::Ambiguous,
            PathExpansion::Unsupported => TypeResolution::Unknown,
        }
    }

    fn resolve_relative(&self, scope: &ModuleScope, path: String) -> TypeResolution {
        match self.local_first_segment(scope, &path) {
            Some(TypeBinding::Unique(_)) => self.resolve_qualified(path),
            Some(TypeBinding::Ambiguous) => TypeResolution::Ambiguous,
            None => match self.by_path.get(&path) {
                Some(TypeBinding::Unique(path)) => TypeResolution::Known(path.clone()),
                Some(TypeBinding::Ambiguous) => TypeResolution::Ambiguous,
                None => self.resolve_globs(scope, &path),
            },
        }
    }

    fn resolve_imported(&self, scope: &ModuleScope, local: &str, path: String) -> TypeResolution {
        if self.local_name(scope, local).is_some() {
            return TypeResolution::Ambiguous;
        }
        match self.resolve_candidate_paths(
            self.import_candidate_paths(scope, &path),
            &mut HashSet::new(),
        ) {
            TypeResolution::Unknown => TypeResolution::Known(path),
            resolution => resolution,
        }
    }

    fn resolve_qualified(&self, path: String) -> TypeResolution {
        match self.by_path.get(&path) {
            Some(TypeBinding::Unique(path)) => TypeResolution::Known(path.clone()),
            Some(TypeBinding::Ambiguous) => TypeResolution::Ambiguous,
            None => TypeResolution::Unknown,
        }
    }

    fn resolve_public_path(&self, path: String) -> TypeResolution {
        match self.resolve_qualified(path.clone()) {
            TypeResolution::Unknown => self.resolve_reexported(&path),
            resolution => resolution,
        }
    }

    fn resolve_globs(&self, scope: &ModuleScope, path: &str) -> TypeResolution {
        let segments = path
            .split("::")
            .skip(scope.path().segments().len())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        self.resolve_candidate_paths(
            self.glob_candidate_paths(scope, &segments),
            &mut HashSet::new(),
        )
    }

    fn resolve_reexported(&self, path: &str) -> TypeResolution {
        self.resolve_reexported_with_visited(path, &mut HashSet::new())
    }

    fn resolve_reexported_with_visited(
        &self,
        path: &str,
        visited: &mut HashSet<String>,
    ) -> TypeResolution {
        if !visited.insert(path.to_owned()) {
            return TypeResolution::Unknown;
        }
        let segments = path.split("::").map(ToOwned::to_owned).collect::<Vec<_>>();
        let Some((name, module_segments)) = segments.split_last() else {
            return TypeResolution::Unknown;
        };
        let module_path = module_segments.join("::");
        let Some(scope) = self.scopes.get(&module_path) else {
            return TypeResolution::Unknown;
        };
        match self.resolve_explicit_reexport(scope, name, visited) {
            TypeResolution::Known(path) => return TypeResolution::Known(path),
            TypeResolution::Ambiguous => return TypeResolution::Ambiguous,
            TypeResolution::Unknown => {}
        }
        let segments = [name.clone()];
        self.resolve_candidate_paths(
            self.reexport_glob_candidate_paths(scope, &segments),
            visited,
        )
    }

    fn resolve_explicit_reexport(
        &self,
        scope: &ModuleScope,
        name: &str,
        visited: &mut HashSet<String>,
    ) -> TypeResolution {
        match scope.reexported(name) {
            ImportLookup::Unique(imported) => {
                let raw = imported.join("::");
                self.resolve_candidate_paths(self.import_candidate_paths(scope, &raw), visited)
            }
            ImportLookup::Ambiguous => TypeResolution::Ambiguous,
            ImportLookup::None => TypeResolution::Unknown,
        }
    }

    fn resolve_candidate_paths(
        &self,
        candidates: Vec<String>,
        visited: &mut HashSet<String>,
    ) -> TypeResolution {
        let matches = candidates
            .into_iter()
            .try_fold(Vec::new(), |mut matches, candidate| {
                match self.by_path.get(&candidate) {
                    Some(TypeBinding::Unique(path)) => matches.push(path.clone()),
                    Some(TypeBinding::Ambiguous) => return Err(TypeResolution::Ambiguous),
                    None => match self.resolve_reexported_with_visited(&candidate, visited) {
                        TypeResolution::Known(path) => matches.push(path),
                        TypeResolution::Ambiguous => return Err(TypeResolution::Ambiguous),
                        TypeResolution::Unknown => {}
                    },
                }
                Ok(matches)
            });
        let mut matches = match matches {
            Ok(matches) => matches,
            Err(resolution) => return resolution,
        };
        matches.sort();
        matches.dedup();
        match matches.as_slice() {
            [path] => TypeResolution::Known(path.clone()),
            [] => TypeResolution::Unknown,
            _ => TypeResolution::Ambiguous,
        }
    }

    fn import_candidate_paths(&self, scope: &ModuleScope, path: &str) -> Vec<String> {
        let qualified = self.module_qualified_candidate(scope, path);
        match qualified == path {
            true => vec![path.to_owned()],
            false => vec![path.to_owned(), qualified],
        }
    }

    fn glob_candidate_paths(&self, scope: &ModuleScope, segments: &[String]) -> Vec<String> {
        scope
            .glob_candidates_for_segments(segments)
            .into_iter()
            .flat_map(|candidate| {
                let qualified = self.module_qualified_candidate(scope, &candidate);
                match qualified == candidate {
                    true => vec![candidate],
                    false => vec![candidate, qualified],
                }
            })
            .collect()
    }

    fn reexport_glob_candidate_paths(
        &self,
        scope: &ModuleScope,
        segments: &[String],
    ) -> Vec<String> {
        scope
            .reexport_glob_candidates_for_segments(segments)
            .into_iter()
            .flat_map(|candidate| {
                let qualified = self.module_qualified_candidate(scope, &candidate);
                match qualified == candidate {
                    true => vec![candidate],
                    false => vec![candidate, qualified],
                }
            })
            .collect()
    }

    fn module_qualified_candidate(&self, scope: &ModuleScope, candidate: &str) -> String {
        let root = scope.path().segments().first().map(String::as_str);
        match candidate.split("::").next() == root {
            true => candidate.to_owned(),
            false => format!("{}::{candidate}", scope.path().segments().join("::")),
        }
    }

    fn local_first_segment(&self, scope: &ModuleScope, path: &str) -> Option<&TypeBinding> {
        path.split("::")
            .nth(scope.path().segments().len())
            .and_then(|name| self.local_name(scope, name))
    }

    fn local_name(&self, scope: &ModuleScope, name: &str) -> Option<&TypeBinding> {
        self.by_module
            .get(&scope.path().segments().join("::"))?
            .get(name)
    }

    fn insert_item(&mut self, module: &SourceModule, item: &syn::Item) {
        match item {
            syn::Item::Type(alias) => self.insert_alias(module, alias),
            _ => {
                if let Some(name) = Self::item_name(item) {
                    self.insert_source(module, name);
                }
            }
        }
    }

    fn insert_alias(&mut self, module: &SourceModule, alias: &syn::ItemType) {
        let path = self.insert_source(module, alias.ident.to_string());
        self.aliases.insert(
            path.clone(),
            TypeAlias {
                path,
                scope: module.scope().clone(),
                target: alias.ty.as_ref().clone(),
            },
        );
    }

    fn insert_source(&mut self, module: &SourceModule, name: String) -> String {
        let path = module.scope().path().qualified(&name);
        self.insert_path(path.clone());
        self.by_module
            .entry(module.scope().path().segments().join("::"))
            .or_default()
            .entry(name)
            .and_modify(TypeBinding::mark_ambiguous)
            .or_insert(TypeBinding::Unique(path.clone()));
        path
    }

    fn insert_scope(&mut self, module: &SourceModule) {
        self.scopes.insert(
            module.scope().path().segments().join("::"),
            module.scope().clone(),
        );
    }

    fn insert_path(&mut self, path: String) {
        self.by_path
            .entry(path.clone())
            .and_modify(TypeBinding::mark_ambiguous)
            .or_insert(TypeBinding::Unique(path));
    }

    fn item_name(item: &syn::Item) -> Option<String> {
        match item {
            syn::Item::Enum(item) => Some(item.ident.to_string()),
            syn::Item::Mod(item) => Some(item.ident.to_string()),
            syn::Item::Struct(item) => Some(item.ident.to_string()),
            syn::Item::Trait(item) => Some(item.ident.to_string()),
            syn::Item::TraitAlias(item) => Some(item.ident.to_string()),
            syn::Item::Type(item) => Some(item.ident.to_string()),
            syn::Item::Union(item) => Some(item.ident.to_string()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TypeBinding {
    Unique(String),
    Ambiguous,
}

impl TypeBinding {
    fn mark_ambiguous(&mut self) {
        *self = Self::Ambiguous;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TypeResolution {
    Known(String),
    Ambiguous,
    Unknown,
}
