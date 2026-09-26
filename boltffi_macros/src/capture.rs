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
pub(crate) fn facade() -> TokenStream {
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

/// What one invocation declares, as fragments paired with their written references.
pub(crate) type Fragments = Vec<(SourceFragment, Vec<boltffi_scan::SlotSource>)>;

/// The fragments `item` contributes, the same ones its source record carries.
pub(crate) fn fragments(
    item: &syn::Item,
    impl_capture: &ImplCapture,
    error: bool,
) -> Result<Fragments, String> {
    let failed = |error: boltffi_scan::ScanError| error.to_string();
    match item {
        syn::Item::Struct(item) if error => capture_error_struct(item)
            .map(|captured| vec![(SourceFragment::Record(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Enum(item) if error => capture_error_enum(item)
            .map(|captured| vec![(SourceFragment::Enum(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Struct(item) => capture_struct(item)
            .map(|captured| vec![(SourceFragment::Record(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Enum(item) => capture_enum(item)
            .map(|captured| vec![(SourceFragment::Enum(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Fn(item) => capture_function(item)
            .map(|captured| vec![(SourceFragment::Function(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Trait(item) => capture_trait(item)
            .map(|captured| vec![(SourceFragment::Trait(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Const(item) => capture_constant(item)
            .map(|captured| vec![(SourceFragment::Constant(captured.def), captured.slots)])
            .map_err(failed),
        syn::Item::Impl(item) => {
            match impl_capture {
                ImplCapture::Class(marker_args) => {
                    let class = capture_class(item, marker_args.clone()).map_err(failed)?;
                    let streams = capture_streams(item).map_err(failed)?;
                    let constants = capture_class_constants(item).map_err(failed)?;
                    let mut fragments = vec![(SourceFragment::Class(class.def), class.slots)];
                    let stream_slots = streams.slots;
                    fragments.extend(streams.def.into_iter().map(|stream| {
                        (SourceFragment::Stream(stream), clone_slots(&stream_slots))
                    }));
                    let constant_slots = constants.slots;
                    fragments.extend(constants.def.into_iter().map(|constant| {
                        (
                            SourceFragment::Constant(constant),
                            clone_slots(&constant_slots),
                        )
                    }));
                    Ok(fragments)
                }
                ImplCapture::Methods => {
                    let captured = capture_methods(item).map_err(failed)?;
                    Ok(vec![(
                        SourceFragment::Methods {
                            target: captured.target,
                            spelling: captured.spelling,
                            methods: captured.methods,
                            constants: captured.constants,
                        },
                        captured.slots,
                    )])
                }
            }
        }
        _ => Ok(Vec::new()),
    }
}

fn clone_slots(slots: &[boltffi_scan::SlotSource]) -> Vec<boltffi_scan::SlotSource> {
    slots
        .iter()
        .map(|slot| match slot {
            boltffi_scan::SlotSource::Type(ty) => boltffi_scan::SlotSource::Type(ty.clone()),
            boltffi_scan::SlotSource::TraitValue(path) => {
                boltffi_scan::SlotSource::TraitValue(path.clone())
            }
        })
        .collect()
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

/// Lays a record out field by field so only the module path and slot descriptors, the parts
/// rustc alone knows, are computed in const context; the bytes match `capture::record`.
fn record_tokens(fragment: &SourceFragment, slots: &[boltffi_scan::SlotSource]) -> TokenStream {
    let Ok(json) = serde_json::to_vec(fragment) else {
        return TokenStream::new();
    };
    let facade = facade();
    let prefixed = |value: String| {
        let mut bytes = (value.len() as u16).to_le_bytes().to_vec();
        bytes.extend(value.into_bytes());
        (Literal::byte_string(&bytes), bytes.len())
    };
    let (package, package_len) = prefixed(std::env::var("CARGO_PKG_NAME").unwrap_or_default());
    let (version, version_len) = prefixed(std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    let slot_count = slots.len() as u16;
    let json_len = json.len();
    let json_len_bytes = json_len as u32;
    let json = Literal::byte_string(&json);
    let descs = slots
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let ident = format_ident!("SLOT_{index}");
            let desc = match source {
                boltffi_scan::SlotSource::Type(ty) => quote! {
                    <#ty as #facade::__private::capture::TypeDesc<crate::__BoltffiTag>>::DESC
                },
                boltffi_scan::SlotSource::TraitValue(path) => quote! {
                    #facade::__private::capture::trait_desc(&#path)
                },
            };
            quote! {
                const #ident: &[u8] = (&#desc).as_bytes();
            }
        })
        .collect::<Vec<_>>();
    let slot_fields = (0..slots.len())
        .map(|index| {
            let len = format_ident!("slot_{index}_len");
            let bytes = format_ident!("slot_{index}");
            let source = format_ident!("SLOT_{index}");
            quote! { #len: [u8; 2], #bytes: [u8; #source.len()], }
        })
        .collect::<Vec<_>>();
    let slot_values = (0..slots.len())
        .map(|index| {
            let len = format_ident!("slot_{index}_len");
            let bytes = format_ident!("slot_{index}");
            let source = format_ident!("SLOT_{index}");
            quote! {
                #len: (#source.len() as u16).to_le_bytes(),
                #bytes: *#source.first_chunk().unwrap(),
            }
        })
        .collect::<Vec<_>>();

    quote! {
        const _: () = {
            #(#descs)*
            const MODULE: &[u8] = ::core::module_path!().as_bytes();
            #[repr(C)]
            struct Record {
                magic: [u8; 8],
                payload: [u8; 8],
                package: [u8; #package_len],
                version: [u8; #version_len],
                module_len: [u8; 2],
                module: [u8; MODULE.len()],
                slot_count: [u8; 2],
                #(#slot_fields)*
                json_len: [u8; 4],
                json: [u8; #json_len],
            }
            #[cfg_attr(
                target_vendor = "apple",
                unsafe(link_section = "__DATA,__boltffisrc")
            )]
            #[cfg_attr(
                not(target_vendor = "apple"),
                unsafe(link_section = ".boltffisrc")
            )]
            #[used]
            static RECORD: Record = Record {
                magic: #facade::__private::capture::SOURCE_RECORD_MAGIC,
                payload: ((::core::mem::size_of::<Record>() - 16) as u64).to_le_bytes(),
                package: *#package,
                version: *#version,
                module_len: (MODULE.len() as u16).to_le_bytes(),
                module: *MODULE.first_chunk().unwrap(),
                slot_count: #slot_count.to_le_bytes(),
                #(#slot_values)*
                json_len: #json_len_bytes.to_le_bytes(),
                json: *#json,
            };
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
pub(crate) fn trait_identity_tokens(item: &syn::ItemTrait) -> TokenStream {
    let vis = &item.vis;
    let ident = &item.ident;
    let marker = trait_marker(ident);
    let name = ident.to_string();
    let facade = facade();
    quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy)]
        #vis struct #marker;

        impl #facade::__private::capture::TraitDesc for #marker {
            const DESC: #facade::__private::capture::DescBuf =
                #facade::__private::capture::DescBuf::named(::core::module_path!(), #name);
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        #vis const #ident: #marker = #marker;
    }
}

/// The type of the value a callback trait's export site defines under the trait's name.
pub(crate) fn trait_marker(trait_ident: &syn::Ident) -> syn::Ident {
    format_ident!("__BoltffiTrait{}", trait_ident)
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

/// `code` plus the marker each of its callback traits gets at its export site, for
/// tests that compile expander output without going through the attribute.
#[cfg(test)]
pub(crate) fn with_trait_identities(code: TokenStream) -> TokenStream {
    let identities = syn::parse2::<syn::File>(code.clone())
        .map(|file| {
            file.items
                .iter()
                .filter_map(|item| match item {
                    syn::Item::Trait(item) => Some(trait_identity_tokens(item)),
                    _ => None,
                })
                .collect::<TokenStream>()
        })
        .unwrap_or_default();
    quote! {
        #code
        #identities
    }
}
