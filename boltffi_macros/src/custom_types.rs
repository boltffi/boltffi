use boltffi_ffi_rules::naming;
use proc_macro2::Span;
use quote::{format_ident, quote};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use syn::{GenericArgument, PathArguments, Type, parse::Parse};

#[derive(Clone)]
pub struct CustomTypeEntry {
    module_path: Vec<String>,
    name: String,
    remote_normalized: String,
    remote_shape: String,
    repr: String,
}

impl CustomTypeEntry {
    pub fn repr_type(&self) -> syn::Result<syn::Type> {
        let repr = syn::parse_str::<Type>(&self.repr)?;
        Ok(qualify_type_for_module(repr, &self.module_path))
    }

    pub fn to_fn_path(&self) -> proc_macro2::TokenStream {
        let snake = naming::to_snake_case(&self.name);
        let fn_name = format_ident!("__boltffi_custom_type_{}_into_ffi", snake);
        let module_path = self.module_path.iter().map(|segment| syn::Ident::new(segment, Span::call_site())).collect::<Vec<_>>();
        quote! { crate::#(#module_path::)*#fn_name }
    }

    pub fn try_from_fn_path(&self) -> proc_macro2::TokenStream {
        let snake = naming::to_snake_case(&self.name);
        let fn_name = format_ident!("__boltffi_custom_type_{}_try_from_ffi", snake);
        let module_path = self.module_path.iter().map(|segment| syn::Ident::new(segment, Span::call_site())).collect::<Vec<_>>();
        quote! { crate::#(#module_path::)*#fn_name }
    }
}

fn qualify_type_for_module(ty: Type, module_path: &[String]) -> Type {
    match ty {
        Type::Array(array) => Type::Array(syn::TypeArray {
            elem: Box::new(qualify_type_for_module(*array.elem, module_path)),
            ..array
        }),
        Type::Group(group) => Type::Group(syn::TypeGroup {
            elem: Box::new(qualify_type_for_module(*group.elem, module_path)),
            ..group
        }),
        Type::Paren(paren) => Type::Paren(syn::TypeParen {
            elem: Box::new(qualify_type_for_module(*paren.elem, module_path)),
            ..paren
        }),
        Type::Ptr(ptr) => Type::Ptr(syn::TypePtr {
            elem: Box::new(qualify_type_for_module(*ptr.elem, module_path)),
            ..ptr
        }),
        Type::Reference(reference) => Type::Reference(syn::TypeReference {
            elem: Box::new(qualify_type_for_module(*reference.elem, module_path)),
            ..reference
        }),
        Type::Slice(slice) => Type::Slice(syn::TypeSlice {
            elem: Box::new(qualify_type_for_module(*slice.elem, module_path)),
            ..slice
        }),
        Type::Tuple(tuple) => Type::Tuple(syn::TypeTuple {
            elems: tuple.elems.into_iter().map(|elem| qualify_type_for_module(elem, module_path)).collect(),
            ..tuple
        }),
        Type::Path(type_path) => Type::Path(qualify_type_path_for_module(type_path, module_path)),
        other => other,
    }
}

fn qualify_type_path_for_module(type_path: syn::TypePath, module_path: &[String]) -> syn::TypePath {
    let qself = type_path.qself.clone();
    let path = qualify_path_segments_for_module(type_path.path, module_path);
    let type_path = syn::TypePath { qself, path };

    if !is_single_segment_path(&type_path.path) {
        return type_path;
    }

    if type_path.path.leading_colon.is_some() {
        return type_path;
    }

    if type_path.qself.is_some() {
        return type_path;
    }

    let Some(segment) = type_path.path.segments.last() else {
        return type_path;
    };

    let ident = segment.ident.to_string();
    let should_qualify = ident.chars().next().is_some_and(|ch| ch.is_uppercase()) && !is_std_global_ident(&ident);

    if should_qualify {
        {
            let module_segments = module_path
                .iter()
                .map(|segment| syn::Ident::new(segment, Span::call_site()))
                .map(syn::PathSegment::from)
                .collect::<syn::punctuated::Punctuated<_, syn::Token![::]>>();

            let mut segments = syn::punctuated::Punctuated::<syn::PathSegment, syn::Token![::]>::new();
            segments.push(syn::PathSegment::from(syn::Ident::new("crate", Span::call_site())));
            module_segments.into_iter().for_each(|segment| segments.push(segment));
            segments.push(segment.clone());

            syn::TypePath {
                qself: None,
                path: syn::Path { leading_colon: None, segments },
            }
        }
    } else {
        type_path
    }
}

fn qualify_path_segments_for_module(path: syn::Path, module_path: &[String]) -> syn::Path {
    let segments = path
        .segments
        .into_iter()
        .map(|segment| qualify_path_segment_for_module(segment, module_path))
        .collect::<syn::punctuated::Punctuated<_, syn::Token![::]>>();
    syn::Path {
        leading_colon: path.leading_colon,
        segments,
    }
}

fn qualify_path_segment_for_module(mut segment: syn::PathSegment, module_path: &[String]) -> syn::PathSegment {
    segment.arguments = match segment.arguments {
        PathArguments::AngleBracketed(args) => PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
            args: args
                .args
                .into_iter()
                .map(|arg| match arg {
                    GenericArgument::Type(inner_ty) => GenericArgument::Type(qualify_type_for_module(inner_ty, module_path)),
                    other => other,
                })
                .collect(),
            ..args
        }),
        other => other,
    };
    segment
}

