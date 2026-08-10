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
            Ok(mut captured) => {
                let wrapper = if wrappers_enabled() {
                    match wrapper_tokens(item, &mut captured.def) {
                        Ok(wrapper) => wrapper,
                        Err(reason) => {
                            return unsupported_tokens(&item.sig.ident.to_string(), reason);
                        }
                    }
                } else {
                    TokenStream::new()
                };
                let record =
                    record_tokens(&SourceFragment::Function(captured.def), &captured.slots);
                quote! {
                    #wrapper
                    #record
                }
            }
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
                    let identity = local_identity_tokens(self_ty, &name, LocalCrossing::None);
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
            let identity = local_identity_tokens(&ident, &pool.name, LocalCrossing::None);
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
    let identity = local_identity_tokens(ident, &ident.to_string(), LocalCrossing::Wire);
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
    let cross = wire_cross_tokens(ident);
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

            #cross
        };
        #record
    }
}

/// Read at expansion time; cargo does not fingerprint the variable, so toggling it
/// only takes effect on a fresh build of the affected crates.
fn wrappers_enabled() -> bool {
    std::env::var_os("BOLTFFI_CAPTURE_WRAPPERS").is_some()
}

fn wrapper_tokens(
    item: &syn::ItemFn,
    def: &mut boltffi_ast::FunctionDef,
) -> Result<TokenStream, &'static str> {
    if !item.sig.generics.params.is_empty() {
        return Err("generic functions have no per-invocation wrapper");
    }
    if item.sig.variadic.is_some() || item.sig.abi.is_some() {
        return Err("extern or variadic functions have no per-invocation wrapper");
    }

    let mut names = Vec::new();
    let mut types = Vec::new();
    for input in &item.sig.inputs {
        let syn::FnArg::Typed(typed) = input else {
            return Err("methods have no per-invocation wrapper yet");
        };
        let syn::Pat::Ident(pat) = &*typed.pat else {
            return Err("pattern parameters have no per-invocation wrapper");
        };
        if crossing_unsupported(&typed.ty) {
            return Err("borrowed or impl-trait parameters have no per-invocation wrapper yet");
        }
        names.push(pat.ident.clone());
        types.push((*typed.ty).clone());
    }

    let return_type = match &item.sig.output {
        syn::ReturnType::Default => syn::parse_quote!(()),
        syn::ReturnType::Type(_, ty) => {
            if crossing_unsupported(ty) {
                return Err("borrowed returns have no per-invocation wrapper yet");
            }
            (**ty).clone()
        }
    };

    for parameter in &def.parameters {
        if !wrapper_crossable(&parameter.type_expr) {
            return Err("a parameter type has no FfiCross crossing yet");
        }
    }
    if let boltffi_ast::ReturnDef::Value(returns) = &def.returns
        && !wrapper_crossable(returns)
    {
        return Err("the return type has no FfiCross crossing yet");
    }

    let ident = &item.sig.ident;
    let symbol = invocation_symbol(&ident.to_string());
    def.native_symbol = Some(symbol.clone());
    let facade = facade();

    if item.sig.asyncness.is_some() {
        return Ok(async_wrapper_tokens(
            ident,
            &symbol,
            &names,
            &types,
            &return_type,
            &facade,
        ));
    }

    let symbol_ident = quote::format_ident!("{symbol}");
    Ok(quote! {
        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #symbol_ident(
            #(#names: <#types as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::Ffi,)*
            __boltffi_status: *mut #facade::__private::FfiStatus,
        ) -> <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::Ffi {
            if !__boltffi_status.is_null() {
                unsafe { *__boltffi_status = #facade::__private::FfiStatus::OK };
            }
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(
                move || -> ::core::result::Result<
                    <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::Ffi,
                    #facade::__private::wire::DecodeError,
                > {
                    let __boltffi_ret = #ident(
                        #(<#types as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::lift(#names)?),*
                    );
                    ::core::result::Result::Ok(
                        <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::lower(__boltffi_ret)
                    )
                }
            )) {
                Ok(::core::result::Result::Ok(value)) => value,
                Ok(::core::result::Result::Err(error)) => {
                    unsafe { #facade::__private::capture::note_invalid_arg(__boltffi_status, error) };
                    <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::poisoned()
                }
                Err(panic) => {
                    unsafe { #facade::__private::capture::note_panic(__boltffi_status, panic) };
                    <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::poisoned()
                }
            }
        }
    })
}

