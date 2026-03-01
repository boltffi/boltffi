use boltffi_ffi_rules::naming;
use proc_macro2::Span;
use quote::quote;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path as FsPath, PathBuf},
};
use syn::{Item, Path, PathArguments, PathSegment, Type, UseTree, punctuated::Punctuated};

pub fn ptr_ident(base: &syn::Ident) -> syn::Ident {
    syn::Ident::new(&format!("{}{}", base, naming::param_ptr_suffix()), base.span())
}

pub fn len_ident(base: &syn::Ident) -> syn::Ident {
    syn::Ident::new(&format!("{}{}", base, naming::param_len_suffix()), base.span())
}

pub enum ParamTransform {
    PassThrough,
    StrRef,
    OwnedString,
    Callback { params: Vec<syn::Type>, returns: Option<syn::Type> },
    SliceRef(syn::Type),
    SliceMut(syn::Type),
    BoxedDynTrait(syn::Path),
    ArcDynTrait(syn::Path),
    OptionArcDynTrait(syn::Path),
    ImplTrait(syn::Path),
    VecPrimitive(syn::Type),
    WireEncoded(WireEncodedParam),
}

#[derive(Clone)]
pub struct WireEncodedParam {
    pub kind: WireEncodedParamKind,
    pub rust_type: syn::Type,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WireEncodedParamKind {
    Vec,
    Option,
    Record,
}

pub fn extract_closure_signature(ty: &Type) -> Option<(Vec<syn::Type>, Option<syn::Type>)> {
    if let Type::BareFn(bare_fn) = ty {
        let params: Vec<syn::Type> = bare_fn.inputs.iter().map(|arg| arg.ty.clone()).collect();
        let returns = match &bare_fn.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => Some((**ty).clone()),
        };
        return Some((params, returns));
    }

    if let Type::ImplTrait(impl_trait) = ty {
        return impl_trait
            .bounds
            .iter()
            .filter_map(|bound| match bound {
                syn::TypeParamBound::Trait(trait_bound) => Some(&trait_bound.path),
                _ => None,
            })
            .filter_map(|path| path.segments.last())
            .filter_map(|segment| {
                let ident = segment.ident.to_string();
                (ident == "Fn" || ident == "FnMut" || ident == "FnOnce").then_some(&segment.arguments)
            })
            .filter_map(|arguments| match arguments {
                syn::PathArguments::Parenthesized(args) => Some(args),
                _ => None,
            })
            .map(|args| {
                let params: Vec<syn::Type> = args.inputs.iter().cloned().collect();
                let returns = match &args.output {
                    syn::ReturnType::Default => None,
                    syn::ReturnType::Type(_, ty) => Some((**ty).clone()),
                };
                (params, returns)
            })
            .next();
    }

    None
}

pub fn extract_slice_inner(ty: &Type) -> Option<(syn::Type, bool)> {
    if let Type::Reference(ref_ty) = ty
        && let Type::Slice(slice_ty) = ref_ty.elem.as_ref()
    {
        let is_mut = ref_ty.mutability.is_some();
        return Some((*slice_ty.elem.clone(), is_mut));
    }
    None
}

pub fn extract_impl_callback_trait(ty: &Type) -> Option<syn::Path> {
    if let Type::ImplTrait(impl_trait) = ty {
        return impl_trait
            .bounds
            .iter()
            .filter_map(|bound| match bound {
                syn::TypeParamBound::Trait(trait_bound) => Some((trait_bound.modifier, &trait_bound.path)),
                _ => None,
            })
            .filter(|(modifier, path)| {
                let trait_name = path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
                !is_non_callback_bound(*modifier, &trait_name)
            })
            .map(|(_, path)| path.clone())
            .next();
    }
    None
}

fn is_non_callback_bound(modifier: syn::TraitBoundModifier, name: &str) -> bool {
    if matches!(modifier, syn::TraitBoundModifier::Maybe(_)) && name == "Sized" {
        return true;
    }
    matches!(
        name,
        "Fn" | "FnMut"
            | "FnOnce"
            | "Send"
            | "Sync"
            | "Unpin"
            | "UnwindSafe"
            | "RefUnwindSafe"
            | "Sized"
            | "Copy"
            | "Clone"
            | "Default"
            | "Debug"
            | "Eq"
            | "PartialEq"
            | "Ord"
            | "PartialOrd"
            | "Hash"
    )
}

