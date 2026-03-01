use crate::model::{
    AsyncIteratorMethod, BuiltinId, CallbackTrait, Class, ClosureSignature as MClosureSignature, Constructor, ConstructorParam, CustomType, Enumeration, Function, Method, Module,
    Parameter, Primitive, Receiver, Record, RecordField, ReturnType, StreamMethod, StreamMode, TraitMethod, TraitMethodParam, Type as MType, Variant,
};
use indexmap::IndexMap;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
use syn::{Attribute, Fields, FnArg, ImplItem, Item, ItemEnum, ItemImpl, ItemStruct, ItemTrait, Type};
use walkdir::WalkDir;

mod compiler_type_resolution;

pub enum PendingKind {
    Record,
    Enum,
    Class,
    Callback,
}

pub enum TypeShape {
    Pending(PendingKind),
    Record {
        fields: Vec<RecordField>,
    },
    Enum {
        variants: Vec<Variant>,
        is_error: bool,
    },
    Class {
        constructors: Vec<Constructor>,
        methods: Vec<Method>,
        streams: Vec<StreamMethod>,
        async_iterators: Vec<AsyncIteratorMethod>,
    },
    Custom {
        repr: MType,
    },
}

pub struct TypeMeta {
    pub doc: Option<String>,
    pub shape: TypeShape,
}

#[derive(Default)]
pub struct TypeRegistry {
    types: IndexMap<String, TypeMeta>,
}

impl TypeRegistry {
    pub fn is_enum(&self, name: &str) -> bool {
        self.types
            .get(name)
            .is_some_and(|meta| matches!(meta.shape, TypeShape::Pending(PendingKind::Enum) | TypeShape::Enum { .. }))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }

    pub fn doc(&self, name: &str) -> Option<&str> {
        self.types.get(name).and_then(|meta| meta.doc.as_deref())
    }

    pub fn register(&mut self, name: String, meta: TypeMeta) {
        self.types.insert(name, meta);
    }

    pub fn fill(&mut self, name: &str, shape: TypeShape) {
        if let Some(meta) = self.types.get_mut(name) {
            meta.shape = shape;
        }
    }

    pub fn set_doc(&mut self, name: &str, doc: String) {
        if let Some(meta) = self.types.get_mut(name) {
            meta.doc = Some(doc);
        }
    }

    pub fn drain(self) -> impl Iterator<Item = (String, TypeMeta)> {
        self.types.into_iter()
    }

    pub fn classify_named_type(&self, name: &str) -> Option<MType> {
        let meta = self.types.get(name)?;
        Some(match &meta.shape {
            TypeShape::Pending(PendingKind::Record) | TypeShape::Record { .. } => MType::Record(name.to_string()),
            TypeShape::Pending(PendingKind::Enum) | TypeShape::Enum { .. } => MType::Enum(name.to_string()),
            TypeShape::Pending(PendingKind::Class) | TypeShape::Class { .. } => MType::Object(name.to_string()),
            TypeShape::Pending(PendingKind::Callback) => MType::BoxedTrait(name.to_string()),
            TypeShape::Custom { repr } => MType::Custom {
                name: name.to_string(),
                repr: Box::new(repr.clone()),
            },
        })
    }

    pub fn register_callback(&mut self, name: String) {
        self.types.insert(
            name,
            TypeMeta {
                shape: TypeShape::Pending(PendingKind::Callback),
                doc: None,
            },
        );
    }

    pub fn has_callback(&self, name: &str) -> bool {
        self.types.get(name).is_some_and(|m| matches!(m.shape, TypeShape::Pending(PendingKind::Callback)))
    }
}

#[derive(Debug, Clone, Default)]
struct AliasResolver {
    use_aliases: HashMap<String, Vec<String>>,
    type_aliases: HashMap<String, Vec<String>>,
}

impl AliasResolver {
    fn from_items(items: &[Item]) -> Self {
        let mut resolver = Self::default();

        items
            .iter()
            .filter_map(|item| match item {
                Item::Use(item_use) => Some(&item_use.tree),
                _ => None,
            })
            .for_each(|tree| resolver.collect_use_tree(Vec::new(), tree));

        items
            .iter()
            .filter_map(|item| match item {
                Item::Type(item_type) => Some(item_type),
                _ => None,
            })
            .filter_map(|item_type| {
                let target = match item_type.ty.as_ref() {
                    Type::Path(type_path) => Some(Self::segments_from_path(type_path)),
                    _ => None,
                }?;
                Some((item_type.ident.to_string(), target))
            })
            .for_each(|(alias, target)| {
                resolver.type_aliases.insert(alias, target);
            });

        resolver
    }

    fn with_global(mut self, global: &HashMap<String, Vec<String>>) -> Self {
        global
            .iter()
            .filter(|&(name, _target)| !self.type_aliases.contains_key(name))
            .map(|(name, target)| (name.clone(), target.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|(name, target)| {
                self.type_aliases.insert(name, target);
            });
        self
    }

    fn resolve_type_spelling<'a>(&self, spelling: &'a str) -> Cow<'a, str> {
        let stripped = spelling.trim().trim_start_matches("::");
        let parts: Vec<String> = stripped.split("::").filter(|p| !p.is_empty()).map(|p| p.to_string()).collect();

        let resolved = self.resolve_segments(parts);
        let resolved_spelling = resolved.iter().map(String::as_str).collect::<Vec<_>>().join("::");

        if resolved_spelling != stripped {
            Cow::Owned(resolved_spelling)
        } else {
            Cow::Borrowed(stripped)
        }
    }

    fn resolve_segments(&self, segments: Vec<String>) -> Vec<String> {
        let expanded = std::iter::successors(Some(segments), |current| {
            let first = current.first()?;
            let replacement = self.use_aliases.get(first)?;
            let next = replacement.iter().cloned().chain(current.iter().skip(1).cloned()).collect::<Vec<_>>();
            (next != *current).then_some(next)
        })
        .take(16)
        .last()
        .unwrap_or_default();

        expanded.last().and_then(|last| self.type_aliases.get(last)).cloned().unwrap_or(expanded)
    }

    fn segments_from_path(type_path: &syn::TypePath) -> Vec<String> {
        type_path.path.segments.iter().map(|seg| seg.ident.to_string()).collect()
    }

    fn collect_use_tree(&mut self, prefix: Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                let mut next_prefix = prefix;
                next_prefix.push(path.ident.to_string());
                self.collect_use_tree(next_prefix, &path.tree);
            }
            syn::UseTree::Name(name) => {
                let mut target = prefix;
                target.push(name.ident.to_string());
                self.use_aliases.insert(name.ident.to_string(), target);
            }
            syn::UseTree::Rename(rename) => {
                let mut target = prefix;
                target.push(rename.ident.to_string());
                self.use_aliases.insert(rename.rename.to_string(), target);
            }
            syn::UseTree::Group(group) => group.items.iter().for_each(|item| self.collect_use_tree(prefix.clone(), item)),
            syn::UseTree::Glob(_) => {}
        }
    }
}

