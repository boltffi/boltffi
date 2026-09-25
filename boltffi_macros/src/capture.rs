//! Per-invocation capture: every `#[data]`/`#[export]` expansion appends its own source
//! record, so binding discovery reads what the compiler compiled instead of re-scanning
//! source. Invocations the capture cannot describe yet emit an unsupported marker, which
//! makes bindgen fall back to the legacy path rather than ship a partial contract.

use boltffi_binding::SourceFragment;
use boltffi_scan::{
    capture_class, capture_class_constants, capture_constant, capture_enum, capture_error_enum,
    capture_error_struct, capture_function, capture_methods, capture_streams, capture_struct,
    capture_trait,
};
use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};

/// Resolves the facade crate's path at the invocation site, honoring renames.
fn facade() -> TokenStream {
    match proc_macro_crate::crate_name("boltffi") {
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote! { ::#name }
        }
        _ => quote! { ::boltffi },
    }
}

/// How an impl block relates to type identity at this entry point.
pub(crate) enum ImplCapture {
    /// `#[export] impl` declares a class; the self type gets its identity here. Carries
    /// the export marker's argument tokens for the thread-safety choice.
    Class(TokenStream),
    /// `#[data(impl)]` adds methods to a type whose identity the `#[data]` site owns.
    Methods,
}

pub(crate) fn item_tokens(item: proc_macro::TokenStream, impl_capture: ImplCapture) -> TokenStream {
    let Ok(item) = syn::parse::<syn::Item>(item) else {
        return TokenStream::new();
    };
    let class_identity = matches!(impl_capture, ImplCapture::Class(_));
    if let Some(tokens) = conditional_capture(&item, class_identity) {
        return tokens;
    }
    if !crate::expansion::build::scanned(&scan_anchors(&item)) {
        return fallback_tokens(
            &item,
            class_identity,
            "items the source scan cannot see have no wrapper",
        );
    }
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
            Ok(captured) => {
                let identity = trait_identity_tokens(item);
                let record = record_tokens(&SourceFragment::Trait(captured.def), &captured.slots);
                quote! {
                    #identity
                    #record
                }
            }
            Err(error) => unsupported_tokens(&item.ident.to_string(), &error.to_string()),
        },
        syn::Item::Impl(item) => {
            let self_ty = &*item.self_ty;
            let name = type_leaf_name(self_ty).unwrap_or_else(|| "impl".to_owned());
            match impl_capture {
                ImplCapture::Class(marker_args) => {
                    let identity = local_identity_tokens(self_ty, &name);
                    let record = match capture_class(item, marker_args) {
                        Ok(captured) => {
                            record_tokens(&SourceFragment::Class(captured.def), &captured.slots)
                        }
                        Err(error) => unsupported_tokens(&name, &error.to_string()),
                    };
                    let streams = match capture_streams(item) {
                        Ok(captured) => captured
                            .def
                            .into_iter()
                            .map(|stream| {
                                record_tokens(&SourceFragment::Stream(stream), &captured.slots)
                            })
                            .collect::<TokenStream>(),
                        Err(error) => unsupported_tokens(&name, &error.to_string()),
                    };
                    let constants = match capture_class_constants(item) {
                        Ok(captured) => captured
                            .def
                            .into_iter()
                            .map(|constant| {
                                record_tokens(&SourceFragment::Constant(constant), &captured.slots)
                            })
                            .collect::<TokenStream>(),
                        Err(error) => unsupported_tokens(&name, &error.to_string()),
                    };
                    quote! {
                        #identity
                        #record
                        #streams
                        #constants
                    }
                }
                ImplCapture::Methods => match capture_methods(item) {
                    Ok(captured) => {
                        let fragment = SourceFragment::Methods {
                            target: captured.target,
                            spelling: captured.spelling,
                            methods: captured.methods,
                            constants: captured.constants,
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

pub(crate) fn interned_string_pool_tokens(item: proc_macro::TokenStream) -> TokenStream {
    if proc_macro_crate::crate_name("boltffi").is_err() {
        return TokenStream::new();
    }
    match boltffi_scan::capture_interned_string_pool(proc_macro2::TokenStream::from(item)) {
        Ok(pool) => {
            let ident = format_ident!("{}", pool.name);
            let identity = local_identity_tokens(&ident, &pool.name);
            let record = record_tokens(
                &SourceFragment::InternedStringPool {
                    id: format!("$self::{}", pool.name),
                    values: pool.values,
                },
                &[],
            );
            quote! {
                #identity
                #record
            }
        }
        Err(error) => unsupported_tokens("interned_string_pool", &error.to_string()),
    }
}

pub(crate) fn error_item_tokens(item: proc_macro::TokenStream) -> TokenStream {
    let Ok(item) = syn::parse::<syn::Item>(item) else {
        return TokenStream::new();
    };
    if let Some(tokens) = conditional_capture(&item, false) {
        return tokens;
    }
    match &item {
        syn::Item::Struct(item) => match capture_error_struct(item) {
            Ok(captured) => data_tokens(
                &item.ident,
                SourceFragment::Record(captured.def),
                &captured.slots,
            ),
            Err(error) => data_unsupported_tokens(&item.ident, &error.to_string()),
        },
        syn::Item::Enum(item) => match capture_error_enum(item) {
            Ok(captured) => data_tokens(
                &item.ident,
                SourceFragment::Enum(captured.def),
                &captured.slots,
            ),
            Err(error) => data_unsupported_tokens(&item.ident, &error.to_string()),
        },
        _ => TokenStream::new(),
    }
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

fn data_tokens(
    ident: &syn::Ident,
    fragment: SourceFragment,
    slots: &[boltffi_scan::SlotSource],
) -> TokenStream {
    let name = ident.to_string();
    let record = record_tokens(&fragment, slots);
    let facade = facade();
    quote! {
        const _: () = {
            impl<Tag> #facade::__private::capture::TypeInfo<Tag> for #ident {
                const MODULE: &'static str = ::core::module_path!();
                const NAME: &'static str = #name;
            }

            impl<Tag> #facade::__private::capture::TypeDesc<Tag> for #ident {
                const DESC: #facade::__private::capture::DescBuf =
                    #facade::__private::capture::DescBuf::named(
                        <#ident as #facade::__private::capture::TypeInfo<Tag>>::MODULE,
                        <#ident as #facade::__private::capture::TypeInfo<Tag>>::NAME,
                    );
            }
        };
        #record
    }
}

fn record_tokens(fragment: &SourceFragment, slots: &[boltffi_scan::SlotSource]) -> TokenStream {
    let json = match serde_json::to_vec(fragment) {
        Ok(json) => Literal::byte_string(&json),
        Err(_) => return TokenStream::new(),
    };
    let facade = facade();
    let descs = slots
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let ident = format_ident!("DESC_{index}");
            let desc = match source {
                boltffi_scan::SlotSource::Type(ty) => quote! {
                    &<#ty as #facade::__private::capture::TypeDesc<crate::__BoltffiTag>>::DESC
                },
                boltffi_scan::SlotSource::TraitValue(path) => quote! { &#path },
            };
            quote! {
                const #ident: &'static #facade::__private::capture::DescBuf = #desc;
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
            const LEN: usize = #facade::__private::capture::record_len(
                ::core::env!("CARGO_PKG_NAME"),
                ::core::env!("CARGO_PKG_VERSION"),
                ::core::module_path!(),
                SLOTS,
                JSON,
            );
            #[cfg_attr(
                target_vendor = "apple",
                unsafe(link_section = "__DATA,__boltffisrc")
            )]
            #[cfg_attr(
                not(target_vendor = "apple"),
                unsafe(link_section = ".boltffisrc")
            )]
            #[used]
            static RECORD: [u8; LEN] = #facade::__private::capture::record(
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
        return unsupported_tokens("custom_ffi", "custom_ffi impl did not parse");
    };
    let self_ty = &*item.self_ty;
    let name = type_leaf_name(self_ty).unwrap_or_else(|| "custom_ffi".to_owned());
    let identity = local_identity_tokens(self_ty, &name);
    let record = match boltffi_scan::capture_custom_ffi(&item) {
        Ok(captured) => record_tokens(&SourceFragment::Custom(captured.def), &captured.slots),
        Err(error) => unsupported_tokens(&name, &error.to_string()),
    };
    quote! {
        #identity
        #record
    }
}

