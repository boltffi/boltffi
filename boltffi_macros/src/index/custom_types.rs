use std::collections::HashMap;

use boltffi_ffi_rules::naming;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::parse::Parse;
use syn::{GenericArgument, PathArguments, Type};

use crate::index::CrateIndex;
use crate::index::source::IndexedSource;

#[derive(Clone)]
pub struct CustomTypeEntry {
    root_path: Vec<String>,
    module_path: Vec<String>,
    name: String,
    remote_normalized: String,
    remote_shape: String,
    repr: String,
}

struct TypeQualifier<'a> {
    root_path: &'a [String],
    module_path: &'a [String],
}

struct RemoteTypeKey {
    normalized: String,
    shape: String,
    last_segment: Option<String>,
}

enum ContainerTypeDescriptor<'a> {
    Vec(&'a syn::Type),
    Option(&'a syn::Type),
    Result {
        ok: &'a syn::Type,
        err: &'a syn::Type,
    },
    Other,
}

impl CustomTypeEntry {
    pub fn repr_type(&self) -> syn::Result<syn::Type> {
        let repr = syn::parse_str::<Type>(&self.repr)?;
        Ok(TypeQualifier::new(&self.root_path, &self.module_path).qualify_type(repr))
    }

    pub fn to_fn_path(&self) -> proc_macro2::TokenStream {
        let snake = naming::to_snake_case(&self.name);
        let fn_name = format_ident!("__boltffi_custom_type_{}_into_ffi", snake);
        let module_path = self
            .helper_path_segments()
            .iter()
            .map(|segment| syn::Ident::new(segment, Span::call_site()))
            .collect::<Vec<_>>();
        quote! { #(#module_path::)*#fn_name }
    }

    pub fn try_from_fn_path(&self) -> proc_macro2::TokenStream {
        let snake = naming::to_snake_case(&self.name);
        let fn_name = format_ident!("__boltffi_custom_type_{}_try_from_ffi", snake);
        let module_path = self
            .helper_path_segments()
            .iter()
            .map(|segment| syn::Ident::new(segment, Span::call_site()))
            .collect::<Vec<_>>();
        quote! { #(#module_path::)*#fn_name }
    }

    fn helper_path_segments(&self) -> Vec<String> {
        if self.root_path.is_empty() {
            std::iter::once("crate".to_string())
                .chain(self.module_path.iter().cloned())
                .collect()
        } else {
            self.root_path
                .iter()
                .cloned()
                .chain(self.module_path.iter().cloned())
                .collect()
        }
    }
}

impl<'a> TypeQualifier<'a> {
    fn new(root_path: &'a [String], module_path: &'a [String]) -> Self {
        Self {
            root_path,
            module_path,
        }
    }

    fn qualify_type(&self, rust_type: Type) -> Type {
        match rust_type {
            Type::Array(array) => Type::Array(syn::TypeArray {
                elem: Box::new(self.qualify_type(*array.elem)),
                ..array
            }),
            Type::Group(group) => Type::Group(syn::TypeGroup {
                elem: Box::new(self.qualify_type(*group.elem)),
                ..group
            }),
            Type::Paren(paren) => Type::Paren(syn::TypeParen {
                elem: Box::new(self.qualify_type(*paren.elem)),
                ..paren
            }),
            Type::Ptr(pointer) => Type::Ptr(syn::TypePtr {
                elem: Box::new(self.qualify_type(*pointer.elem)),
                ..pointer
            }),
            Type::Reference(reference) => Type::Reference(syn::TypeReference {
                elem: Box::new(self.qualify_type(*reference.elem)),
                ..reference
            }),
            Type::Slice(slice) => Type::Slice(syn::TypeSlice {
                elem: Box::new(self.qualify_type(*slice.elem)),
                ..slice
            }),
            Type::Tuple(tuple) => Type::Tuple(syn::TypeTuple {
                elems: tuple
                    .elems
                    .into_iter()
                    .map(|element| self.qualify_type(element))
                    .collect(),
                ..tuple
            }),
            Type::Path(type_path) => Type::Path(self.qualify_type_path(type_path)),
            other => other,
        }
    }

    fn qualify_type_path(&self, type_path: syn::TypePath) -> syn::TypePath {
        let qualified_type_path = syn::TypePath {
            qself: type_path.qself.clone(),
            path: self.qualify_path(type_path.path),
        };

        if !self.should_qualify_single_segment(&qualified_type_path) {
            return qualified_type_path;
        }

        let Some(last_segment) = qualified_type_path.path.segments.last() else {
            return qualified_type_path;
        };

        let mut segments = syn::punctuated::Punctuated::<syn::PathSegment, syn::Token![::]>::new();
        self.root_segments()
            .into_iter()
            .map(|segment| syn::Ident::new(&segment, Span::call_site()))
            .map(syn::PathSegment::from)
            .for_each(|segment| segments.push(segment));
        self.module_path
            .iter()
            .map(|segment| syn::Ident::new(segment, Span::call_site()))
            .map(syn::PathSegment::from)
            .for_each(|segment| segments.push(segment));
        segments.push(last_segment.clone());

        syn::TypePath {
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments,
            },
        }
    }

    fn qualify_path(&self, path: syn::Path) -> syn::Path {
        let segments = path
            .segments
            .into_iter()
            .map(|segment| self.qualify_path_segment(segment))
            .collect::<syn::punctuated::Punctuated<_, syn::Token![::]>>();
        syn::Path {
            leading_colon: path.leading_colon,
            segments,
        }
    }

    fn qualify_path_segment(&self, mut segment: syn::PathSegment) -> syn::PathSegment {
        segment.arguments = match segment.arguments {
            PathArguments::AngleBracketed(arguments) => {
                PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                    args: arguments
                        .args
                        .into_iter()
                        .map(|argument| match argument {
                            GenericArgument::Type(inner_type) => {
                                GenericArgument::Type(self.qualify_type(inner_type))
                            }
                            other => other,
                        })
                        .collect(),
                    ..arguments
                })
            }
            other => other,
        };
        segment
    }

    fn should_qualify_single_segment(&self, type_path: &syn::TypePath) -> bool {
        if type_path.path.segments.len() != 1 {
            return false;
        }
        if type_path.path.leading_colon.is_some() || type_path.qself.is_some() {
            return false;
        }

        type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .is_some_and(|ident| {
                ident
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_uppercase())
                    && !matches!(
                        ident.as_str(),
                        "String" | "Vec" | "Option" | "Result" | "Box" | "Arc" | "Rc" | "Cow"
                    )
            })
    }

    fn root_segments(&self) -> Vec<String> {
        if self.root_path.is_empty() {
            vec!["crate".to_string()]
        } else {
            self.root_path.to_vec()
        }
    }
}