pub struct SourceScanner {
    module_name: String,
    type_registry: TypeRegistry,
    functions: Vec<Function>,
    callback_traits: Vec<CallbackTrait>,
    alias_resolver: AliasResolver,
    global_aliases: HashMap<String, Vec<String>>,
    compiler_canonical_types: HashMap<String, String>,
}

impl SourceScanner {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            type_registry: TypeRegistry::default(),
            functions: Vec::new(),
            callback_traits: Vec::new(),
            alias_resolver: AliasResolver::default(),
            global_aliases: HashMap::new(),
            compiler_canonical_types: HashMap::new(),
        }
    }

    pub fn scan_directory(&mut self, crate_path: &Path, dir: &Path) -> Result<(), String> {
        let mut files: Vec<_> = WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .map(|e| e.path().to_path_buf())
            .collect();
        files.sort();

        self.global_aliases = Self::collect_global_aliases(&files)?;
        let compiler_targets = Self::collect_compiler_type_targets(&files, &self.global_aliases)?;
        self.compiler_canonical_types = compiler_type_resolution::resolve(crate_path, &self.module_name, compiler_targets)?;
        files.iter().try_for_each(|path| self.collect_type_names(path))?;
        files.iter().try_for_each(|path| self.collect_custom_types(path))?;
        files.iter().try_for_each(|path| self.scan_file(path))?;
        Ok(())
    }

    fn collect_compiler_type_targets(files: &[std::path::PathBuf], global_aliases: &HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
        let mut targets = Vec::<String>::new();
        let mut seen = HashSet::<String>::new();

        files.iter().try_for_each(|path| {
            let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            let syntax = syn::parse_file(&content).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

            let alias_resolver = AliasResolver::from_items(&syntax.items).with_global(global_aliases);
            syntax
                .items
                .iter()
                .for_each(|item| Self::collect_item_type_targets(item, &alias_resolver, &mut targets, &mut seen));

            Ok::<(), String>(())
        })?;

        Ok(targets)
    }

    fn collect_item_type_targets(item: &Item, alias_resolver: &AliasResolver, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        match item {
            Item::Struct(item_struct) => {
                let is_record = has_attribute(&item_struct.attrs, "ffi_record")
                    || has_attribute(&item_struct.attrs, "data")
                    || has_repr_c(&item_struct.attrs)
                    || (has_attribute(&item_struct.attrs, "derive") && has_ffi_type_derive(&item_struct.attrs));
                if is_record {
                    item_struct.fields.iter().for_each(|field| Self::collect_type_targets(&field.ty, alias_resolver, out, seen));
                }
            }
            Item::Enum(item_enum) => {
                let is_error = has_attribute(&item_enum.attrs, "error");
                let is_data_enum = has_repr_int(&item_enum.attrs) || has_attribute(&item_enum.attrs, "data") || is_error;
                if is_data_enum {
                    item_enum
                        .variants
                        .iter()
                        .flat_map(|variant| variant.fields.iter())
                        .for_each(|field| Self::collect_type_targets(&field.ty, alias_resolver, out, seen));
                }
            }
            Item::Impl(item_impl) => {
                let is_exported = has_attribute(&item_impl.attrs, "ffi_class") || has_attribute(&item_impl.attrs, "export");
                if is_exported {
                    item_impl
                        .items
                        .iter()
                        .filter_map(|impl_item| match impl_item {
                            ImplItem::Fn(method) => Some(method),
                            _ => None,
                        })
                        .filter(|method| matches!(method.vis, syn::Visibility::Public(_)))
                        .filter(|method| !method.attrs.iter().any(|a| a.path().is_ident("skip")))
                        .for_each(|method| {
                            method
                                .sig
                                .inputs
                                .iter()
                                .filter_map(|arg| match arg {
                                    FnArg::Typed(pat_type) => Some(pat_type.ty.as_ref()),
                                    FnArg::Receiver(_) => None,
                                })
                                .for_each(|ty| Self::collect_type_targets(ty, alias_resolver, out, seen));

                            match &method.sig.output {
                                syn::ReturnType::Default => {}
                                syn::ReturnType::Type(_, ty) => {
                                    Self::collect_type_targets(ty.as_ref(), alias_resolver, out, seen);
                                }
                            }
                        });
                }
            }
            Item::Trait(item_trait) => {
                let is_exported = has_attribute(&item_trait.attrs, "ffi_trait") || has_attribute(&item_trait.attrs, "export");
                if is_exported {
                    item_trait
                        .items
                        .iter()
                        .filter_map(|trait_item| match trait_item {
                            syn::TraitItem::Fn(method) => Some(method),
                            _ => None,
                        })
                        .for_each(|method| {
                            method
                                .sig
                                .inputs
                                .iter()
                                .filter_map(|arg| match arg {
                                    FnArg::Typed(pat_type) => Some(pat_type.ty.as_ref()),
                                    FnArg::Receiver(_) => None,
                                })
                                .for_each(|ty| Self::collect_type_targets(ty, alias_resolver, out, seen));

                            match &method.sig.output {
                                syn::ReturnType::Default => {}
                                syn::ReturnType::Type(_, ty) => {
                                    Self::collect_type_targets(ty.as_ref(), alias_resolver, out, seen);
                                }
                            }
                        });
                }
            }
            Item::Fn(item_fn) => {
                let is_exported = has_attribute(&item_fn.attrs, "ffi_export") || has_attribute(&item_fn.attrs, "export");
                if is_exported {
                    item_fn
                        .sig
                        .inputs
                        .iter()
                        .filter_map(|arg| match arg {
                            FnArg::Typed(pat_type) => Some(pat_type.ty.as_ref()),
                            FnArg::Receiver(_) => None,
                        })
                        .for_each(|ty| Self::collect_type_targets(ty, alias_resolver, out, seen));

                    match &item_fn.sig.output {
                        syn::ReturnType::Default => {}
                        syn::ReturnType::Type(_, ty) => {
                            Self::collect_type_targets(ty.as_ref(), alias_resolver, out, seen);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_type_targets(ty: &Type, alias_resolver: &AliasResolver, out: &mut Vec<String>, seen: &mut HashSet<String>) {
        match ty {
            Type::Path(type_path) => {
                let all_segments_plain = type_path.path.segments.iter().all(|seg| matches!(seg.arguments, syn::PathArguments::None));

                type_path
                    .path
                    .segments
                    .iter()
                    .filter_map(|seg| match &seg.arguments {
                        syn::PathArguments::AngleBracketed(args) => Some(args),
                        _ => None,
                    })
                    .flat_map(|args| args.args.iter())
                    .filter_map(|arg| match arg {
                        syn::GenericArgument::Type(inner_ty) => Some(inner_ty),
                        _ => None,
                    })
                    .for_each(|inner_ty| Self::collect_type_targets(inner_ty, alias_resolver, out, seen));

                type_path
                    .path
                    .segments
                    .iter()
                    .filter_map(|seg| match &seg.arguments {
                        syn::PathArguments::Parenthesized(args) => Some(args),
                        _ => None,
                    })
                    .for_each(|args| {
                        args.inputs.iter().for_each(|inner_ty| Self::collect_type_targets(inner_ty, alias_resolver, out, seen));
                        match &args.output {
                            syn::ReturnType::Default => {}
                            syn::ReturnType::Type(_, out_ty) => {
                                Self::collect_type_targets(out_ty.as_ref(), alias_resolver, out, seen);
                            }
                        }
                    });

                if all_segments_plain {
                    let path_str = type_path.path.segments.iter().map(|seg| seg.ident.to_string()).collect::<Vec<_>>().join("::");

                    let resolved = alias_resolver.resolve_type_spelling(&path_str).into_owned();
                    if resolved.starts_with("crate::") && seen.insert(resolved.clone()) {
                        out.push(resolved);
                    }
                }
            }
            Type::Reference(type_ref) => {
                Self::collect_type_targets(type_ref.elem.as_ref(), alias_resolver, out, seen);
            }
            Type::Slice(slice) => {
                Self::collect_type_targets(slice.elem.as_ref(), alias_resolver, out, seen);
            }
            Type::Array(array) => {
                Self::collect_type_targets(array.elem.as_ref(), alias_resolver, out, seen);
            }
            Type::Tuple(tuple) => tuple.elems.iter().for_each(|inner_ty| Self::collect_type_targets(inner_ty, alias_resolver, out, seen)),
            Type::Group(group) => {
                Self::collect_type_targets(group.elem.as_ref(), alias_resolver, out, seen);
            }
            Type::Paren(paren) => {
                Self::collect_type_targets(paren.elem.as_ref(), alias_resolver, out, seen);
            }
            _ => {}
        }
    }

    fn collect_global_aliases(files: &[std::path::PathBuf]) -> Result<HashMap<String, Vec<String>>, String> {
        let mut aliases = HashMap::<String, Vec<String>>::new();
        let mut conflicts = HashSet::<String>::new();

        files.iter().try_for_each(|path| {
            let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            let syntax = syn::parse_file(&content).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

            let local = AliasResolver::from_items(&syntax.items);
            syntax
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Type(item_type) => Some(item_type),
                    _ => None,
                })
                .filter_map(|item_type| {
                    let target_path = match item_type.ty.as_ref() {
                        Type::Path(type_path) => Some(AliasResolver::segments_from_path(type_path).join("::")),
                        _ => None,
                    }?;

                    let resolved = local.resolve_type_spelling(&target_path).into_owned();
                    let segments = resolved.split("::").filter(|p| !p.is_empty()).map(|p| p.to_string()).collect::<Vec<_>>();
                    Some((item_type.ident.to_string(), segments))
                })
                .for_each(|(alias_name, target)| match aliases.get(&alias_name) {
                    None => {
                        aliases.insert(alias_name, target);
                    }
                    Some(existing) if *existing == target => {}
                    Some(_) => {
                        conflicts.insert(alias_name);
                    }
                });

            Ok::<(), String>(())
        })?;

        conflicts.iter().for_each(|name| {
            aliases.remove(name);
        });

        Ok(aliases)
    }

    fn collect_custom_types(&mut self, path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let syntax = syn::parse_file(&content).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        let alias_resolver = AliasResolver::from_items(&syntax.items).with_global(&self.global_aliases);
        syntax
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Macro(item_macro) if item_macro.mac.path.segments.last().is_some_and(|segment| segment.ident == "custom_type") => Some(item_macro),
                _ => None,
            })
            .try_for_each(|item_macro| self.collect_custom_type_macro(item_macro, &alias_resolver))?;
        syntax
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Impl(item_impl) if has_attribute(&item_impl.attrs, "custom_ffi") => Some(item_impl),
                _ => None,
            })
            .try_for_each(|item_impl| self.collect_custom_type(item_impl, &alias_resolver))
    }

    fn collect_custom_type_macro(&mut self, item_macro: &syn::ItemMacro, alias_resolver: &AliasResolver) -> Result<(), String> {
        let spec: CustomTypeMacroSpec = syn::parse2(item_macro.mac.tokens.clone()).map_err(|e| format!("custom_type!: failed to parse: {e}"))?;

        let name = spec.name.to_string();
        if self.type_registry.contains(&name) {
            return Err(format!("custom_type!: `{}` conflicts with an existing record/enum/class name", name));
        }

        let repr_syn_type = &spec.repr;
        let repr = rust_type_to_ffi_type(repr_syn_type, &self.type_registry, alias_resolver, &self.compiler_canonical_types, None)
            .ok_or_else(|| format!("custom_type!: `{}` has an unsupported repr type: {}", name, quote::quote!(#repr_syn_type)))?;

        self.type_registry.register(
            name.clone(),
            TypeMeta {
                doc: extract_doc_string(&item_macro.attrs),
                shape: TypeShape::Custom { repr: repr.clone() },
            },
        );
        Ok(())
    }

    fn collect_custom_type(&mut self, item_impl: &ItemImpl, alias_resolver: &AliasResolver) -> Result<(), String> {
        let Some(name) = impl_self_type_ident(item_impl) else {
            return Err("custom_ffi: unsupported self type".to_string());
        };

        if self.type_registry.contains(&name) {
            return Err(format!("custom_ffi: `{}` conflicts with an existing record/enum/class name", name));
        }

        let repr_syn_type = item_impl
            .items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Type(assoc) if assoc.ident == "FfiRepr" => Some(&assoc.ty),
                _ => None,
            })
            .next()
            .ok_or_else(|| format!("custom_ffi: `{}` is missing `type FfiRepr = ...;`", name))?;

        let repr = rust_type_to_ffi_type(repr_syn_type, &self.type_registry, alias_resolver, &self.compiler_canonical_types, None)
            .ok_or_else(|| format!("custom_ffi: `{}` has an unsupported FfiRepr type: {}", name, quote::quote!(#repr_syn_type)))?;

        self.type_registry.register(
            name.clone(),
            TypeMeta {
                doc: extract_doc_string(&item_impl.attrs),
                shape: TypeShape::Custom { repr: repr.clone() },
            },
        );

        Ok(())
    }

    fn collect_type_names(&mut self, path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let syntax = syn::parse_file(&content).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        for item in &syntax.items {
            match item {
                Item::Struct(item_struct) => {
                    if has_attribute(&item_struct.attrs, "ffi_record")
                        || has_attribute(&item_struct.attrs, "data")
                        || has_repr_c(&item_struct.attrs)
                        || (has_attribute(&item_struct.attrs, "derive") && has_ffi_type_derive(&item_struct.attrs))
                    {
                        self.type_registry.register(
                            item_struct.ident.to_string(),
                            TypeMeta {
                                doc: extract_doc_string(&item_struct.attrs),
                                shape: TypeShape::Pending(PendingKind::Record),
                            },
                        );
                    }
                }
                Item::Enum(item_enum) => {
                    if has_repr_int(&item_enum.attrs) || has_attribute(&item_enum.attrs, "data") || has_attribute(&item_enum.attrs, "error") {
                        self.type_registry.register(
                            item_enum.ident.to_string(),
                            TypeMeta {
                                doc: extract_doc_string(&item_enum.attrs),
                                shape: TypeShape::Pending(PendingKind::Enum),
                            },
                        );
                    }
                }
                Item::Impl(item_impl) => {
                    if (has_attribute(&item_impl.attrs, "ffi_class") || has_attribute(&item_impl.attrs, "export"))
                        && let Type::Path(type_path) = item_impl.self_ty.as_ref()
                        && let Some(seg) = type_path.path.segments.last()
                    {
                        self.type_registry.register(
                            seg.ident.to_string(),
                            TypeMeta {
                                doc: None,
                                shape: TypeShape::Pending(PendingKind::Class),
                            },
                        );
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn scan_file(&mut self, path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let syntax = syn::parse_file(&content).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        self.alias_resolver = AliasResolver::from_items(&syntax.items).with_global(&self.global_aliases);
        syntax.items.iter().for_each(|item| self.process_item(item));

        Ok(())
    }

    fn process_item(&mut self, item: &Item) {
        match item {
            Item::Struct(item_struct) => {
                if let Some(doc) = extract_doc_string(&item_struct.attrs) {
                    self.type_registry.set_doc(&item_struct.ident.to_string(), doc);
                }
                if has_attribute(&item_struct.attrs, "ffi_record")
                    || has_attribute(&item_struct.attrs, "data")
                    || has_repr_c(&item_struct.attrs)
                    || (has_attribute(&item_struct.attrs, "derive") && has_ffi_type_derive(&item_struct.attrs))
                {
                    self.process_record(item_struct);
                }
            }
            Item::Impl(item_impl) => {
                if has_attribute(&item_impl.attrs, "ffi_class") || has_attribute(&item_impl.attrs, "export") {
                    self.process_class(item_impl);
                }
            }
            Item::Trait(item_trait) => {
                if has_attribute(&item_trait.attrs, "ffi_trait") || has_attribute(&item_trait.attrs, "export") {
                    self.process_callback_trait(item_trait);
                }
            }
            Item::Fn(item_fn) => {
                if has_attribute(&item_fn.attrs, "ffi_export") || has_attribute(&item_fn.attrs, "export") {
                    self.process_function(item_fn);
                }
            }
            Item::Enum(item_enum) => {
                let is_error = has_attribute(&item_enum.attrs, "error");
                if has_repr_int(&item_enum.attrs) || has_attribute(&item_enum.attrs, "data") || is_error {
                    self.process_enum(item_enum, is_error);
                }
            }
            _ => {}
        }
    }

    fn resolve_typed_params(&self, inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>, self_type: Option<&str>) -> Option<Vec<(String, MType)>> {
        let typed: Vec<_> = inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                _ => None,
            })
            .collect();

        let resolved: Vec<(String, MType)> = typed
            .iter()
            .filter_map(|pat_type| {
                let name = match &*pat_type.pat {
                    syn::Pat::Ident(ident) => ident.ident.to_string(),
                    _ => return None,
                };
                let ty = rust_type_to_ffi_type(&pat_type.ty, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types, self_type)?;
                Some((name, ty))
            })
            .collect();

        (resolved.len() == typed.len()).then_some(resolved)
    }

    fn resolve_output(&self, output: &syn::ReturnType, self_type: Option<&str>) -> Option<MType> {
        match output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => rust_type_to_ffi_type(ty, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types, self_type),
        }
    }

    fn extract_receiver(sig: &syn::Signature) -> Receiver {
        sig.inputs
            .first()
            .and_then(|arg| match arg {
                syn::FnArg::Receiver(r) => Some(if r.mutability.is_some() { Receiver::RefMut } else { Receiver::Ref }),
                _ => None,
            })
            .unwrap_or(Receiver::None)
    }

    fn process_record(&mut self, item_struct: &ItemStruct) {
        let name = item_struct.ident.to_string();
        let fields = match &item_struct.fields {
            Fields::Named(named) => named
                .named
                .iter()
                .filter_map(|f| {
                    let field_name = f.ident.as_ref()?.to_string();
                    let field_type = rust_type_to_ffi_type(&f.ty, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types, None)?;
                    let mut record_field = RecordField::new(&field_name, field_type);
                    if let Some(doc) = extract_doc_string(&f.attrs) {
                        record_field = record_field.with_doc(doc);
                    }
                    if let Some(default) = extract_default_value(&f.attrs) {
                        record_field = record_field.with_default(default);
                    }
                    Some(record_field)
                })
                .collect(),
            _ => Vec::new(),
        };

        self.type_registry.fill(&name, TypeShape::Record { fields });
    }

    fn process_enum(&mut self, item_enum: &ItemEnum, is_error: bool) {
        let name = item_enum.ident.to_string();
        let mut next_discriminant: i64 = 0;

        let variants = item_enum
            .variants
            .iter()
            .map(|v| {
                let variant_name = v.ident.to_string();
                let discriminant = v.discriminant.as_ref().and_then(|(_, expr)| parse_discriminant_expr(expr)).unwrap_or(next_discriminant);
                next_discriminant = discriminant + 1;

                let fields: Vec<RecordField> = match &v.fields {
                    Fields::Named(named) => named
                        .named
                        .iter()
                        .filter_map(|f| {
                            let field_name = f.ident.as_ref()?.to_string();
                            let field_type = rust_type_to_ffi_type(&f.ty, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types, None)?;
                            let mut record_field = RecordField::new(&field_name, field_type);
                            if let Some(doc) = extract_doc_string(&f.attrs) {
                                record_field = record_field.with_doc(doc);
                            }
                            Some(record_field)
                        })
                        .collect(),
                    Fields::Unnamed(unnamed) => unnamed
                        .unnamed
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            let field_type = rust_type_to_ffi_type(&f.ty, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types, None)?;
                            Some(RecordField::new(format!("value_{i}"), field_type))
                        })
                        .collect(),
                    Fields::Unit => Vec::new(),
                };

                let variant = Variant::new(&variant_name).with_discriminant(discriminant).maybe_doc(extract_doc_string(&v.attrs));
                fields.into_iter().fold(variant, |v, field| v.with_field(field))
            })
            .collect();

        self.type_registry.fill(&name, TypeShape::Enum { variants, is_error });
    }

    fn process_function(&mut self, item_fn: &syn::ItemFn) {
        let sig = &item_fn.sig;
        let Some(params) = self.resolve_typed_params(&sig.inputs, None) else {
            return;
        };
        let output = self.resolve_output(&sig.output, None);
        if matches!(sig.output, syn::ReturnType::Type(..)) && output.is_none() {
            return;
        }

        let function = params
            .into_iter()
            .fold(Function::new(sig.ident.to_string()), |f, (name, ty)| f.with_param(Parameter::new(&name, ty)))
            .maybe_doc(extract_doc_string(&item_fn.attrs))
            .maybe_return(output.map(ReturnType::from_output))
            .maybe_async(sig.asyncness.is_some());

        self.functions.push(function);
    }

    fn process_callback_trait(&mut self, item_trait: &ItemTrait) {
        let name = item_trait.ident.to_string();

        let callback = item_trait
            .items
            .iter()
            .filter_map(|item| match item {
                syn::TraitItem::Fn(method) => self.build_trait_method(method),
                _ => None,
            })
            .fold(CallbackTrait::new(&name), |ct, m| ct.with_method(m))
            .maybe_doc(extract_doc_string(&item_trait.attrs));

        self.type_registry.register_callback(name);
        self.callback_traits.push(callback);
    }

    fn build_trait_method(&self, method: &syn::TraitItemFn) -> Option<TraitMethod> {
        let sig = &method.sig;
        let params = self.resolve_typed_params(&sig.inputs, None)?;
        let output = self.resolve_output(&sig.output, None);

        Some(
            params
                .into_iter()
                .fold(TraitMethod::new(sig.ident.to_string()), |tm, (name, ty)| tm.with_param(TraitMethodParam::new(&name, ty)))
                .maybe_doc(extract_doc_string(&method.attrs))
                .maybe_return(output.map(ReturnType::from_output))
                .maybe_async(sig.asyncness.is_some()),
        )
    }

    fn process_class(&mut self, item_impl: &ItemImpl) {
        let Some(class_name) = impl_self_type_ident(item_impl) else {
            return;
        };

        let mut constructors = Vec::new();
        let mut methods = Vec::new();
        let mut streams = Vec::new();
        let mut async_iterators = Vec::new();

        item_impl
            .items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .filter(|method| matches!(method.vis, syn::Visibility::Public(_)))
            .filter(|method| !has_attribute(&method.attrs, "skip"))
            .for_each(|method| {
                if has_attribute(&method.attrs, "ffi_stream") {
                    if let Some(stream) = self.build_stream(method) {
                        streams.push(stream);
                    }
                    return;
                }

                if has_attribute(&method.attrs, "ffi_async_iter") {
                    if let Some(iter) = self.build_async_iterator(method) {
                        async_iterators.push(iter);
                    }
                    return;
                }

                if self.is_constructor(method, &class_name) {
                    if let Some(ctor) = self.build_constructor(method, &class_name) {
                        constructors.push(ctor);
                    }
                    return;
                }

                if let Some(built_method) = self.build_method(method, &class_name) {
                    methods.push(built_method);
                }
            });

        self.type_registry.fill(
            &class_name,
            TypeShape::Class {
                constructors,
                methods,
                streams,
                async_iterators,
            },
        );
    }

    fn build_method(&self, method: &syn::ImplItemFn, self_type_name: &str) -> Option<Method> {
        let sig = &method.sig;
        let receiver = Self::extract_receiver(sig);
        let params = self.resolve_typed_params(&sig.inputs, Some(self_type_name))?;
        let output = self.resolve_output(&sig.output, Some(self_type_name));

        Some(
            params
                .into_iter()
                .fold(Method::new(sig.ident.to_string(), receiver), |m, (name, ty)| m.with_param(Parameter::new(&name, ty)))
                .maybe_doc(extract_doc_string(&method.attrs))
                .maybe_return(output.map(ReturnType::from_output))
                .maybe_async(sig.asyncness.is_some()),
        )
    }

    fn build_stream(&self, method: &syn::ImplItemFn) -> Option<StreamMethod> {
        let name = method.sig.ident.to_string();

        let (item_type, mode) = extract_stream_attr(&method.attrs, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types)?;

        Some(StreamMethod::new(&name, item_type).with_mode(mode).maybe_doc(extract_doc_string(&method.attrs)))
    }

    fn build_async_iterator(&self, method: &syn::ImplItemFn) -> Option<AsyncIteratorMethod> {
        let name = method.sig.ident.to_string();
        let item_type = extract_async_iter_attr(&method.attrs, &self.type_registry, &self.alias_resolver, &self.compiler_canonical_types)?;
        Some(AsyncIteratorMethod::new(&name, item_type).maybe_doc(extract_doc_string(&method.attrs)))
    }

    fn is_constructor(&self, method: &syn::ImplItemFn, class_name: &str) -> bool {
        let has_self_receiver = method.sig.inputs.iter().any(|arg| matches!(arg, syn::FnArg::Receiver(_)));
        if has_self_receiver {
            return false;
        }

        match &method.sig.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, ty) => return_type_is_self(ty.as_ref(), class_name) || return_type_is_result_self(ty.as_ref(), class_name),
        }
    }

    fn build_constructor(&self, method: &syn::ImplItemFn, self_type_name: &str) -> Option<Constructor> {
        let sig = &method.sig;
        let is_fallible = match &sig.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, ty) => return_type_is_result_self(ty.as_ref(), self_type_name),
        };
        let params = self.resolve_typed_params(&sig.inputs, Some(self_type_name))?;

        Some(
            params
                .into_iter()
                .fold(Constructor::new().with_name(sig.ident.to_string()).with_fallible(is_fallible), |c, (name, ty)| {
                    c.with_param(ConstructorParam::new(&name, ty))
                })
                .maybe_doc(extract_doc_string(&method.attrs)),
        )
    }

    pub fn into_module(self) -> Module {
        let mut module = Module::new(&self.module_name);

        for (name, entry) in self.type_registry.drain() {
            match entry.shape {
                TypeShape::Record { fields } => {
                    let record = fields.into_iter().fold(Record::new(&name), |r, f| r.with_field(f)).maybe_doc(entry.doc);
                    module = module.with_record(record);
                }
                TypeShape::Enum { variants, is_error } => {
                    let mut enumeration = variants.into_iter().fold(Enumeration::new(&name), |e, v| e.with_variant(v)).maybe_doc(entry.doc);
                    if is_error {
                        enumeration = enumeration.as_error();
                    }
                    module = module.with_enum(enumeration);
                }
                TypeShape::Class {
                    constructors,
                    methods,
                    streams,
                    async_iterators,
                } => {
                    let class = constructors.into_iter().fold(Class::new(&name), |c, ctor| c.with_constructor(ctor));
                    let class = methods.into_iter().fold(class, |c, m| c.with_method(m));
                    let class = streams.into_iter().fold(class, |c, s| c.with_stream(s));
                    let class = async_iterators.into_iter().fold(class, |c, a| c.with_async_iterator(a)).maybe_doc(entry.doc);
                    module = module.with_class(class);
                }
                TypeShape::Custom { repr } => {
                    module = module.with_custom_type(CustomType::new(name, repr));
                }
                TypeShape::Pending(_) => {}
            }
        }

        for function in self.functions {
            module = module.with_function(function);
        }
        for callback in self.callback_traits {
            module = module.with_callback_trait(callback);
        }

        module.collect_derived_types();
        module
    }
}