pub fn foreign_trait_path(trait_path: &syn::Path) -> syn::Path {
    let resolved = resolve_alias_path(trait_path).unwrap_or_else(|| trait_path.clone());
    foreign_trait_path_from(&resolved)
}

fn foreign_trait_path_from(trait_path: &syn::Path) -> syn::Path {
    let foreign_ident = trait_path
        .segments
        .last()
        .map(|segment| syn::Ident::new(&format!("Foreign{}", segment.ident), segment.ident.span()))
        .unwrap_or_else(|| syn::Ident::new("Foreign", Span::call_site()));
    let mut foreign_path = trait_path.clone();
    if let Some(segment) = foreign_path.segments.last_mut() {
        segment.ident = foreign_ident;
    }
    foreign_path
}

fn resolve_alias_path(trait_path: &Path) -> Option<Path> {
    let resolver = alias_resolver_for_call_site()?;
    resolver.resolve_path(trait_path)
}

fn alias_resolver_for_call_site() -> Option<AliasResolver> {
    build_alias_resolver()
}

fn build_alias_resolver() -> Option<AliasResolver> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok()?;
    let src_root = PathBuf::from(manifest_dir).join("src");
    let files = list_rs_files(&src_root)?;
    let resolver = files
        .iter()
        .filter_map(|file_path| {
            let content = fs::read_to_string(file_path).ok()?;
            let syntax = syn::parse_file(&content).ok()?;
            Some(AliasResolver::from_items(&syntax.items))
        })
        .fold(AliasResolver::default(), |mut acc, next| {
            acc.merge(next);
            acc
        });
    Some(resolver)
}

fn list_rs_files(root: &FsPath) -> Option<Vec<PathBuf>> {
    fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .try_fold(Vec::new(), |mut acc, path| {
            if path.is_dir() {
                let mut nested = list_rs_files(&path)?;
                acc.append(&mut nested);
                Some(acc)
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                acc.push(path);
                Some(acc)
            } else {
                Some(acc)
            }
        })
}

#[derive(Default, Clone)]
struct AliasResolver {
    use_aliases: HashMap<String, Vec<PathSegment>>,
    type_aliases: HashMap<String, Vec<PathSegment>>,
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
                    Type::Path(type_path) => Some(path_segments(&type_path.path)),
                    _ => None,
                }?;
                Some((item_type.ident.to_string(), target))
            })
            .for_each(|(alias, target)| {
                resolver.type_aliases.insert(alias, target);
            });

        resolver
    }

    fn merge(&mut self, other: AliasResolver) {
        self.use_aliases.extend(other.use_aliases);
        self.type_aliases.extend(other.type_aliases);
    }

    fn resolve_path(&self, path: &Path) -> Option<Path> {
        let segments = path_segments(path);
        let first = segments.first()?;
        let first_name = first.ident.to_string();
        let is_single = segments.len() == 1;

        let resolved = self
            .use_aliases
            .get(&first_name)
            .map(|prefix| {
                let rest = segments.iter().skip(1).cloned();
                prefix.iter().cloned().chain(rest).collect::<Vec<_>>()
            })
            .or_else(|| is_single.then(|| self.type_aliases.get(&first_name).cloned()).flatten())?;

        let original_args = first.arguments.clone();
        let mut adjusted = resolved;
        if is_single && let Some(last) = adjusted.last_mut() {
            last.arguments = original_args;
        }

        Some(build_path(adjusted))
    }

    fn collect_use_tree(&mut self, prefix: Vec<PathSegment>, tree: &UseTree) {
        match tree {
            UseTree::Path(path) => {
                let mut next_prefix = prefix;
                next_prefix.push(path_segment(&path.ident));
                self.collect_use_tree(next_prefix, &path.tree);
            }
            UseTree::Name(name) => {
                let mut target = prefix;
                target.push(path_segment(&name.ident));
                self.use_aliases.insert(name.ident.to_string(), target);
            }
            UseTree::Rename(rename) => {
                let mut target = prefix;
                target.push(path_segment(&rename.ident));
                self.use_aliases.insert(rename.rename.to_string(), target);
            }
            UseTree::Group(group) => group.items.iter().for_each(|item| self.collect_use_tree(prefix.clone(), item)),
            UseTree::Glob(_) => {}
        }
    }
}