fn is_single_segment_path(path: &syn::Path) -> bool {
    path.segments.len() == 1
}

fn is_std_global_ident(ident: &str) -> bool {
    matches!(ident, "String" | "Vec" | "Option" | "Result" | "Box" | "Arc" | "Rc" | "Cow")
}

#[derive(Default, Clone)]
pub struct CustomTypeRegistry {
    by_name: HashMap<String, CustomTypeEntry>,
    by_remote_normalized: HashMap<String, String>,
    by_remote_shape: HashMap<String, String>,
}

impl CustomTypeRegistry {
    pub fn lookup(&self, ty: &syn::Type) -> Option<&CustomTypeEntry> {
        let normalized = normalize_type(ty);
        self.by_remote_normalized
            .get(&normalized)
            .and_then(|name| self.by_name.get(name))
            .or_else(|| {
                let shape = type_shape_key(ty);
                self.by_remote_shape.get(&shape).and_then(|name| self.by_name.get(name))
            })
            .or_else(|| type_last_segment(ty).and_then(|name| self.by_name.get(&name)))
    }

    fn register(&mut self, entry: CustomTypeEntry) -> syn::Result<()> {
        match self.by_name.get(&entry.name) {
            None => {}
            Some(_) => {
                return Err(syn::Error::new(Span::call_site(), format!("custom_type!: duplicate definition for `{}`", entry.name)));
            }
        }

        match self.by_remote_normalized.get(&entry.remote_normalized) {
            None => {}
            Some(existing) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("custom_type!: remote type already registered by `{}`", existing),
                ));
            }
        }

        match self.by_remote_shape.get(&entry.remote_shape) {
            None => {}
            Some(existing) if *existing == entry.name => {}
            Some(_) => {}
        }

        let name = entry.name.clone();
        let remote_normalized = entry.remote_normalized.clone();
        let remote_shape = entry.remote_shape.clone();

        self.by_name.insert(name.clone(), entry);
        self.by_remote_normalized.insert(remote_normalized, name.clone());
        self.by_remote_shape.entry(remote_shape).or_insert(name);
        Ok(())
    }
}

struct CustomTypeMacroSpec {
    visibility: syn::Visibility,
    name: syn::Ident,
    repr: String,
    remote: syn::Type,
}