struct CustomTypeMacroSpec {
    name: syn::Ident,
    repr: syn::Type,
}

impl syn::parse::Parse for CustomTypeMacroSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _visibility: syn::Visibility = input.parse()?;
        let name: syn::Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;

        let mut repr: Option<syn::Type> = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            match key.to_string().as_str() {
                "remote" => {
                    let _: syn::Type = input.parse()?;
                }
                "repr" => {
                    repr = Some(input.parse()?);
                }
                "error" => {
                    let _: syn::Type = input.parse()?;
                }
                "into_ffi" | "try_from_ffi" => {
                    let _: syn::Expr = input.parse()?;
                }
                _ => {
                    let _: syn::Expr = input.parse()?;
                }
            }

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }

        let repr = repr.ok_or_else(|| input.error("custom_type!: missing `repr = ...`"))?;

        Ok(Self { name, repr })
    }
}

fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident(name) || attr.path().segments.last().is_some_and(|segment| segment.ident == name))
}

fn extract_default_value(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        let path = attr.path();
        let is_boltffi_default = path.segments.len() == 2 && path.segments[0].ident == "boltffi" && path.segments[1].ident == "default";
        if !is_boltffi_default {
            return None;
        }
        let tokens = match &attr.meta {
            syn::Meta::List(list) => list.tokens.to_string(),
            _ => return None,
        };
        Some(tokens.trim().to_string())
    })
}

