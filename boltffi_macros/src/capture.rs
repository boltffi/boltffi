//! Per-invocation capture: every `#[data]`/`#[export]` expansion appends its own source
//! record, so binding discovery reads what the compiler compiled instead of re-scanning
//! source. Invocations the capture cannot describe yet emit an unsupported marker, which
//! makes bindgen fall back to the legacy path rather than ship a partial contract.

use boltffi_binding::SourceFragment;
use boltffi_scan::{
    capture_class, capture_constant, capture_enum, capture_function, capture_methods,
    capture_struct, capture_trait,
};
use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};

/// How an impl block relates to type identity at this entry point.
pub(crate) enum ImplCapture {
    /// `#[export] impl` declares a class; the self type gets its identity here.
    Class,
    /// `#[data(impl)]` adds methods to a type whose identity the `#[data]` site owns.
    Methods,
}

pub(crate) fn item_tokens(item: proc_macro::TokenStream, impl_capture: ImplCapture) -> TokenStream {
    let Ok(item) = syn::parse::<syn::Item>(item) else {
        return TokenStream::new();
    };
    match &item {
        syn::Item::Struct(item) => match capture_struct(item) {
            Ok(captured) => data_tokens(
                &item.ident,
                SourceFragment::Record(captured.def),
                &captured.slots,
            ),
            Err(error) => data_unsupported_tokens(&item.ident, &error.to_string()),
        },
        syn::Item::Enum(item) => match capture_enum(item) {
            Ok(captured) => data_tokens(
                &item.ident,
                SourceFragment::Enum(captured.def),
                &captured.slots,
            ),
            Err(error) => data_unsupported_tokens(&item.ident, &error.to_string()),
        },
        syn::Item::Fn(item) => match capture_function(item) {
            Ok(captured) => record_tokens(&SourceFragment::Function(captured.def), &captured.slots),
            Err(error) => unsupported_tokens(&item.sig.ident.to_string(), &error.to_string()),
        },
        syn::Item::Trait(item) => match capture_trait(item) {
            Ok(captured) => record_tokens(&SourceFragment::Trait(captured.def), &captured.slots),
            Err(error) => unsupported_tokens(&item.ident.to_string(), &error.to_string()),
        },
        syn::Item::Impl(item) => {
            let self_ty = &*item.self_ty;
            let name = type_leaf_name(self_ty).unwrap_or_else(|| "impl".to_owned());
            match impl_capture {
                ImplCapture::Class => {
                    let identity = local_identity_tokens(self_ty, &name);
                    let record = if has_stream_methods(item) {
                        unsupported_tokens(
                            &name,
                            "stream methods are not captured per-invocation yet",
                        )
                    } else {
                        match capture_class(item) {
                            Ok(captured) => {
                                record_tokens(&SourceFragment::Class(captured.def), &captured.slots)
                            }
                            Err(error) => unsupported_tokens(&name, &error.to_string()),
                        }
                    };
                    quote! {
                        #identity
                        #record
                    }
                }
                ImplCapture::Methods => match capture_methods(item) {
                    Ok(captured) => {
                        let fragment = SourceFragment::Methods {
                            target: captured.target,
                            spelling: captured.spelling,
                            methods: captured.methods,
                        };
                        record_tokens(&fragment, &captured.slots)
                    }
                    Err(error) => unsupported_tokens(&name, &error.to_string()),
                },
            }
        }
        syn::Item::Const(item) => match capture_constant(item) {
            Ok(captured) => record_tokens(&SourceFragment::Constant(captured.def), &captured.slots),
            Err(error) => unsupported_tokens(&item.ident.to_string(), &error.to_string()),
        },
        _ => TokenStream::new(),
    }
}

fn has_stream_methods(item: &syn::ItemImpl) -> bool {
    item.items.iter().any(|member| {
        matches!(
            member,
            syn::ImplItem::Fn(function) if function.attrs.iter().any(|attr| {
                attr.path()
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "ffi_stream")
            })
        )
    })
}

fn data_unsupported_tokens(ident: &syn::Ident, reason: &str) -> TokenStream {
    let identity = local_identity_tokens(ident, &ident.to_string());
    let unsupported = unsupported_tokens(&ident.to_string(), reason);
    quote! {
        #identity
        #unsupported
    }
}

pub(crate) fn unsupported_tokens(name: &str, reason: &str) -> TokenStream {
    record_tokens(
        &SourceFragment::Unsupported {
            name: name.to_owned(),
            reason: reason.to_owned(),
        },
        &[],
    )
}

pub(crate) fn scaffolding_tokens() -> TokenStream {
    quote! {
        #[doc(hidden)]
        pub enum __BoltffiTag {}
    }
}

