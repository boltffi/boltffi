use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, ItemFn, parse_macro_input};

mod callbacks;
mod custom;
mod data;
mod experimental;
mod exports;
mod index;
mod interned_string;
mod lowering;
mod safety;

#[proc_macro_derive(FfiType)]
pub fn derive_ffi_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let has_repr_c = input.attrs.iter().any(|attr| {
        attr.path().is_ident("repr")
            && attr
                .parse_args::<syn::Ident>()
                .map(|id| id == "C")
                .unwrap_or(false)
    });

    if !has_repr_c {
        return syn::Error::new_spanned(&input, "FfiType requires #[repr(C)]")
            .to_compile_error()
            .into();
    }

    TokenStream::from(quote! {})
}

fn expand_or_experimental(
    item: TokenStream,
    legacy: impl FnOnce(TokenStream) -> TokenStream,
    dependency: DependencyExpansion,
) -> TokenStream {
    match experimental::expansion_build::item() {
        experimental::expansion_build::Item::Inactive => {}
        experimental::expansion_build::Item::Dependency => {
            return match dependency {
                DependencyExpansion::Legacy => legacy(item),
                DependencyExpansion::Preserve => strip_boltffi_attrs(item),
            };
        }
        experimental::expansion_build::Item::Preserve => return strip_boltffi_attrs(item),
        experimental::expansion_build::Item::Tokens(tokens) => {
            let item = proc_macro2::TokenStream::from(strip_boltffi_attrs(item));
            return TokenStream::from(quote! {
                #item
                mod __boltffi_ir_expansion {
                    use crate::*;

                    #tokens
                }
            });
        }
        experimental::expansion_build::Item::Error(tokens) => return TokenStream::from(tokens),
    }

    match experimental::metadata_build::item() {
        experimental::metadata_build::Item::Inactive => legacy(item),
        experimental::metadata_build::Item::Preserve => strip_boltffi_attrs(item),
        experimental::metadata_build::Item::Tokens(tokens) => {
            let item = proc_macro2::TokenStream::from(strip_boltffi_attrs(item));
            TokenStream::from(quote! {
                #item
                #tokens
            })
        }
        experimental::metadata_build::Item::Error(tokens) => TokenStream::from(tokens),
    }
}

enum DependencyExpansion {
    Legacy,
    Preserve,
}

fn strip_boltffi_attrs(item: TokenStream) -> TokenStream {
    let Ok(mut item) = syn::parse::<syn::Item>(item.clone()) else {
        return item;
    };
    strip_item_attrs(&mut item);
    TokenStream::from(quote!(#item))
}

fn strip_item_attrs(item: &mut syn::Item) {
    match item {
        syn::Item::Const(item) => strip_attrs(&mut item.attrs),
        syn::Item::Enum(item) => {
            strip_attrs(&mut item.attrs);
            item.variants.iter_mut().for_each(|variant| {
                strip_attrs(&mut variant.attrs);
                strip_fields_attrs(&mut variant.fields);
            });
        }
        syn::Item::Fn(item) => {
            strip_attrs(&mut item.attrs);
            strip_signature_attrs(&mut item.sig);
        }
        syn::Item::Impl(item) => {
            strip_attrs(&mut item.attrs);
            item.items.iter_mut().for_each(strip_impl_item_attrs);
        }
        syn::Item::Struct(item) => {
            strip_attrs(&mut item.attrs);
            strip_fields_attrs(&mut item.fields);
        }
        syn::Item::Trait(item) => {
            strip_attrs(&mut item.attrs);
            item.items.iter_mut().for_each(strip_trait_item_attrs);
        }
        _ => {}
    }
}

fn strip_fields_attrs(fields: &mut syn::Fields) {
    match fields {
        syn::Fields::Named(fields) => fields
            .named
            .iter_mut()
            .for_each(|field| strip_attrs(&mut field.attrs)),
        syn::Fields::Unnamed(fields) => fields
            .unnamed
            .iter_mut()
            .for_each(|field| strip_attrs(&mut field.attrs)),
        syn::Fields::Unit => {}
    }
}

fn strip_impl_item_attrs(item: &mut syn::ImplItem) {
    match item {
        syn::ImplItem::Const(item) => strip_attrs(&mut item.attrs),
        syn::ImplItem::Fn(item) => {
            strip_attrs(&mut item.attrs);
            strip_signature_attrs(&mut item.sig);
        }
        syn::ImplItem::Type(item) => strip_attrs(&mut item.attrs),
        _ => {}
    }
}

fn strip_trait_item_attrs(item: &mut syn::TraitItem) {
    match item {
        syn::TraitItem::Const(item) => strip_attrs(&mut item.attrs),
        syn::TraitItem::Fn(item) => {
            strip_attrs(&mut item.attrs);
            strip_signature_attrs(&mut item.sig);
        }
        syn::TraitItem::Type(item) => strip_attrs(&mut item.attrs),
        _ => {}
    }
}

fn strip_signature_attrs(signature: &mut syn::Signature) {
    signature.inputs.iter_mut().for_each(|input| match input {
        syn::FnArg::Receiver(receiver) => strip_attrs(&mut receiver.attrs),
        syn::FnArg::Typed(argument) => strip_attrs(&mut argument.attrs),
    });
}

fn strip_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !is_boltffi_helper_attr(attr));
}