fn path_segments(path: &Path) -> Vec<PathSegment> {
    path.segments.iter().cloned().collect()
}

fn path_segment(ident: &syn::Ident) -> PathSegment {
    PathSegment {
        ident: ident.clone(),
        arguments: PathArguments::None,
    }
}

fn build_path(segments: Vec<PathSegment>) -> Path {
    Path {
        leading_colon: None,
        segments: Punctuated::from_iter(segments),
    }
}

fn extract_dyn_trait_in_container(ty: &Type, container: &str) -> Option<syn::Path> {
    if let Type::Path(type_path) = ty
        && type_path.qself.is_none()
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == container
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(Type::TraitObject(trait_obj))) = args.args.first()
        && let Some(syn::TypeParamBound::Trait(trait_bound)) = trait_obj.bounds.first()
    {
        return Some(trait_bound.path.clone());
    }
    None
}

pub fn extract_vec_param_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn extract_option_param_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

pub fn is_primitive_vec_inner(s: &str) -> bool {
    matches!(s, "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool")
}

pub fn classify_param_transform(ty: &Type) -> ParamTransform {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    if let Some((params, returns)) = extract_closure_signature(ty) {
        return ParamTransform::Callback { params, returns };
    }

    if let Some(trait_path) = extract_impl_callback_trait(ty) {
        return ParamTransform::ImplTrait(trait_path);
    }

    if let Some((inner_ty, is_mut)) = extract_slice_inner(ty) {
        return if is_mut {
            ParamTransform::SliceMut(inner_ty)
        } else {
            ParamTransform::SliceRef(inner_ty)
        };
    }

    if let Some(trait_path) = extract_dyn_trait_in_container(ty, "Box") {
        return ParamTransform::BoxedDynTrait(trait_path);
    }

    if let Some(trait_path) = extract_dyn_trait_in_container(ty, "Arc") {
        return ParamTransform::ArcDynTrait(trait_path);
    }

    if type_str.starts_with("*const") || type_str.starts_with("*mut") {
        return ParamTransform::PassThrough;
    }

    if type_str.contains("extern") && type_str.contains("fn(") {
        return ParamTransform::PassThrough;
    }

    if let Some(inner_ty) = extract_vec_param_inner(ty) {
        let inner_str = quote!(#inner_ty).to_string().replace(' ', "");
        if is_primitive_vec_inner(&inner_str) {
            return ParamTransform::VecPrimitive(inner_ty);
        } else {
            return ParamTransform::WireEncoded(WireEncodedParam {
                kind: WireEncodedParamKind::Vec,
                rust_type: ty.clone(),
            });
        }
    }

    if let Some(inner_ty) = extract_option_param_inner(ty) {
        if let Some(trait_path) = extract_dyn_trait_in_container(&inner_ty, "Arc") {
            return ParamTransform::OptionArcDynTrait(trait_path);
        }
        return ParamTransform::WireEncoded(WireEncodedParam {
            kind: WireEncodedParamKind::Option,
            rust_type: ty.clone(),
        });
    }

    if type_str == "&str" || (type_str.starts_with("&'") && type_str.ends_with("str")) {
        ParamTransform::StrRef
    } else if type_str == "String" || type_str == "std::string::String" {
        ParamTransform::OwnedString
    } else if is_record_type(&type_str) {
        ParamTransform::WireEncoded(WireEncodedParam {
            kind: WireEncodedParamKind::Record,
            rust_type: ty.clone(),
        })
    } else {
        ParamTransform::PassThrough
    }
}

fn is_record_type(type_str: &str) -> bool {
    if is_primitive_type(type_str) {
        return false;
    }
    if type_str.starts_with('&') || type_str.starts_with('*') {
        return false;
    }
    if type_str.contains('<') || type_str.contains('>') {
        return false;
    }
    type_str.chars().next().is_some_and(|c| c.is_uppercase())
}

fn is_primitive_type(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool" | "isize" | "usize" | "()"
    )
}