fn extract_doc_string(attrs: &[Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            syn::Meta::NameValue(nv) => {
                if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                    Some(s.value())
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    if lines.is_empty() {
        return None;
    }

    let joined = lines.iter().map(|line| line.strip_prefix(' ').unwrap_or(line)).collect::<Vec<_>>().join("\n");

    let trimmed = joined.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn return_type_is_self(ty: &Type, class_name: &str) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path.path.segments.last().is_some_and(|segment| segment.ident == "Self" || segment.ident == class_name)
}

fn return_type_is_result_self(ty: &Type, class_name: &str) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    let Some(result_segment) = type_path.path.segments.last().filter(|segment| segment.ident == "Result") else {
        return false;
    };
    let syn::PathArguments::AngleBracketed(args) = &result_segment.arguments else {
        return false;
    };
    let Some(syn::GenericArgument::Type(Type::Path(ok_ty))) = args.args.first() else {
        return false;
    };
    ok_ty.path.segments.last().is_some_and(|segment| segment.ident == "Self" || segment.ident == class_name)
}

fn impl_self_type_ident(item_impl: &ItemImpl) -> Option<String> {
    match item_impl.self_ty.as_ref() {
        Type::Path(type_path) => type_path.path.segments.last().map(|segment| segment.ident.to_string()),
        Type::Group(group) => match group.elem.as_ref() {
            Type::Path(type_path) => type_path.path.segments.last().map(|segment| segment.ident.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn has_repr_c(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("repr") {
            return false;
        }
        let Ok(meta) = attr.meta.require_list() else {
            return false;
        };
        meta.tokens.to_string().contains('C')
    })
}

fn has_repr_int(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("repr") {
            return false;
        }
        let Ok(meta) = attr.meta.require_list() else {
            return false;
        };
        let tokens = meta.tokens.to_string();
        ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"].iter().any(|ty| tokens.contains(ty))
    })
}

fn parse_discriminant_expr(expr: &syn::Expr) -> Option<i64> {
    match expr {
        syn::Expr::Lit(lit) => {
            if let syn::Lit::Int(int_lit) = &lit.lit {
                int_lit.base10_parse().ok()
            } else {
                None
            }
        }
        syn::Expr::Unary(unary) => {
            if matches!(unary.op, syn::UnOp::Neg(_)) {
                parse_discriminant_expr(&unary.expr).map(|v| -v)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn has_ffi_type_derive(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let Ok(meta) = attr.meta.require_list() else {
            return false;
        };
        meta.tokens.to_string().contains("FfiType")
    })
}

fn extract_stream_attr(
    attrs: &[Attribute],
    registry: &TypeRegistry,
    alias_resolver: &AliasResolver,
    compiler_canonical_types: &HashMap<String, String>,
) -> Option<(MType, StreamMode)> {
    for attr in attrs {
        if !attr.path().is_ident("ffi_stream") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        let tokens = meta.tokens.to_string();
        let item_type = extract_item_type(&tokens, registry, alias_resolver, compiler_canonical_types)?;
        let mode = extract_stream_mode(&tokens);

        return Some((item_type, mode));
    }
    None
}

fn extract_async_iter_attr(attrs: &[Attribute], registry: &TypeRegistry, alias_resolver: &AliasResolver, compiler_canonical_types: &HashMap<String, String>) -> Option<MType> {
    for attr in attrs {
        if !attr.path().is_ident("ffi_async_iter") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        let tokens = meta.tokens.to_string();
        return extract_item_type(&tokens, registry, alias_resolver, compiler_canonical_types);
    }
    None
}

fn extract_item_type(tokens: &str, registry: &TypeRegistry, alias_resolver: &AliasResolver, compiler_canonical_types: &HashMap<String, String>) -> Option<MType> {
    let item_start = tokens.find("item")? + 4;
    let rest = &tokens[item_start..];
    let eq_pos = rest.find('=')?;
    let after_eq = rest[eq_pos + 1..].trim();

    let type_end = after_eq.find(',').unwrap_or(after_eq.find(')').unwrap_or(after_eq.len()));
    let type_str = after_eq[..type_end].trim();

    string_to_ffi_type(type_str, registry, alias_resolver, compiler_canonical_types)
}

fn extract_stream_mode(tokens: &str) -> StreamMode {
    if tokens.contains("mode") {
        if tokens.contains("\"batch\"") {
            StreamMode::Batch
        } else if tokens.contains("\"callback\"") {
            StreamMode::Callback
        } else {
            StreamMode::Async
        }
    } else {
        StreamMode::Async
    }
}

fn rust_type_to_ffi_type(
    ty: &Type,
    registry: &TypeRegistry,
    alias_resolver: &AliasResolver,
    compiler_canonical_types: &HashMap<String, String>,
    self_type_name: Option<&str>,
) -> Option<MType> {
    match ty {
        Type::Path(type_path) => {
            let last_segment = type_path.path.segments.last()?;
            let ident = last_segment.ident.to_string();

            if ident == "Self" {
                return self_type_name.map(|name| MType::Object(name.to_string()));
            }

            if ident == "Box"
                && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                && let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) = args.args.first()
                && let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
                && let Some(seg) = trait_bound.path.segments.last()
            {
                return Some(MType::BoxedTrait(seg.ident.to_string()));
            }

            if ident == "Arc"
                && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                && let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) = args.args.first()
                && let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
                && let Some(seg) = trait_bound.path.segments.last()
            {
                return Some(MType::BoxedTrait(seg.ident.to_string()));
            }

            if (ident == "Arc" || ident == "Box")
                && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
            {
                return rust_type_to_ffi_type(inner_ty, registry, alias_resolver, compiler_canonical_types, self_type_name);
            }

            if ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    let inner = rust_type_to_ffi_type(inner_ty, registry, alias_resolver, compiler_canonical_types, self_type_name)?;
                    return Some(MType::Vec(Box::new(inner)));
                }
                return None;
            }

            if ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    let inner = rust_type_to_ffi_type(inner_ty, registry, alias_resolver, compiler_canonical_types, self_type_name)?;
                    return Some(MType::Option(Box::new(inner)));
                }
                return None;
            }

            if ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    let mut args_iter = args.args.iter();
                    if let Some(syn::GenericArgument::Type(ok_ty)) = args_iter.next() {
                        let ok = rust_type_to_ffi_type(ok_ty, registry, alias_resolver, compiler_canonical_types, self_type_name)?;
                        let err = args_iter
                            .next()
                            .and_then(|arg| {
                                if let syn::GenericArgument::Type(err_ty) = arg {
                                    rust_type_to_ffi_type(err_ty, registry, alias_resolver, compiler_canonical_types, self_type_name)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(MType::String);
                        return Some(MType::Result {
                            ok: Box::new(ok),
                            err: Box::new(err),
                        });
                    }
                }
                return None;
            }

            let path_str = type_path.path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::");

            string_to_ffi_type(&path_str, registry, alias_resolver, compiler_canonical_types)
        }
        Type::Reference(type_ref) => {
            if let Type::Path(inner) = &*type_ref.elem {
                let ident = inner.path.segments.last()?.ident.to_string();
                if ident == "str" {
                    return Some(MType::String);
                }
            }
            if let Type::Slice(slice) = &*type_ref.elem {
                let inner = rust_type_to_ffi_type(&slice.elem, registry, alias_resolver, compiler_canonical_types, self_type_name)?;
                return if type_ref.mutability.is_some() {
                    Some(MType::MutSlice(Box::new(inner)))
                } else {
                    Some(MType::Slice(Box::new(inner)))
                };
            }
            rust_type_to_ffi_type(&type_ref.elem, registry, alias_resolver, compiler_canonical_types, self_type_name)
        }
        Type::Slice(slice) => {
            let inner = rust_type_to_ffi_type(&slice.elem, registry, alias_resolver, compiler_canonical_types, self_type_name)?;
            Some(MType::Slice(Box::new(inner)))
        }
        Type::ImplTrait(impl_trait) => {
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    let trait_name = trait_bound.path.segments.last().map(|s| s.ident.to_string())?;

                    if (trait_name == "FnMut" || trait_name == "Fn" || trait_name == "FnOnce")
                        && let syn::PathArguments::Parenthesized(args) = &trait_bound.path.segments.last()?.arguments
                    {
                        let params: Vec<MType> = args
                            .inputs
                            .iter()
                            .filter_map(|t| rust_type_to_ffi_type(t, registry, alias_resolver, compiler_canonical_types, self_type_name))
                            .collect();

                        let returns = match &args.output {
                            syn::ReturnType::Default => MType::Void,
                            syn::ReturnType::Type(_, ty) => rust_type_to_ffi_type(ty, registry, alias_resolver, compiler_canonical_types, self_type_name).unwrap_or(MType::Void),
                        };

                        return Some(MType::Closure(MClosureSignature::new(params, returns)));
                    }

                    if registry.has_callback(&trait_name) {
                        return Some(MType::BoxedTrait(trait_name));
                    }
                }
            }
            None
        }
        Type::TraitObject(trait_obj) => {
            if let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
                && let Some(seg) = trait_bound.path.segments.last()
            {
                return Some(MType::BoxedTrait(seg.ident.to_string()));
            }
            None
        }
        _ => None,
    }
}