#[derive(Default, Clone)]
pub struct CustomTypeRegistry {
    by_name: HashMap<String, CustomTypeEntry>,
    by_remote_normalized: HashMap<String, String>,
    by_remote_shape: HashMap<String, String>,
}

impl CustomTypeRegistry {
    pub fn lookup(&self, ty: &syn::Type) -> Option<&CustomTypeEntry> {
        let remote_type_key = RemoteTypeKey::from_type(ty);
        self.by_remote_normalized
            .get(&remote_type_key.normalized)
            .and_then(|name| self.by_name.get(name))
            .or_else(|| {
                self.by_remote_shape
                    .get(&remote_type_key.shape)
                    .and_then(|name| self.by_name.get(name))
            })
            .or_else(|| {
                remote_type_key
                    .last_segment
                    .as_ref()
                    .and_then(|name| self.by_name.get(name))
            })
    }

    fn register(&mut self, entry: CustomTypeEntry) -> syn::Result<()> {
        match self.by_name.get(&entry.name) {
            None => {}
            Some(_) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("custom_type!: duplicate definition for `{}`", entry.name),
                ));
            }
        }

        match self.by_remote_normalized.get(&entry.remote_normalized) {
            None => {}
            Some(existing) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!(
                        "custom_type!: remote type already registered by `{}`",
                        existing
                    ),
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
        self.by_remote_normalized
            .insert(remote_normalized, name.clone());
        self.by_remote_shape.entry(remote_shape).or_insert(name);
        Ok(())
    }
}

struct CustomTypeMacroSpec {
    name: syn::Ident,
    repr: String,
    remote: syn::Type,
}

impl Parse for CustomTypeMacroSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _: syn::Visibility = input.parse()?;
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
            name,
            repr: quote::quote!(#repr).to_string(),
            remote,
        })
    }
}

struct CustomTypeCollector<'a> {
    root_path: Vec<String>,
    module_path: Vec<syn::Ident>,
    custom_types: &'a mut CustomTypeRegistry,
}

impl<'a> CustomTypeCollector<'a> {
    fn collect_item(&mut self, item: &syn::Item) -> syn::Result<()> {
        match item {
            syn::Item::Macro(item_macro) => self.collect_item_macro(item_macro),
            _ => Ok(()),
        }
    }

    fn collect_item_macro(&mut self, item_macro: &syn::ItemMacro) -> syn::Result<()> {
        let is_custom_type = item_macro
            .mac
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "custom_type");