impl Parse for CustomTypeMacroSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let visibility: syn::Visibility = input.parse()?;
        let name: syn::Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;

        let mut repr: Option<syn::Type> = None;
        let mut remote: Option<syn::Type> = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            match key.to_string().as_str() {
                "repr" => {
                    repr = Some(input.parse()?);
                }
                "remote" => {
                    remote = Some(input.parse()?);
                }
                "error" => {
                    let _: syn::Type = input.parse()?;
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
        let remote = remote.ok_or_else(|| input.error("custom_type!: missing `remote = ...`"))?;
        Ok(Self {
            visibility,
            name,
            repr: quote::quote!(#repr).to_string(),
            remote,
        })
    }
}
struct CustomTypeCollector<'a> {
    module_path: Vec<syn::Ident>,
    custom_types: &'a mut CustomTypeRegistry,
}

impl<'a> CustomTypeCollector<'a> {
    fn collect_module(mut self, module: &syn::ItemMod) -> syn::Result<()> {
        let Some((_, items)) = &module.content else {
            return Ok(());
        };

        self.module_path.push(module.ident.clone());
        items.iter().try_for_each(|item| self.collect_item(item))?;
        self.module_path.pop();
        Ok(())
    }

    fn collect_item(&mut self, item: &syn::Item) -> syn::Result<()> {
        match item {
            syn::Item::Macro(item_macro) => self.collect_item_macro(item_macro),
            syn::Item::Mod(item_mod) => CustomTypeCollector {
                module_path: self.module_path.clone(),
                custom_types: self.custom_types,
            }
            .collect_module(item_mod),
            _ => Ok(()),
        }
    }

    fn collect_item_macro(&mut self, item_macro: &syn::ItemMacro) -> syn::Result<()> {
        let is_custom_type = item_macro.mac.path.segments.last().is_some_and(|segment| segment.ident == "custom_type");

        if !is_custom_type {
            return Ok(());
        }

        let spec: CustomTypeMacroSpec = syn::parse2(item_macro.mac.tokens.clone())?;
        let name = spec.name.to_string();

        if !matches!(spec.visibility, syn::Visibility::Public(_)) {
            return Ok(());
        }

        let entry = CustomTypeEntry {
            module_path: self.module_path.iter().map(|id| id.to_string()).collect(),
            name,
            remote_normalized: normalize_type(&spec.remote),
            remote_shape: type_shape_key(&spec.remote),
            repr: spec.repr,
        };

        self.custom_types.register(entry)
    }
}

static REGISTRY_CACHE: OnceLock<Mutex<HashMap<PathBuf, CustomTypeRegistry>>> = OnceLock::new();

pub fn registry_for_current_crate() -> syn::Result<CustomTypeRegistry> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| syn::Error::new(Span::call_site(), "CARGO_MANIFEST_DIR not set"))?;
    let manifest_dir = PathBuf::from(manifest_dir);

    let cache = REGISTRY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cached = cache
        .lock()
        .map_err(|_| syn::Error::new(Span::call_site(), "custom type registry lock poisoned"))?
        .get(&manifest_dir)
        .cloned();
    if let Some(registry) = cached {
        return Ok(registry);
    }

    let registry = build_registry(&manifest_dir)?;
    let mut guard = cache.lock().map_err(|_| syn::Error::new(Span::call_site(), "custom type registry lock poisoned"))?;
    guard.insert(manifest_dir, registry.clone());
    Ok(registry)
}