fn data_tokens(ident: &syn::Ident, fragment: SourceFragment, slots: &[syn::Path]) -> TokenStream {
    let name = ident.to_string();
    let record = record_tokens(&fragment, slots);
    quote! {
        const _: () = {
            impl<Tag> ::boltffi::__private::capture::TypeInfo<Tag> for #ident {
                const MODULE: &'static str = ::core::module_path!();
                const NAME: &'static str = #name;
            }

            impl<Tag> ::boltffi::__private::capture::TypeDesc<Tag> for #ident {
                const DESC: ::boltffi::__private::capture::DescBuf =
                    ::boltffi::__private::capture::DescBuf::named(
                        <#ident as ::boltffi::__private::capture::TypeInfo<Tag>>::MODULE,
                        <#ident as ::boltffi::__private::capture::TypeInfo<Tag>>::NAME,
                    );
            }
        };
        #record
    }
}

fn record_tokens(fragment: &SourceFragment, slots: &[syn::Path]) -> TokenStream {
    let json = match serde_json::to_vec(fragment) {
        Ok(json) => Literal::byte_string(&json),
        Err(_) => return TokenStream::new(),
    };
    let descs = slots
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let ident = format_ident!("DESC_{index}");
            quote! {
                const #ident: &'static ::boltffi::__private::capture::DescBuf =
                    &<#path as ::boltffi::__private::capture::TypeDesc<crate::__BoltffiTag>>::DESC;
            }
        })
        .collect::<Vec<_>>();
    let desc_refs = (0..slots.len())
        .map(|index| {
            let ident = format_ident!("DESC_{index}");
            quote! { #ident.as_str() }
        })
        .collect::<Vec<_>>();

    quote! {
        const _: () = {
            #(#descs)*
            const SLOTS: &[&str] = &[#(#desc_refs),*];
            const JSON: &[u8] = #json;
            const LEN: usize = ::boltffi::__private::capture::record_len(
                ::core::env!("CARGO_PKG_NAME"),
                ::core::env!("CARGO_PKG_VERSION"),
                ::core::module_path!(),
                SLOTS,
                JSON,
            );
            #[cfg_attr(
                any(target_os = "macos", target_os = "ios"),
                unsafe(link_section = "__DATA,__boltffisrc")
            )]
            #[cfg_attr(
                not(any(target_os = "macos", target_os = "ios")),
                unsafe(link_section = ".boltffisrc")
            )]
            #[used]
            static RECORD: [u8; LEN] = ::boltffi::__private::capture::record(
                ::core::env!("CARGO_PKG_NAME"),
                ::core::env!("CARGO_PKG_VERSION"),
                ::core::module_path!(),
                SLOTS,
                JSON,
            );
        };
    }
}

pub(crate) fn custom_ffi_tokens(item: proc_macro::TokenStream) -> TokenStream {
    let Ok(item) = syn::parse::<syn::ItemImpl>(item) else {
        return unsupported_tokens(
            "custom_ffi",
            "custom FFI impls are not captured per-invocation yet",
        );
    };
    let self_ty = &*item.self_ty;
    let name = type_leaf_name(self_ty).unwrap_or_else(|| "custom_ffi".to_owned());
    let identity = local_identity_tokens(self_ty, &name);
    let unsupported = unsupported_tokens(
        &name,
        "custom FFI impls are not captured per-invocation yet",
    );
    quote! {
        #identity
        #unsupported
    }
}

pub(crate) fn custom_type_tokens(item: proc_macro::TokenStream) -> TokenStream {
    let Ok(spec) = crate::custom::r#type::parse_spec(item) else {
        return unsupported_tokens(
            "custom_type!",
            "custom types are not captured per-invocation yet",
        );
    };
    let remote = &spec.remote;
    let name = type_leaf_name(remote).unwrap_or_else(|| spec.name.to_string());
    let module = type_module_spelling(remote);
    let unsupported = unsupported_tokens(&name, "custom types are not captured per-invocation yet");
    quote! {
        const _: () = {
            impl ::boltffi::__private::capture::TypeInfo<crate::__BoltffiTag> for #remote {
                const MODULE: &'static str = #module;
                const NAME: &'static str = #name;
            }

            impl ::boltffi::__private::capture::TypeDesc<crate::__BoltffiTag> for #remote {
                const DESC: ::boltffi::__private::capture::DescBuf =
                    ::boltffi::__private::capture::DescBuf::named(#module, #name);
            }
        };
        #unsupported
    }
}

fn local_identity_tokens<T: quote::ToTokens>(self_ty: &T, name: &str) -> TokenStream {
    quote! {
        const _: () = {
            impl<Tag> ::boltffi::__private::capture::TypeInfo<Tag> for #self_ty {
                const MODULE: &'static str = ::core::module_path!();
                const NAME: &'static str = #name;
            }

            impl<Tag> ::boltffi::__private::capture::TypeDesc<Tag> for #self_ty {
                const DESC: ::boltffi::__private::capture::DescBuf =
                    ::boltffi::__private::capture::DescBuf::named(
                        <#self_ty as ::boltffi::__private::capture::TypeInfo<Tag>>::MODULE,
                        <#self_ty as ::boltffi::__private::capture::TypeInfo<Tag>>::NAME,
                    );
            }
        };
    }
}

fn type_leaf_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        _ => None,
    }
}

fn type_module_spelling(ty: &syn::Type) -> String {
    let syn::Type::Path(path) = ty else {
        return String::new();
    };
    let segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    segments[..segments.len().saturating_sub(1)].join("::")
}