        if !is_custom_type {
            return Ok(());
        }

        let spec: CustomTypeMacroSpec = syn::parse2(item_macro.mac.tokens.clone())?;
        let name = spec.name.to_string();
        let remote_type_key = RemoteTypeKey::from_type(&spec.remote);

        let entry = CustomTypeEntry {
            root_path: self.root_path.clone(),
            module_path: self.module_path.iter().map(|id| id.to_string()).collect(),
            name,
            remote_normalized: remote_type_key.normalized,
            remote_shape: remote_type_key.shape,
            repr: spec.repr,
        };

        self.custom_types.register(entry)
    }
}

pub fn registry_for_current_crate() -> syn::Result<CustomTypeRegistry> {
    Ok(CrateIndex::for_current_crate()?.custom_types().clone())
}

pub(super) fn build_custom_type_registry(
    sources: &[IndexedSource],
) -> syn::Result<CustomTypeRegistry> {
    let mut registry = CustomTypeRegistry::default();
    sources
        .iter()
        .try_for_each(|source| collect_source_custom_types(source, &mut registry))?;

    Ok(registry)
}

fn collect_source_custom_types(
    source: &IndexedSource,
    registry: &mut CustomTypeRegistry,
) -> syn::Result<()> {
    source.modules().iter().try_for_each(|source_module| {
        let mut collector = CustomTypeCollector {
            root_path: source.root_path().to_vec(),
            module_path: source
                .module_path(source_module)
                .iter()
                .map(|segment| syn::Ident::new(segment, Span::call_site()))
                .collect(),
            custom_types: registry,
        };

        source_module
            .items()
            .iter()
            .try_for_each(|item| collector.collect_item(item))
    })
}

pub fn contains_custom_types(ty: &syn::Type, registry: &CustomTypeRegistry) -> bool {
    if registry.lookup(ty).is_some() {
        return true;
    }

    match ty {
        syn::Type::Reference(reference) => contains_custom_types(reference.elem.as_ref(), registry),
        _ => match ContainerTypeDescriptor::from_type(ty) {
            ContainerTypeDescriptor::Vec(inner) | ContainerTypeDescriptor::Option(inner) => {
                contains_custom_types(inner, registry)
            }
            ContainerTypeDescriptor::Result { ok, err } => {
                contains_custom_types(ok, registry) || contains_custom_types(err, registry)
            }
            ContainerTypeDescriptor::Other => false,
        },
    }
}