fn build_registry(manifest_dir: &Path) -> syn::Result<CustomTypeRegistry> {
    let src_root = manifest_dir.join("src");
    let files = list_rs_files(&src_root)?;

    let mut registry = CustomTypeRegistry::default();
    files.iter().try_for_each(|file_path| {
        let module_path = module_path_for_rs_file(&src_root, file_path)?;
        let content = fs::read_to_string(file_path).map_err(|e| syn::Error::new(Span::call_site(), format!("read {}: {}", file_path.display(), e)))?;
        let syntax = syn::parse_file(&content)?;

        let mut collector = CustomTypeCollector {
            module_path,
            custom_types: &mut registry,
        };

        syntax.items.iter().try_for_each(|item| collector.collect_item(item))
    })?;

    Ok(registry)
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

fn module_path_for_rs_file(src_root: &Path, file_path: &Path) -> syn::Result<Vec<syn::Ident>> {
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

    Ok(module_parts.into_iter().filter(|p| !p.is_empty()).map(|p| syn::Ident::new(&p, Span::call_site())).collect())
}

pub fn contains_custom_types(ty: &syn::Type, registry: &CustomTypeRegistry) -> bool {
    if registry.lookup(ty).is_some() {
        return true;
    }

    match ty {
        syn::Type::Reference(reference) => contains_custom_types(reference.elem.as_ref(), registry),
        syn::Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return false;
            };

            match segment.ident.to_string().as_str() {
                "Vec" | "Option" => angle_arg_types(&segment.arguments)
                    .into_iter()
                    .next()
                    .is_some_and(|inner| contains_custom_types(inner, registry)),
                "Result" => {
                    let mut args = angle_arg_types(&segment.arguments).into_iter();
                    args.next().is_some_and(|ok| contains_custom_types(ok, registry)) || args.next().is_some_and(|err| contains_custom_types(err, registry))
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn angle_arg_types(arguments: &syn::PathArguments) -> Vec<&syn::Type> {
    let syn::PathArguments::AngleBracketed(args) = arguments else {
        return Vec::new();
    };
    args.args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(inner_ty) => Some(inner_ty),
            _ => None,
        })
        .collect()
}

pub fn wire_type_for(ty: &syn::Type, registry: &CustomTypeRegistry) -> syn::Type {
    registry
        .lookup(ty)
        .and_then(|entry| entry.repr_type().ok())
        .or_else(|| wire_type_for_container(ty, registry))
        .unwrap_or_else(|| ty.clone())
}

fn wire_type_for_container(ty: &syn::Type, registry: &CustomTypeRegistry) -> Option<syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    match segment.ident.to_string().as_str() {
        "Vec" => args.args.first().and_then(type_arg).map(|inner| {
            let inner_wire = wire_type_for(inner, registry);
            syn::parse_quote!(Vec<#inner_wire>)
        }),
        "Option" => args.args.first().and_then(type_arg).map(|inner| {
            let inner_wire = wire_type_for(inner, registry);
            syn::parse_quote!(Option<#inner_wire>)
        }),
        "Result" => {
            let ok = args.args.first().and_then(type_arg)?;
            let err = args.args.iter().nth(1).and_then(type_arg)?;
            let ok_wire = wire_type_for(ok, registry);
            let err_wire = wire_type_for(err, registry);
            Some(syn::parse_quote!(Result<#ok_wire, #err_wire>))
        }
        _ => None,
    }
}

fn type_arg(arg: &syn::GenericArgument) -> Option<&syn::Type> {
    match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

pub fn to_wire_expr_owned(ty: &syn::Type, registry: &CustomTypeRegistry, value_ident: &syn::Ident) -> proc_macro2::TokenStream {
    if let Some(entry) = registry.lookup(ty) {
        let into_fn = entry.to_fn_path();
        return quote! { #into_fn(&#value_ident) };
    }

    let syn::Type::Path(type_path) = ty else {
        return quote! { #value_ident };
    };
    let Some(segment) = type_path.path.segments.last() else {
        return quote! { #value_ident };
    };
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return quote! { #value_ident };
    };

    match segment.ident.to_string().as_str() {
        "Vec" => {
            let Some(inner) = args.args.first().and_then(type_arg) else {
                return quote! { #value_ident };
            };
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = to_wire_expr_owned(inner, registry, &inner_ident);
            quote! {
                #value_ident
                    .into_iter()
                    .map(|#inner_ident| #inner_expr)
                    .collect::<Vec<_>>()
            }
        }
        "Option" => {
            let Some(inner) = args.args.first().and_then(type_arg) else {
                return quote! { #value_ident };
            };
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = to_wire_expr_owned(inner, registry, &inner_ident);
            quote! { #value_ident.map(|#inner_ident| #inner_expr) }
        }
        "Result" => {
            let ok = args.args.first().and_then(type_arg);
            let err = args.args.iter().nth(1).and_then(type_arg);
            match (ok, err) {
                (Some(ok), Some(err)) => {
                    let ok_ident = syn::Ident::new("__boltffi_ok", Span::call_site());
                    let err_ident = syn::Ident::new("__boltffi_err", Span::call_site());
                    let ok_expr = to_wire_expr_owned(ok, registry, &ok_ident);
                    let err_expr = to_wire_expr_owned(err, registry, &err_ident);
                    quote! {
                        match #value_ident {
                            Ok(#ok_ident) => Ok(#ok_expr),
                            Err(#err_ident) => Err(#err_expr),
                        }
                    }
                }
                _ => quote! { #value_ident },
            }
        }
        _ => quote! { #value_ident },
    }
}

pub fn from_wire_expr_owned(ty: &syn::Type, registry: &CustomTypeRegistry, value_ident: &syn::Ident) -> proc_macro2::TokenStream {
    if let Some(entry) = registry.lookup(ty) {
        let try_from_fn = entry.try_from_fn_path();
        let error_message = format!("{}: custom type conversion failed", entry.name);
        return quote! { #try_from_fn(#value_ident).expect(#error_message) };
    }

    let syn::Type::Path(type_path) = ty else {
        return quote! { #value_ident };
    };
    let Some(segment) = type_path.path.segments.last() else {
        return quote! { #value_ident };
    };
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return quote! { #value_ident };
    };

    match segment.ident.to_string().as_str() {
        "Vec" => {
            let Some(inner) = args.args.first().and_then(type_arg) else {
                return quote! { #value_ident };
            };
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = from_wire_expr_owned(inner, registry, &inner_ident);
            quote! {
                #value_ident
                    .into_iter()
                    .map(|#inner_ident| #inner_expr)
                    .collect::<Vec<_>>()
            }
        }
        "Option" => {
            let Some(inner) = args.args.first().and_then(type_arg) else {
                return quote! { #value_ident };
            };
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = from_wire_expr_owned(inner, registry, &inner_ident);
            quote! { #value_ident.map(|#inner_ident| #inner_expr) }
        }
        "Result" => {
            let ok = args.args.first().and_then(type_arg);
            let err = args.args.iter().nth(1).and_then(type_arg);
            match (ok, err) {
                (Some(ok), Some(err)) => {
                    let ok_ident = syn::Ident::new("__boltffi_ok", Span::call_site());
                    let err_ident = syn::Ident::new("__boltffi_err", Span::call_site());
                    let ok_expr = from_wire_expr_owned(ok, registry, &ok_ident);
                    let err_expr = from_wire_expr_owned(err, registry, &err_ident);
                    quote! {
                        match #value_ident {
                            Ok(#ok_ident) => Ok(#ok_expr),
                            Err(#err_ident) => Err(#err_expr),
                        }
                    }
                }
                _ => quote! { #value_ident },
            }
        }
        _ => quote! { #value_ident },
    }
}

fn normalize_type(ty: &syn::Type) -> String {
    quote::quote!(#ty).to_string().replace(' ', "")
}

fn type_last_segment(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    type_path.path.segments.last().map(|segment| segment.ident.to_string())
}

fn type_shape_key(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Reference(reference) => type_shape_key(reference.elem.as_ref()),
        syn::Type::Path(type_path) => type_path.path.segments.last().map(shape_key_for_segment).unwrap_or_else(|| normalize_type(ty)),
        _ => normalize_type(ty),
    }
}

fn shape_key_for_segment(segment: &syn::PathSegment) -> String {
    let ident = segment.ident.to_string();
    let args = match &segment.arguments {
        syn::PathArguments::AngleBracketed(args) => args
            .args
            .iter()
            .filter_map(|arg| match arg {
                syn::GenericArgument::Type(inner_ty) => Some(type_shape_key(inner_ty)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    if args.is_empty() { ident.clone() } else { format!("{}<{}>", ident, args.join(",")) }
}
