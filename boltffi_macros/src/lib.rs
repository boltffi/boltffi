use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, ItemFn, parse_macro_input};

mod callback_registry;
mod callback_trait;
mod class;
mod custom_ffi;
mod custom_type;
mod custom_types;
mod data;
mod export;
mod params;
mod returns;
mod safety;
mod util;
mod wire_gen;

#[proc_macro_derive(FfiType)]
pub fn derive_ffi_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let has_repr_c = input
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("repr") && attr.parse_args::<syn::Ident>().map(|id| id == "C").unwrap_or(false));

    if !has_repr_c {
        return syn::Error::new_spanned(&input, "FfiType requires #[repr(C)]").to_compile_error().into();
    }

    TokenStream::from(quote! {})
}

#[proc_macro_attribute]
pub fn ffi_export(_attr: TokenStream, item: TokenStream) -> TokenStream {
    export::ffi_export_impl(item)
}

#[proc_macro_attribute]
pub fn ffi_async_iter(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn ffi_stream(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn ffi_class(attr: TokenStream, item: TokenStream) -> TokenStream {
    class::ffi_class_impl(attr, item)
}

#[proc_macro_attribute]
pub fn ffi_trait(_attr: TokenStream, item: TokenStream) -> TokenStream {
    callback_trait::ffi_trait_impl(item)
}

#[proc_macro_attribute]
pub fn custom_ffi(_attr: TokenStream, item: TokenStream) -> TokenStream {
    custom_ffi::custom_ffi_impl(item)
}

#[proc_macro]
pub fn custom_type(item: TokenStream) -> TokenStream {
    custom_type::custom_type_impl(item)
}

#[proc_macro_attribute]
pub fn data(_attr: TokenStream, item: TokenStream) -> TokenStream {
    data::data_impl(item)
}

#[proc_macro_attribute]
pub fn error(_attr: TokenStream, item: TokenStream) -> TokenStream {
    data::data_impl(item)
}

#[proc_macro_derive(Data)]
pub fn derive_data(input: TokenStream) -> TokenStream {
    data::derive_data_impl(input)
}

#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(item_fn) = syn::parse::<ItemFn>(item_clone.clone()) {
        return export::ffi_export_impl(TokenStream::from(quote!(#item_fn)));
    }

    if let Ok(item_impl) = syn::parse::<syn::ItemImpl>(item_clone.clone()) {
        return ffi_class(attr, TokenStream::from(quote!(#item_impl)));
    }

    if let Ok(item_trait) = syn::parse::<syn::ItemTrait>(item_clone) {
        return ffi_trait(attr, TokenStream::from(quote!(#item_trait)));
    }

    syn::Error::new_spanned(proc_macro2::TokenStream::from(item), "export can only be applied to fn, impl, or trait")
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