fn string_to_ffi_type(s: &str, registry: &TypeRegistry, alias_resolver: &AliasResolver, compiler_canonical_types: &HashMap<String, String>) -> Option<MType> {
    let trimmed = s.trim();
    match trimmed {
        "i8" => Some(MType::Primitive(Primitive::I8)),
        "i16" => Some(MType::Primitive(Primitive::I16)),
        "i32" => Some(MType::Primitive(Primitive::I32)),
        "i64" => Some(MType::Primitive(Primitive::I64)),
        "u8" => Some(MType::Primitive(Primitive::U8)),
        "u16" => Some(MType::Primitive(Primitive::U16)),
        "u32" => Some(MType::Primitive(Primitive::U32)),
        "u64" => Some(MType::Primitive(Primitive::U64)),
        "f32" => Some(MType::Primitive(Primitive::F32)),
        "f64" => Some(MType::Primitive(Primitive::F64)),
        "bool" => Some(MType::Primitive(Primitive::Bool)),
        "usize" => Some(MType::Primitive(Primitive::Usize)),
        "isize" => Some(MType::Primitive(Primitive::Isize)),
        "String" | "str" | "std::string::String" | "alloc::string::String" => Some(MType::String),
        s if s.starts_with("Vec<") => {
            let inner = &s[4..s.len() - 1];
            Some(MType::Vec(Box::new(string_to_ffi_type(inner, registry, alias_resolver, compiler_canonical_types)?)))
        }
        s if s.starts_with("Option<") => {
            let inner = &s[7..s.len() - 1];
            Some(MType::Option(Box::new(string_to_ffi_type(inner, registry, alias_resolver, compiler_canonical_types)?)))
        }
        s if s.starts_with("Result<") => {
            let inner = &s[7..s.len() - 1];
            let parts: Vec<&str> = inner.splitn(2, ',').map(|p| p.trim()).collect();
            let ok = string_to_ffi_type(parts.first()?, registry, alias_resolver, compiler_canonical_types)?;
            let err = parts
                .get(1)
                .and_then(|e| string_to_ffi_type(e, registry, alias_resolver, compiler_canonical_types))
                .unwrap_or(MType::String);
            Some(MType::Result {
                ok: Box::new(ok),
                err: Box::new(err),
            })
        }
        s => {
            if let Some(ty) = registry.classify_named_type(s) {
                return Some(ty);
            }

            let resolved = alias_resolver.resolve_type_spelling(s);
            let canonical = compiler_canonical_types.get(resolved.as_ref()).map(String::as_str).unwrap_or(resolved.as_ref());

            BuiltinId::from_rust_path(canonical)
                .map(MType::Builtin)
                .or_else(|| registry.classify_named_type(canonical))
                .or_else(|| canonical.rsplit("::").next().and_then(|name| registry.classify_named_type(name)))
        }
    }
}

