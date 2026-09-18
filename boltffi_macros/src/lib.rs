use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

mod custom;
mod data;
mod expansion;
mod interned_string;

#[proc_macro_derive(FfiType)]
pub fn derive_ffi_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let has_repr_c = input.attrs.iter().any(|attribute| {
        attribute.path().is_ident("repr")
            && attribute
                .parse_args::<syn::Ident>()
                .is_ok_and(|identifier| identifier == "C")
    });
    if has_repr_c {
        TokenStream::new()
    } else {
        syn::Error::new_spanned(&input, "FfiType requires #[repr(C)]")
            .to_compile_error()
            .into()
    }
}

#[proc_macro_attribute]
pub fn ffi_stream(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn custom_ffi(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand(item)
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
pub fn data(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if attribute.to_string().trim() == "impl" {
        expand(item)
    } else {
        expand_data(data::repr::materialize(item))
    }
}

#[proc_macro_attribute]
pub fn error(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_data(data::repr::materialize(item))
}

#[proc_macro_attribute]
pub fn export(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    match syn::parse::<syn::Item>(item.clone()) {
        Ok(syn::Item::Const(_) | syn::Item::Fn(_) | syn::Item::Impl(_) | syn::Item::Trait(_)) => {
            expand(item)
        }
        Ok(item) => syn::Error::new_spanned(
            item,
            "export can only be applied to const, fn, impl, or trait",
        )
        .to_compile_error()
        .into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn skip(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn name(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn default(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn transparent(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

fn expand(item: TokenStream) -> TokenStream {
    match expansion::build::item() {
        expansion::build::Item::Preserve => strip_boltffi_attrs(item),
        expansion::build::Item::Tokens(tokens) => {
            let item = proc_macro2::TokenStream::from(strip_boltffi_attrs(item));
            TokenStream::from(quote! {
                #item
                mod __boltffi_expansion {
                    use crate::*;

                    #tokens
                }
            })
        }
        expansion::build::Item::Error(tokens) => TokenStream::from(tokens),
    }
}

fn expand_data(item: TokenStream) -> TokenStream {
    let declaration = match data::scope::Declaration::from_macro_input(&item) {
        Ok(declaration) => declaration,
        Err(error) => return error.to_compile_error().into(),
    };
    match expansion::build::data(&declaration) {
        expansion::build::DataItem::Tokens(expansion) => {
            let item = proc_macro2::TokenStream::from(strip_boltffi_attrs(item));
            let runtime = expansion.runtime();
            let root = expansion.root().map(|root| {
                quote! {
                    mod __boltffi_expansion {
                        use crate::*;

                        #root
                    }
                }
            });
            TokenStream::from(quote! {
                #item
                #runtime
                #root
            })
        }
        expansion::build::DataItem::Error(tokens) => TokenStream::from(tokens),
    }
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

fn strip_attrs(attributes: &mut Vec<syn::Attribute>) {
    attributes.retain(|attribute| !is_boltffi_helper_attr(attribute));
}

fn is_boltffi_helper_attr(attribute: &syn::Attribute) -> bool {
    let path = attribute.path();
    if !is_boltffi_helper_path(path) {
        return false;
    }
    match path.segments.last().map(|segment| &segment.ident) {
        Some(identifier)
            if identifier == "skip"
                || identifier == "name"
                || identifier == "ffi_stream"
                || identifier == "transparent" =>
        {
            true
        }
        Some(identifier) if identifier == "default" => {
            path.segments.len() == 2 || matches!(attribute.meta, syn::Meta::List(_))
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
