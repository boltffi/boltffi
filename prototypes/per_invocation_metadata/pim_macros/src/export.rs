use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

use crate::parse::{self, Field, Slots, TypeNode};
use crate::{emit, payload, symbol};

/// One exported function: its record payload and everything the wrapper needs.
pub struct ExportFn {
    pub name: String,
    pub symbol: String,
    pub params: Vec<Field>,
    pub ret: Option<TypeNode>,
    pub slots: Slots,
}

pub fn item(item: &ItemFn) -> syn::Result<TokenStream> {
    let function = parse_fn(item)?;
    let json = payload::function_json(&function).map_err(|error| {
        syn::Error::new_spanned(item, format!("pim: cannot serialize the function: {error}"))
    })?;
    let record = emit::metadata_block(&json, function.slots.types());
    let wrapper = wrapper(item, &function.symbol);

    Ok(quote! {
        #item
        #record
        #wrapper
    })
}

fn parse_fn(item: &ItemFn) -> syn::Result<ExportFn> {
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

    let ret = match &item.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(parse::classify(ty, &mut slots)?),
    };

    let name = item.sig.ident.to_string();
    Ok(ExportFn {
        symbol: symbol::export_symbol(&name),
        name,
        params,
        ret,
        slots,
    })
}

fn wrapper(item: &ItemFn, symbol: &str) -> TokenStream {
    let name = &item.sig.ident;
    let wrapper = format_ident!("__pim_export_{name}");
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
    let unit = syn::parse_quote!(());
    let ret: &Type = match &item.sig.output {
        ReturnType::Default => &unit,
        ReturnType::Type(_, ty) => ty,
    };

    quote! {
        #[unsafe(export_name = #symbol)]
        extern "C" fn #wrapper(
            #(#idents: <#types as ::pim_runtime::Codec<crate::PimTag>>::FfiType),*
        ) -> <#ret as ::pim_runtime::Codec<crate::PimTag>>::FfiType {
            #(let #idents = <#types as ::pim_runtime::Codec<crate::PimTag>>::lift(#idents);)*
            let out = #name(#(#idents),*);
            <#ret as ::pim_runtime::Codec<crate::PimTag>>::lower(out)
        }
    }
}