pub fn scan_crate(crate_path: &Path, module_name: &str) -> Result<Module, String> {
    let src_path = crate_path.join("src");
    let mut scanner = SourceScanner::new(module_name);
    scanner.scan_directory(crate_path, &src_path)?;
    Ok(scanner.into_module())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_doc_from_single_line() {
        let source: syn::File = syn::parse_quote! {
            /// A point in 2D space.
            struct Point;
        };
        let attrs = match &source.items[0] {
            Item::Struct(s) => &s.attrs,
            _ => panic!("expected struct"),
        };
        assert_eq!(extract_doc_string(attrs), Some("A point in 2D space.".to_string()));
    }

    #[test]
    fn extract_doc_from_multiple_lines() {
        let source: syn::File = syn::parse_quote! {
            /// First line.
            /// Second line.
            /// Third line.
            struct Widget;
        };
        let attrs = match &source.items[0] {
            Item::Struct(s) => &s.attrs,
            _ => panic!("expected struct"),
        };
        assert_eq!(extract_doc_string(attrs), Some("First line.\nSecond line.\nThird line.".to_string()));
    }

    #[test]
    fn extract_doc_returns_none_for_undocumented() {
        let source: syn::File = syn::parse_quote! {
            struct Bare;
        };
        let attrs = match &source.items[0] {
            Item::Struct(s) => &s.attrs,
            _ => panic!("expected struct"),
        };
        assert_eq!(extract_doc_string(attrs), None);
    }

    #[test]
    fn extract_doc_trims_empty_lines() {
        let source: syn::File = syn::parse_quote! {
            ///
            /// Actual content.
            ///
            struct Padded;
        };
        let attrs = match &source.items[0] {
            Item::Struct(s) => &s.attrs,
            _ => panic!("expected struct"),
        };
        let doc = extract_doc_string(attrs).unwrap();
        assert_eq!(doc, "Actual content.");
    }

    fn pending(kind: PendingKind) -> TypeMeta {
        TypeMeta {
            doc: None,
            shape: TypeShape::Pending(kind),
        }
    }

    fn pending_with_doc(kind: PendingKind, doc: &str) -> TypeMeta {
        TypeMeta {
            doc: Some(doc.to_string()),
            shape: TypeShape::Pending(kind),
        }
    }

    #[test]
    fn type_registry_single_map_classify() {
        let mut reg = TypeRegistry::default();
        reg.register("Point".into(), pending(PendingKind::Record));
        reg.register("Color".into(), pending(PendingKind::Enum));
        reg.register("Sensor".into(), pending(PendingKind::Class));
        reg.register(
            "UtcDateTime".into(),
            TypeMeta {
                doc: None,
                shape: TypeShape::Custom {
                    repr: MType::Primitive(Primitive::I64),
                },
            },
        );

        assert!(matches!(reg.classify_named_type("Point"), Some(MType::Record(_))));
        assert!(matches!(reg.classify_named_type("Color"), Some(MType::Enum(_))));
        assert!(matches!(reg.classify_named_type("Sensor"), Some(MType::Object(_))));
        assert!(matches!(reg.classify_named_type("UtcDateTime"), Some(MType::Custom { .. })));
        assert!(reg.classify_named_type("Unknown").is_none());
    }

    #[test]
    fn type_registry_is_enum() {
        let mut reg = TypeRegistry::default();
        reg.register("Status".into(), pending(PendingKind::Enum));
        reg.register("Point".into(), pending(PendingKind::Record));

        assert!(reg.is_enum("Status"));
        assert!(!reg.is_enum("Point"));
        assert!(!reg.is_enum("Missing"));
    }

    #[test]
    fn type_registry_contains() {
        let mut reg = TypeRegistry::default();
        reg.register("Point".into(), pending(PendingKind::Record));

        assert!(reg.contains("Point"));
        assert!(!reg.contains("Nope"));
    }

    #[test]
    fn type_registry_doc_at_registration() {
        let mut reg = TypeRegistry::default();
        reg.register("Sensor".into(), pending_with_doc(PendingKind::Class, "A hardware sensor."));

        assert_eq!(reg.doc("Sensor"), Some("A hardware sensor."));
    }

    #[test]
    fn type_registry_set_doc_after_registration() {
        let mut reg = TypeRegistry::default();
        reg.register("Sensor".into(), pending(PendingKind::Class));
        reg.set_doc("Sensor", "A hardware sensor.".into());

        assert_eq!(reg.doc("Sensor"), Some("A hardware sensor."));
    }

    #[test]
    fn type_registry_set_doc_ignores_unregistered() {
        let mut reg = TypeRegistry::default();
        reg.set_doc("Ghost", "spooky".into());

        assert!(reg.doc("Ghost").is_none());
    }

    #[test]
    fn type_registry_custom_type_classifies_correctly() {
        let mut reg = TypeRegistry::default();
        reg.register(
            "Timestamp".into(),
            TypeMeta {
                doc: None,
                shape: TypeShape::Custom {
                    repr: MType::Primitive(Primitive::I64),
                },
            },
        );

        assert!(matches!(reg.classify_named_type("Timestamp"), Some(MType::Custom { .. })));
        assert!(!reg.is_enum("Timestamp"));
    }

    #[test]
    fn type_registry_fill_replaces_pending() {
        let mut reg = TypeRegistry::default();
        reg.register("Point".into(), pending(PendingKind::Record));
        reg.fill(
            "Point",
            TypeShape::Record {
                fields: vec![RecordField::new("x", MType::Primitive(Primitive::F64))],
            },
        );

        assert!(matches!(reg.classify_named_type("Point"), Some(MType::Record(_))));
        assert!(matches!(
            reg.types.get("Point").unwrap().shape,
            TypeShape::Record { ref fields } if fields.len() == 1
        ));
    }

    #[test]
    fn type_registry_filled_enum_still_is_enum() {
        let mut reg = TypeRegistry::default();
        reg.register("Color".into(), pending(PendingKind::Enum));
        reg.fill(
            "Color",
            TypeShape::Enum {
                variants: vec![],
                is_error: false,
            },
        );

        assert!(reg.is_enum("Color"));
    }

    #[test]
    fn extract_doc_from_struct_field() {
        let source: syn::File = syn::parse_quote! {
            struct Location {
                /// Unique identifier for this location.
                id: i64,
                lat: f64,
            }
        };
        let fields = match &source.items[0] {
            Item::Struct(s) => match &s.fields {
                Fields::Named(named) => &named.named,
                _ => panic!("expected named fields"),
            },
            _ => panic!("expected struct"),
        };
        assert_eq!(extract_doc_string(&fields[0].attrs), Some("Unique identifier for this location.".to_string()));
        assert_eq!(extract_doc_string(&fields[1].attrs), None);
    }

    #[test]
    fn extract_doc_from_enum_variant() {
        let source: syn::File = syn::parse_quote! {
            enum Direction {
                /// Pointing north.
                North,
                South,
            }
        };
        let variants = match &source.items[0] {
            Item::Enum(e) => &e.variants,
            _ => panic!("expected enum"),
        };
        assert_eq!(extract_doc_string(&variants[0].attrs), Some("Pointing north.".to_string()));
        assert_eq!(extract_doc_string(&variants[1].attrs), None);
    }

    #[test]
    fn extract_doc_from_enum_variant_field() {
        let source: syn::File = syn::parse_quote! {
            enum Status {
                Failed {
                    /// Error code describing the failure.
                    error_code: i32,
                    retry_count: i32,
                },
            }
        };
        let fields = match &source.items[0] {
            Item::Enum(e) => match &e.variants[0].fields {
                Fields::Named(named) => &named.named,
                _ => panic!("expected named fields"),
            },
            _ => panic!("expected enum"),
        };
        assert_eq!(extract_doc_string(&fields[0].attrs), Some("Error code describing the failure.".to_string()));
        assert_eq!(extract_doc_string(&fields[1].attrs), None);
    }
}