/// The async family behind one record-carried start symbol: poll, complete, cancel,
/// free, and panic-message companions are fixed suffixes of it — an ABI convention
/// like the trailing status parameter, not a name derived from user items.
fn async_wrapper_tokens(
    ident: &syn::Ident,
    symbol: &str,
    names: &[syn::Ident],
    types: &[syn::Type],
    return_type: &syn::Type,
    facade: &TokenStream,
) -> TokenStream {
    let start_ident = quote::format_ident!("{symbol}");
    let poll_ident = quote::format_ident!("{symbol}_poll");
    let complete_ident = quote::format_ident!("{symbol}_complete");
    let cancel_ident = quote::format_ident!("{symbol}_cancel");
    let free_ident = quote::format_ident!("{symbol}_free");
    let panic_message_ident = quote::format_ident!("{symbol}_panic_message");

    quote! {
        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #start_ident(
            #(#names: <#types as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::Ffi,)*
        ) -> #facade::__private::RustFutureHandle {
            #(
                let #names = match <#types as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::lift(#names) {
                    ::core::result::Result::Ok(value) => value,
                    ::core::result::Result::Err(error) => {
                        unsafe {
                            #facade::__private::capture::note_invalid_arg(::core::ptr::null_mut(), error)
                        };
                        return #facade::__private::rustfuture::rust_future_invalid_arg::<#return_type>();
                    }
                };
            )*
            #facade::__private::rustfuture::rust_future_new(async move {
                #ident(#(#names),*).await
            })
        }

        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #poll_ident(
            handle: #facade::__private::RustFutureHandle,
            callback_data: u64,
            callback: #facade::__private::RustFutureContinuationCallback,
        ) {
            unsafe {
                #facade::__private::rustfuture::rust_future_poll::<#return_type>(
                    handle,
                    callback,
                    callback_data,
                )
            }
        }

        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #complete_ident(
            handle: #facade::__private::RustFutureHandle,
            __boltffi_status: *mut #facade::__private::FfiStatus,
        ) -> <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::Ffi {
            match unsafe {
                #facade::__private::rustfuture::rust_future_complete::<#return_type>(handle)
            } {
                ::core::result::Result::Ok(value) => {
                    if !__boltffi_status.is_null() {
                        unsafe { *__boltffi_status = #facade::__private::FfiStatus::OK };
                    }
                    <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::lower(value)
                }
                ::core::result::Result::Err(status) => {
                    if !__boltffi_status.is_null() {
                        unsafe { *__boltffi_status = status };
                    }
                    <#return_type as #facade::__private::capture::FfiCross<crate::__BoltffiTag>>::poisoned()
                }
            }
        }

        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #cancel_ident(handle: #facade::__private::RustFutureHandle) {
            unsafe { #facade::__private::rustfuture::rust_future_cancel::<#return_type>(handle) }
        }

        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #free_ident(handle: #facade::__private::RustFutureHandle) {
            unsafe { #facade::__private::rustfuture::rust_future_free::<#return_type>(handle) }
        }

        #[doc(hidden)]
        #[unsafe(no_mangle)]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn #panic_message_ident(
            handle: #facade::__private::RustFutureHandle,
        ) -> #facade::__private::FfiBuf {
            match unsafe {
                #facade::__private::rustfuture::rust_future_panic_message::<#return_type>(handle)
            } {
                ::core::option::Option::Some(message) => {
                    #facade::__private::FfiBuf::wire_encode(&message)
                }
                ::core::option::Option::None => #facade::__private::FfiBuf::empty(),
            }
        }
    }
}

