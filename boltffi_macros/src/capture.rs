//! Per-invocation capture: every `#[data]`/`#[export]` expansion appends its own source
//! record, so binding discovery reads what the compiler compiled instead of re-scanning
//! source. Invocations the capture cannot describe yet emit an unsupported marker, which
//! makes bindgen fall back to the legacy path rather than ship a partial contract.

use boltffi_binding::SourceFragment;
use boltffi_scan::{
    ClassDeclaration, capture_class, capture_class_methods, capture_constant, capture_enum,
    capture_error_enum, capture_error_struct, capture_function, capture_methods, capture_struct,
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

/// What an invocation declares, where its item's syntax alone does not say.
pub(crate) enum Capture {
    /// The item declares what its syntax says: a record, enum, function, trait or constant.
    Item,
    /// `#[export]` on a struct or enum declares a class. Carries the export marker's
    /// argument tokens for the thread-safety choice.
    Class(TokenStream),
    /// `#[data(impl)]` adds methods to a type whose identity the `#[data]` site owns.
    Methods,
    /// `#[export] impl` adds methods to a class its struct or enum declares.
    ClassMethods,
}

/// The source records `item` contributes, rendered from its [`fragments`] alongside the
/// identity of the type it declares.
pub(crate) fn item_tokens(
    item: proc_macro::TokenStream,
    capture: Capture,
    error: bool,
) -> TokenStream {
    let Ok(item) = syn::parse::<syn::Item>(item) else {
        return TokenStream::new();
    };
    let (name, identity) = match (&item, &capture) {
        (
            syn::Item::Struct(syn::ItemStruct { ident, .. })
            | syn::Item::Enum(syn::ItemEnum { ident, .. }),
            Capture::Class(_),
        ) => {
            let identity = local_identity_tokens(ident, &ident.to_string());
            let class = class_identity_tokens(ident);
            (ident.to_string(), Some(quote! { #identity #class }))
        }
        (syn::Item::Struct(item), _) => (
            item.ident.to_string(),
            Some(local_identity_tokens(&item.ident, &item.ident.to_string())),
        ),
        (syn::Item::Enum(item), _) => (
            item.ident.to_string(),
            Some(local_identity_tokens(&item.ident, &item.ident.to_string())),
        ),
        (syn::Item::Impl(item), capture) => {
            let name = type_leaf_name(&item.self_ty).unwrap_or_else(|| "impl".to_owned());
            let identity = matches!(capture, Capture::ClassMethods)
                .then(|| exported_class_tokens(&item.self_ty));
            (name, identity)
        }
        (syn::Item::Fn(item), _) => (item.sig.ident.to_string(), None),
        (syn::Item::Trait(item), _) => (item.ident.to_string(), None),
        (syn::Item::Const(item), _) => (item.ident.to_string(), None),
        _ => return TokenStream::new(),
    };
    let records = match fragments(&item, &capture, error) {
        Ok(fragments) => fragments
            .iter()
            .map(|(fragment, slots)| record_tokens(fragment, slots))
            .collect(),
        Err(reason) => unsupported_tokens(&name, &reason),
    };
    quote! {
        #identity
        #records
    }
}

/// What one invocation declares, as fragments paired with their written references.
pub(crate) type Fragments = Vec<(SourceFragment, Vec<boltffi_scan::SlotSource>)>;

/// The fragments `item` contributes, the same ones its source record carries.
pub(crate) fn fragments(
    item: &syn::Item,
    capture: &Capture,
    error: bool,
) -> Result<Fragments, String> {
    let failed = |error: boltffi_scan::ScanError| error.to_string();
    if let (Some(declaration), Capture::Class(marker_args)) = (ClassDeclaration::of(item), capture)
    {
        return capture_class(declaration, marker_args.clone())
            .map(|captured| vec![(SourceFragment::Class(captured.def), captured.slots)])
            .map_err(failed);
    }
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
            let captured = match capture {
                Capture::ClassMethods => capture_class_methods(item),
                _ => capture_methods(item),
            }
            .map_err(failed)?;
            Ok(vec![(
                SourceFragment::Methods {
                    target: captured.target,
                    spelling: captured.spelling,
                    methods: captured.methods,
                    constants: captured.constants,
                    streams: captured.streams,
                    doc: captured.doc,
                },
                captured.slots,
            )])
        }
        _ => Ok(Vec::new()),
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
                boltffi_scan::SlotSource::Type(ty) => {
                    let lane = crate::lane::lane_path(source);
                    quote! {
                        <#ty as #facade::__private::capture::TypeDesc<#lane!(@tag)>>::DESC
                    }
                }
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

    let record = quote! {
        #[allow(deprecated)]
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
    };
    quote! {
        #facade::__private::source_record! { #record }
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
        #[allow(deprecated)]
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
        #[allow(deprecated)]
        const _: () = {
            impl<__BoltffiAnyTag> #facade::__private::capture::TypeInfo<__BoltffiAnyTag> for #self_ty {
                const MODULE: &'static str = ::core::module_path!();
                const NAME: &'static str = #name;
            }

            impl<__BoltffiAnyTag> #facade::__private::capture::TypeDesc<__BoltffiAnyTag> for #self_ty {
                const DESC: #facade::__private::capture::DescBuf =
                    #facade::__private::capture::DescBuf::named(
                        <#self_ty as #facade::__private::capture::TypeInfo<__BoltffiAnyTag>>::MODULE,
                        <#self_ty as #facade::__private::capture::TypeInfo<__BoltffiAnyTag>>::NAME,
                    );
            }
        };
    }
}

/// Marks a type as a class at the struct or enum that declares it, which is what an
/// `#[export] impl` block requires of its target.
fn class_identity_tokens<T: quote::ToTokens>(self_ty: &T) -> TokenStream {
    let facade = facade();
    quote! {
        #[allow(deprecated)]
        const _: () = {
            impl<__BoltffiAnyTag> #facade::__private::capture::ClassDesc<__BoltffiAnyTag> for #self_ty {
                const DESC: #facade::__private::capture::DescBuf =
                    <#self_ty as #facade::__private::capture::TypeDesc<__BoltffiAnyTag>>::DESC;
            }
        };
    }
}

/// Requires an `#[export] impl` block's target to be a declared class, in every build, so a
/// type without `#[export]` is reported as such next to its missing lane.
fn exported_class_tokens(self_ty: &syn::Type) -> TokenStream {
    let facade = facade();
    quote! {
        #[allow(deprecated)]
        const _: fn() = || {
            fn exported<Class: ?::core::marker::Sized + #facade::__private::capture::ClassDesc<()>>() {}
            exported::<#self_ty>();
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