fn is_boltffi_helper_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    if !is_boltffi_helper_path(path) {
        return false;
    }
    match path.segments.last().map(|segment| &segment.ident) {
        Some(ident)
            if ident == "skip"
                || ident == "name"
                || ident == "ffi_stream"
                || ident == "borrowed" =>
        {
            true
        }
        Some(ident) if ident == "default" => {
            path.segments.len() == 2 || matches!(attr.meta, syn::Meta::List(_))
        }
        _ => false,
    }
}

fn is_boltffi_helper_path(path: &syn::Path) -> bool {
    match path.segments.len() {
        1 => true,
        2 => path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "boltffi"),
        _ => false,
    }
}

#[proc_macro_attribute]
pub fn ffi_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand_or_experimental(
        item,
        exports::function::ffi_export_impl,
        DependencyExpansion::Preserve,
    )
}

#[proc_macro_attribute]
pub fn ffi_stream(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn ffi_trait(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand_or_experimental(
        item,
        callbacks::trait_export::ffi_trait_impl,
        DependencyExpansion::Preserve,
    )
}

#[proc_macro_attribute]
pub fn custom_ffi(_attr: TokenStream, item: TokenStream) -> TokenStream {
    custom::ffi::custom_ffi_impl(item)
}

#[proc_macro]
pub fn custom_type(item: TokenStream) -> TokenStream {
    custom::r#type::custom_type_impl(item)
}

#[proc_macro]
pub fn interned_string_pool(item: TokenStream) -> TokenStream {
    interned_string::interned_string_pool_impl(item)
}

#[proc_macro_attribute]
pub fn data(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_str = attr.to_string();
    match attr_str.trim() {
        "impl" => expand_or_experimental(
            item,
            data::expansion::data_impl_block,
            DependencyExpansion::Legacy,
        ),
        "opaque" => expand_or_experimental(
            item,
            data::expansion::data_opaque_impl,
            DependencyExpansion::Legacy,
        ),
        _ => expand_or_experimental(
            data::repr::materialize(item),
            data::expansion::data_impl,
            DependencyExpansion::Legacy,
        ),
    }
}

#[proc_macro_attribute]
pub fn error(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand_or_experimental(
        data::repr::materialize(item),
        data::expansion::data_impl,
        DependencyExpansion::Legacy,
    )
}

#[proc_macro_derive(Data)]
pub fn derive_data(input: TokenStream) -> TokenStream {
    data::expansion::derive_data_impl(input)
}

#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(item_const) = syn::parse::<syn::ItemConst>(item_clone.clone()) {
        return expand_or_experimental(
            TokenStream::from(quote!(#item_const)),
            strip_boltffi_attrs,
            DependencyExpansion::Preserve,
        );
    }

    if let Ok(item_fn) = syn::parse::<ItemFn>(item_clone.clone()) {
        return expand_or_experimental(
            TokenStream::from(quote!(#item_fn)),
            exports::function::ffi_export_impl,
            DependencyExpansion::Preserve,
        );
    }

    if let Ok(item_impl) = syn::parse::<syn::ItemImpl>(item_clone.clone()) {
        return expand_or_experimental(
            TokenStream::from(quote!(#item_impl)),
            |item| exports::methods::export_impl(attr, item),
            DependencyExpansion::Preserve,
        );
    }

    if let Ok(item_trait) = syn::parse::<syn::ItemTrait>(item_clone) {
        return ffi_trait(attr, TokenStream::from(quote!(#item_trait)));
    }

    syn::Error::new_spanned(
        proc_macro2::TokenStream::from(item),
        "export can only be applied to const, fn, impl, or trait",
    )
    .to_compile_error()
    .into()
}

#[proc_macro_attribute]
pub fn skip(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn name(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn default(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