fn crossing_unsupported(ty: &syn::Type) -> bool {
    matches!(
        ty,
        syn::Type::Reference(_) | syn::Type::ImplTrait(_) | syn::Type::Ptr(_)
    )
}

/// Whether every type reached by this expression has an `FfiCross` impl the wrapper can
/// name: core's primitives, builtins, and containers, plus the impls `#[data]` sites
/// emit. Everything else refuses so the crate falls back instead of failing to compile —
/// including all custom types, since the function site cannot tell a `custom_ffi` local
/// (which has an impl) from a `custom_type!` remote (which the orphan rule keeps bare).
fn wrapper_crossable(type_expr: &boltffi_ast::TypeExpr) -> bool {
    use boltffi_ast::TypeExpr;
    match type_expr {
        TypeExpr::Primitive(_)
        | TypeExpr::Unit
        | TypeExpr::String
        | TypeExpr::Builtin(_)
        | TypeExpr::Record { .. }
        | TypeExpr::Enum { .. } => true,
        TypeExpr::Boxed(inner) | TypeExpr::Vec(inner) | TypeExpr::Option(inner) => {
            wrapper_crossable(inner)
        }
        TypeExpr::Result { ok, err } => wrapper_crossable(ok) && wrapper_crossable(err),
        TypeExpr::Map { key, value, .. } => wrapper_crossable(key) && wrapper_crossable(value),
        TypeExpr::Tuple(items) => items.iter().all(wrapper_crossable),
        _ => false,
    }
}

fn invocation_symbol(item: &str) -> String {
    let span = proc_macro::Span::call_site();
    let crate_name = std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "unknown".to_owned());
    let position = format!("{}:{}:{}", span.file(), span.line(), span.column());
    format!("boltffi_inv_{crate_name}_{item}_{:08x}", fnv1a(&position))
}

fn fnv1a(input: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in input.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
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
    let identity = local_identity_tokens(self_ty, &name, LocalCrossing::Wire);
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

/// Whether an identity site also gets a wire-encoded [`FfiCross`] impl.
enum LocalCrossing {
    Wire,
    None,
}

fn local_identity_tokens<T: quote::ToTokens>(
    self_ty: &T,
    name: &str,
    crossing: LocalCrossing,
) -> TokenStream {
    let cross = match crossing {
        LocalCrossing::Wire => wire_cross_tokens(self_ty),
        LocalCrossing::None => TokenStream::new(),
    };
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

            #cross
        };
    }
}

fn wire_cross_tokens<T: quote::ToTokens>(self_ty: &T) -> TokenStream {
    let facade = facade();
    quote! {
        impl<Tag> #facade::__private::capture::FfiCross<Tag> for #self_ty {
            type Ffi = #facade::__private::FfiBuf;

            fn lower(self) -> #facade::__private::FfiBuf {
                #facade::__private::FfiBuf::wire_encode(&self)
            }

            fn lift(
                ffi: #facade::__private::FfiBuf,
            ) -> ::core::result::Result<Self, #facade::__private::wire::DecodeError> {
                #facade::__private::wire::decode(unsafe { ffi.as_byte_slice() })
            }

            fn poisoned() -> #facade::__private::FfiBuf {
                #facade::__private::FfiBuf::empty()
            }
        }
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

#[cfg(test)]
mod tests {
    use boltffi_ast::{BuiltinType, TypeExpr};

    use super::wrapper_crossable;

    #[test]
    fn wrapper_crossability_follows_ffi_cross_coverage() {
        assert!(
            wrapper_crossable(&TypeExpr::Builtin(BuiltinType::Duration)),
            "builtins have core crossings"
        );
        assert!(
            wrapper_crossable(&TypeExpr::Vec(Box::new(TypeExpr::String))),
            "containers cross when their elements do"
        );
        assert!(!wrapper_crossable(&TypeExpr::Str), "borrowed types refuse");
        assert!(
            !wrapper_crossable(&TypeExpr::Vec(Box::new(TypeExpr::Str))),
            "containers refuse when their elements do"
        );
    }
}