pub(crate) fn custom_type_tokens(item: proc_macro::TokenStream) -> TokenStream {
    let Ok(spec) = crate::custom::r#type::parse_spec(item.clone()) else {
        return unsupported_tokens("custom_type!", "custom_type! spec did not parse");
    };
    let remote = &spec.remote;
    let name = spec.name.to_string();
    let record = match boltffi_scan::capture_custom(proc_macro2::TokenStream::from(item)) {
        Ok(captured) => record_tokens(&SourceFragment::Custom(captured.def), &captured.slots),
        Err(error) => unsupported_tokens(&name, &error.to_string()),
    };
    let facade = facade();
    quote! {
        const _: () = {
            impl #facade::__private::capture::TypeInfo<crate::__BoltffiTag> for #remote {
                const MODULE: &'static str = ::core::module_path!();
                const NAME: &'static str = #name;
            }

            impl #facade::__private::capture::TypeDesc<crate::__BoltffiTag> for #remote {
                const DESC: #facade::__private::capture::DescBuf =
                    #facade::__private::capture::DescBuf::named(
                        ::core::module_path!(),
                        #name,
                    );
            }
        };
        #record
    }
}

/// A trait's descriptor lives in the value namespace under the trait's own
/// name, so use sites reach it through the same written path as the bound —
/// with no dyn-compatibility requirement on the trait.
fn trait_identity_tokens(item: &syn::ItemTrait) -> TokenStream {
    let vis = &item.vis;
    let ident = &item.ident;
    let name = ident.to_string();
    let facade = facade();
    quote! {
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        #vis const #ident: #facade::__private::capture::DescBuf =
            #facade::__private::capture::DescBuf::named(::core::module_path!(), #name);
    }
}