pub fn wire_type_for(ty: &syn::Type, registry: &CustomTypeRegistry) -> syn::Type {
    registry
        .lookup(ty)
        .and_then(|entry| entry.repr_type().ok())
        .or_else(|| match ContainerTypeDescriptor::from_type(ty) {
            ContainerTypeDescriptor::Vec(inner) => {
                let inner_wire = wire_type_for(inner, registry);
                Some(syn::parse_quote!(Vec<#inner_wire>))
            }
            ContainerTypeDescriptor::Option(inner) => {
                let inner_wire = wire_type_for(inner, registry);
                Some(syn::parse_quote!(Option<#inner_wire>))
            }
            ContainerTypeDescriptor::Result { ok, err } => {
                let ok_wire = wire_type_for(ok, registry);
                let err_wire = wire_type_for(err, registry);
                Some(syn::parse_quote!(Result<#ok_wire, #err_wire>))
            }
            ContainerTypeDescriptor::Other => None,
        })
        .unwrap_or_else(|| ty.clone())
}

pub fn to_wire_expr_owned(
    ty: &syn::Type,
    registry: &CustomTypeRegistry,
    value_ident: &syn::Ident,
) -> proc_macro2::TokenStream {
    if let Some(entry) = registry.lookup(ty) {
        let into_fn = entry.to_fn_path();
        return quote! { #into_fn(&#value_ident) };
    }

    match ContainerTypeDescriptor::from_type(ty) {
        ContainerTypeDescriptor::Vec(inner) => {
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = to_wire_expr_owned(inner, registry, &inner_ident);
            quote! {
                #value_ident
                    .into_iter()
                    .map(|#inner_ident| #inner_expr)
                    .collect::<Vec<_>>()
            }
        }
        ContainerTypeDescriptor::Option(inner) => {
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = to_wire_expr_owned(inner, registry, &inner_ident);
            quote! { #value_ident.map(|#inner_ident| #inner_expr) }
        }
        ContainerTypeDescriptor::Result { ok, err } => {
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
        ContainerTypeDescriptor::Other => quote! { #value_ident },
    }
}

pub fn from_wire_expr_owned(
    ty: &syn::Type,
    registry: &CustomTypeRegistry,
    value_ident: &syn::Ident,
) -> proc_macro2::TokenStream {
    if let Some(entry) = registry.lookup(ty) {
        let try_from_fn = entry.try_from_fn_path();
        let error_message = format!("{}: custom type conversion failed", entry.name);
        return quote! { #try_from_fn(#value_ident).expect(#error_message) };
    }

    match ContainerTypeDescriptor::from_type(ty) {
        ContainerTypeDescriptor::Vec(inner) => {
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = from_wire_expr_owned(inner, registry, &inner_ident);
            quote! {
                #value_ident
                    .into_iter()
                    .map(|#inner_ident| #inner_expr)
                    .collect::<Vec<_>>()
            }
        }
        ContainerTypeDescriptor::Option(inner) => {
            let inner_ident = syn::Ident::new("__boltffi_item", Span::call_site());
            let inner_expr = from_wire_expr_owned(inner, registry, &inner_ident);
            quote! { #value_ident.map(|#inner_ident| #inner_expr) }
        }
        ContainerTypeDescriptor::Result { ok, err } => {
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
        ContainerTypeDescriptor::Other => quote! { #value_ident },
    }
}

impl RemoteTypeKey {
    fn from_type(rust_type: &syn::Type) -> Self {
        Self {
            normalized: quote::quote!(#rust_type).to_string().replace(' ', ""),
            shape: Self::shape_key(rust_type),
            last_segment: Self::last_segment(rust_type),
        }
    }

    fn last_segment(rust_type: &syn::Type) -> Option<String> {
        let syn::Type::Path(type_path) = rust_type else {
            return None;
        };
        type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
    }

    fn shape_key(rust_type: &syn::Type) -> String {
        match rust_type {
            syn::Type::Reference(reference) => Self::shape_key(reference.elem.as_ref()),
            syn::Type::Path(type_path) => type_path
                .path
                .segments
                .last()
                .map(Self::shape_key_for_segment)
                .unwrap_or_else(|| quote::quote!(#rust_type).to_string().replace(' ', "")),
            _ => quote::quote!(#rust_type).to_string().replace(' ', ""),
        }
    }

    fn shape_key_for_segment(segment: &syn::PathSegment) -> String {
        let ident = segment.ident.to_string();
        let arguments = match &segment.arguments {
            syn::PathArguments::AngleBracketed(arguments) => arguments
                .args
                .iter()
                .filter_map(|argument| match argument {
                    syn::GenericArgument::Type(inner_type) => Some(Self::shape_key(inner_type)),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };

        if arguments.is_empty() {
            ident
        } else {
            format!("{}<{}>", ident, arguments.join(","))
        }
    }
}

impl<'a> ContainerTypeDescriptor<'a> {
    fn from_type(rust_type: &'a syn::Type) -> Self {
        let syn::Type::Path(type_path) = rust_type else {
            return Self::Other;
        };
        let Some(segment) = type_path.path.segments.last() else {
            return Self::Other;
        };
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return Self::Other;
        };

        match segment.ident.to_string().as_str() {
            "Vec" => arguments
                .args
                .first()
                .and_then(Self::type_argument)
                .map(Self::Vec)
                .unwrap_or(Self::Other),
            "Option" => arguments
                .args
                .first()
                .and_then(Self::type_argument)
                .map(Self::Option)
                .unwrap_or(Self::Other),
            "Result" => match (
                arguments.args.first().and_then(Self::type_argument),
                arguments.args.iter().nth(1).and_then(Self::type_argument),
            ) {
                (Some(ok), Some(err)) => Self::Result { ok, err },
                _ => Self::Other,
            },
            _ => Self::Other,
        }
    }

    fn type_argument(argument: &'a syn::GenericArgument) -> Option<&'a syn::Type> {
        match argument {
            syn::GenericArgument::Type(rust_type) => Some(rust_type),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_registers_inherited_visibility_custom_type_macro() {
        let item_macro: syn::ItemMacro = syn::parse_quote! {
            custom_type!(
                UtcDateTime,
                remote = DateTime<Utc>,
                repr = i64,
                into_ffi = |dt: &DateTime<Utc>| dt.timestamp_millis(),
                try_from_ffi = |millis: i64| {
                    DateTime::from_timestamp_millis(millis).ok_or(CustomTypeConversionError)
                },
            );
        };
        let mut registry = CustomTypeRegistry::default();
        CustomTypeCollector {
            root_path: Vec::new(),
            module_path: Vec::new(),
            custom_types: &mut registry,
        }
        .collect_item_macro(&item_macro)
        .unwrap();

        assert!(registry.lookup(&syn::parse_quote!(DateTime<Utc>)).is_some());
    }
}
