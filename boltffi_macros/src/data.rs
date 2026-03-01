use crate::{custom_types, wire_gen};
use proc_macro::TokenStream;
use quote::{format_ident, quote};

pub fn data_impl(item: TokenStream) -> TokenStream {
    let item_clone = item.clone();

    if let Ok(mut item_struct) = syn::parse::<syn::ItemStruct>(item_clone.clone()) {
        let has_repr = item_struct.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            item_struct.attrs.insert(0, syn::parse_quote!(#[repr(C)]));
        }

        strip_boltffi_field_attrs(&mut item_struct.fields);

        let struct_name = &item_struct.ident;
        let free_fn_name = format_ident!("boltffi_free_buf_{}", struct_name);

        let custom_types = match custom_types::registry_for_current_crate() {
            Ok(registry) => registry,
            Err(error) => return error.to_compile_error().into(),
        };
        let wire_impls = wire_gen::generate_wire_impls(&item_struct, &custom_types);

        return TokenStream::from(quote! {
            #item_struct
            #wire_impls

            #[cfg(not(test))]
            #[unsafe(no_mangle)]
            pub extern "C" fn #free_fn_name(buf: ::boltffi::__private::FfiBuf<#struct_name>) {
                drop(buf);
            }
        });
    }

    if let Ok(mut item_enum) = syn::parse::<syn::ItemEnum>(item_clone) {
        let has_repr = item_enum.attrs.iter().any(|a| a.path().is_ident("repr"));
        if !has_repr {
            let has_data = item_enum.variants.iter().any(|v| !v.fields.is_empty());
            if has_data {
                item_enum.attrs.insert(0, syn::parse_quote!(#[repr(C, i32)]));
            } else {
                item_enum.attrs.insert(0, syn::parse_quote!(#[repr(i32)]));
            }
        }

        let custom_types = match custom_types::registry_for_current_crate() {
            Ok(registry) => registry,
            Err(error) => return error.to_compile_error().into(),
        };
        let wire_impls = wire_gen::generate_enum_wire_impls(&item_enum, &custom_types);

        return TokenStream::from(quote! {
            #item_enum
            #wire_impls
        });
    }

    syn::Error::new_spanned(proc_macro2::TokenStream::from(item), "data can only be applied to struct or enum")
        .to_compile_error()
        .into()
}

fn is_boltffi_field_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    path.segments.len() == 2 && path.segments[0].ident == "boltffi" && path.segments[1].ident == "default"
}

fn strip_boltffi_field_attrs(fields: &mut syn::Fields) {
    fields.iter_mut().for_each(|field| {
        field.attrs.retain(|attr| !is_boltffi_field_attr(attr));
    });
}

pub fn derive_data_impl(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
