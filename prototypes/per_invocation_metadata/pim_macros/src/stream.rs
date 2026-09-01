//! `#[export]` on a function returning `PimStream<Item>`: the subscribe, pop, and free
//! exports are all emitted adjacent to the function, and the record carries their symbols.
//! Detection is by the `PimStream` name — an alias would hide it, same caveat as the
//! container vocabulary. Pop encodes `Option<Item>`; async wake semantics stay out of scope.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, GenericArgument, ItemFn, Pat, PathArguments, ReturnType, Type};

use crate::parse::{self, Field, Slots, TypeNode};
use crate::{emit, payload, symbol};

pub struct StreamExport {
    pub name: String,
    pub subscribe_symbol: String,
    pub pop_symbol: String,
    pub free_symbol: String,
    pub params: Vec<Field>,
    pub item: TypeNode,
}

pub fn returns_stream(item: &ItemFn) -> Option<&Type> {
    let ReturnType::Type(_, ty) = &item.sig.output else {
        return None;
    };
    let Type::Path(path) = &**ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "PimStream" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let item_ty = types.next()?;
    types.next().is_none().then_some(item_ty)
}

pub fn item(item: &ItemFn, item_ty: &Type) -> syn::Result<TokenStream> {
    if !item.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.sig.generics,
            "pim: generic functions have no single symbol",
        ));
    }
    if item.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            &item.sig,
            "pim: async functions are not supported",
        ));
    }

    let mut slots = Slots::default();
    let params = item
        .sig
        .inputs
        .iter()
        .map(|input| {
            let FnArg::Typed(typed) = input else {
                return Err(syn::Error::new_spanned(
                    input,
                    "pim: methods are not supported",
                ));
            };
            let Pat::Ident(pat) = &*typed.pat else {
                return Err(syn::Error::new_spanned(
                    &typed.pat,
                    "pim: only plain parameter names are supported",
                ));
            };
            Ok(Field {
                name: pat.ident.to_string(),
                ty: parse::classify(&typed.ty, &mut slots)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let name = item.sig.ident.to_string();
    let stream = StreamExport {
        subscribe_symbol: symbol::export_symbol(&name),
        pop_symbol: symbol::export_symbol(&format!("{name}_pop")),
        free_symbol: symbol::export_symbol(&format!("{name}_free")),
        params,
        item: parse::classify(item_ty, &mut slots)?,
        name,
    };
    let json = payload::stream_json(&stream).map_err(|error| {
        syn::Error::new_spanned(item, format!("pim: cannot serialize the stream: {error}"))
    })?;
    let record = emit::metadata_block(&json, slots.types());
    let wrappers = wrappers(item, item_ty, &stream);

    Ok(quote! {
        #item
        #record
        #wrappers
    })
}

fn wrappers(item: &ItemFn, item_ty: &Type, stream: &StreamExport) -> TokenStream {
    let name = &item.sig.ident;
    let subscribe_wrapper = format_ident!("__pim_export_{name}");
    let pop_wrapper = format_ident!("__pim_export_{name}_pop");
    let free_wrapper = format_ident!("__pim_export_{name}_free");
    let subscribe_symbol = &stream.subscribe_symbol;
    let pop_symbol = &stream.pop_symbol;
    let free_symbol = &stream.free_symbol;

    let idents = (0..item.sig.inputs.len())
        .map(|index| format_ident!("p{index}"))
        .collect::<Vec<_>>();
    let types = item
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => Some(&*typed.ty),
            FnArg::Receiver(_) => None,
        })
        .collect::<Vec<_>>();

    let subscribe_body = quote! {
        #(let #idents = <#types as ::pim_runtime::Codec<crate::PimTag>>::lift(#idents);)*
        ::std::boxed::Box::into_raw(::std::boxed::Box::new(#name(#(#idents),*))) as usize as u64
    };
    let subscribe_poison = quote! { 0u64 };
    let subscribe_trapped = emit::trapped(&subscribe_body, &subscribe_poison);

    let pop_body = quote! {
        assert!(__pim_handle != 0, "null stream handle");
        let __pim_stream = unsafe {
            &mut *(__pim_handle as usize as *mut ::pim_runtime::PimStream<#item_ty>)
        };
        let __pim_item = __pim_stream.pop();
        let mut __pim_bytes = ::std::vec::Vec::new();
        ::pim_runtime::Encode::<crate::PimTag>::write(&__pim_item, &mut __pim_bytes);
        ::pim_runtime::RawBuffer::from_vec(__pim_bytes)
    };
    let pop_poison = quote! { ::pim_runtime::RawBuffer::null() };
    let pop_trapped = emit::trapped(&pop_body, &pop_poison);

    quote! {
        #[unsafe(export_name = #subscribe_symbol)]
        extern "C" fn #subscribe_wrapper(
            #(#idents: <#types as ::pim_runtime::Codec<crate::PimTag>>::FfiType,)*
            __pim_status: *mut ::pim_runtime::CallStatus,
        ) -> u64 {
            #subscribe_trapped
        }

        #[unsafe(export_name = #pop_symbol)]
        extern "C" fn #pop_wrapper(
            __pim_handle: u64,
            __pim_status: *mut ::pim_runtime::CallStatus,
        ) -> ::pim_runtime::RawBuffer {
            #pop_trapped
        }

        #[unsafe(export_name = #free_symbol)]
        extern "C" fn #free_wrapper(__pim_handle: u64) {
            if __pim_handle != 0 {
                drop(unsafe {
                    ::std::boxed::Box::from_raw(
                        __pim_handle as usize as *mut ::pim_runtime::PimStream<#item_ty>,
                    )
                });
            }
        }
    }
}