fn local_identity_tokens<T: quote::ToTokens>(self_ty: &T, name: &str) -> TokenStream {
    let facade = facade();
    quote! {
        const _: () = {
            impl<Tag> #facade::__private::capture::TypeInfo<Tag> for #self_ty {
                const MODULE: &'static str = ::core::module_path!();
                const NAME: &'static str = #name;
            }

            impl<Tag> #facade::__private::capture::TypeDesc<Tag> for #self_ty {
                const DESC: #facade::__private::capture::DescBuf =
                    #facade::__private::capture::DescBuf::named(
                        <#self_ty as #facade::__private::capture::TypeInfo<Tag>>::MODULE,
                        <#self_ty as #facade::__private::capture::TypeInfo<Tag>>::NAME,
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

// rustc has not expanded cfg attributes on members when the enclosing attribute
// macro runs. Refuse the whole fragment before projecting any conditional slots.
fn conditional_capture(item: &syn::Item, class_identity: bool) -> Option<TokenStream> {
    use syn::visit::Visit;
    #[derive(Default)]
    struct Conditional(bool);
    impl<'ast> Visit<'ast> for Conditional {
        fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
            self.0 |= attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr");
        }
    }
    let mut conditional = Conditional::default();
    conditional.visit_item(item);
    conditional.0.then(|| {
        fallback_tokens(
            item,
            class_identity,
            "conditional members require whole-crate scanning",
        )
    })
}

/// Tokens the crate-wide scan has seen when it saw `item`: a macro-generated
/// item's names point at the macro call, an included one's at another file.
fn scan_anchors(item: &syn::Item) -> Vec<proc_macro2::Span> {
    match item {
        syn::Item::Fn(item) => vec![item.sig.ident.span()],
        syn::Item::Trait(item) => vec![item.ident.span()],
        syn::Item::Const(item) => vec![item.ident.span()],
        syn::Item::Impl(item) => item
            .items
            .iter()
            .filter_map(|member| match member {
                syn::ImplItem::Fn(method) => Some(method.sig.ident.span()),
                syn::ImplItem::Const(constant) => Some(constant.ident.span()),
                _ => None,
            })
            .chain(std::iter::once(syn::spanned::Spanned::span(&*item.self_ty)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The item's identity plus an unsupported marker, so bindgen keeps the
/// scanner-backed path for this crate.
fn fallback_tokens(item: &syn::Item, class_identity: bool, reason: &str) -> TokenStream {
    let (name, identity) = match item {
        syn::Item::Struct(item) => (
            item.ident.to_string(),
            local_identity_tokens(&item.ident, &item.ident.to_string()),
        ),
        syn::Item::Enum(item) => (
            item.ident.to_string(),
            local_identity_tokens(&item.ident, &item.ident.to_string()),
        ),
        syn::Item::Trait(item) => (item.ident.to_string(), trait_identity_tokens(item)),
        syn::Item::Impl(item) if class_identity => {
            let name = type_leaf_name(&item.self_ty).unwrap_or_else(|| "impl".to_owned());
            let identity = local_identity_tokens(&item.self_ty, &name);
            (name, identity)
        }
        syn::Item::Impl(item) => (
            type_leaf_name(&item.self_ty).unwrap_or_else(|| "impl".to_owned()),
            TokenStream::new(),
        ),
        syn::Item::Fn(item) => (item.sig.ident.to_string(), TokenStream::new()),
        syn::Item::Const(item) => (item.ident.to_string(), TokenStream::new()),
        _ => ("declaration".to_owned(), TokenStream::new()),
    };
    let unsupported = unsupported_tokens(&name, reason);
    quote!(#identity #unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditional_member_capture_never_projects_disabled_type_slots() {
        let item = syn::parse_quote! {
            impl Engine {
                #[cfg(any())]
                pub fn unavailable(&self, value: UnavailableType) {}
            }
        };
        let tokens = conditional_capture(&item, true)
            .expect("must fall back")
            .to_string();
        assert!(!tokens.contains("UnavailableType"));
        assert!(tokens.contains("TypeInfo"));
    }
    #[test]
    fn conditional_fields_and_variants_require_fallback() {
        for item in [
            syn::parse_quote!(
                struct Value {
                    #[cfg(any())]
                    absent: Missing,
                }
            ),
            syn::parse_quote!(
                enum Value {
                    #[cfg(any())]
                    Absent(Missing),
                    Present,
                }
            ),
            syn::parse_quote!(
                trait Listener {
                    #[cfg_attr(feature = "x", cfg(any()))]
                    fn event(&self);
                }
            ),
        ] {
            assert!(conditional_capture(&item, false).is_some());
        }
    }
}
