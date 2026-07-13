mod enum_expansion;
mod field_attrs;
mod passable;
mod record_impl;
mod struct_expansion;
mod wire;

use proc_macro::TokenStream;
use syn::{ItemEnum, ItemStruct};

use enum_expansion::EnumDataExpansion;
pub use record_impl::data_impl_block;
use struct_expansion::StructDataExpansion;

/// Implements the stable-path expansion for `#[data(opaque)]`.
///
/// Opaque records do not participate in wire encoding. The experimental
/// bindgen expansion emits the native handle return and per-field accessors
/// from binding IR; the legacy attribute path preserves only the Rust struct.
pub fn data_opaque_impl(item: TokenStream) -> TokenStream {
    let original_tokens = proc_macro2::TokenStream::from(item.clone());

    if let Ok(mut item_struct) = syn::parse::<ItemStruct>(item) {
        use crate::data::expansion::field_attrs::BoltffiFieldAttributes;
        BoltffiFieldAttributes::strip_from_fields(&mut item_struct.fields);
        return quote::quote!(#item_struct).into();
    }

    syn::Error::new_spanned(
        original_tokens,
        "data(opaque) can only be applied to a struct",
    )
    .to_compile_error()
    .into()
}

pub fn data_impl(item: TokenStream) -> TokenStream {
    let original_tokens = proc_macro2::TokenStream::from(item.clone());

    if let Ok(item_struct) = syn::parse::<ItemStruct>(item.clone()) {
        return StructDataExpansion::new(item_struct)
            .and_then(StructDataExpansion::render)
            .unwrap_or_else(|error| error.to_compile_error())
            .into();
    }

    if let Ok(item_enum) = syn::parse::<ItemEnum>(item) {
        return EnumDataExpansion::new(item_enum)
            .and_then(EnumDataExpansion::render)
            .unwrap_or_else(|error| error.to_compile_error())
            .into();
    }

    syn::Error::new_spanned(
        original_tokens,
        "data can only be applied to struct or enum",
    )
    .to_compile_error()
    .into()
}

pub fn derive_data_impl(input: TokenStream) -> TokenStream {
    let derive_input = match syn::parse::<syn::DeriveInput>(input) {
        Ok(derive_input) => derive_input,
        Err(error) => return error.to_compile_error().into(),
    };

    syn::Error::new_spanned(
        derive_input.ident,
        "#[derive(Data)] is not supported; use #[data] or #[error] instead",
    )
    .to_compile_error()
    .into()
}
