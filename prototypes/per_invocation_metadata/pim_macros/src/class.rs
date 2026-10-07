//! `#[export]` on an impl block: constructors and `&self` methods behind a `u64` handle,
//! with the whole handle protocol emitted adjacent to the block it describes.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, Pat, ReturnType, Type};

use crate::parse::{self, Field, Slots, TypeNode};
use crate::{emit, payload, symbol};

pub struct MethodFn {
    pub name: String,
    pub symbol: String,
    pub params: Vec<Field>,
    pub ret: Option<TypeNode>,
}

struct Parsed<'block> {
    class: Ident,
    constructors: Vec<(MethodFn, &'block ImplItemFn)>,
    methods: Vec<(MethodFn, &'block ImplItemFn)>,
    free_symbol: String,
    slots: Slots,
}

pub fn item(block: &ItemImpl) -> syn::Result<TokenStream> {
    let parsed = parse_impl(block)?;
    let constructors = parsed
        .constructors
        .iter()
        .map(|(meta, _)| meta)
        .collect::<Vec<_>>();
    let methods = parsed
        .methods
        .iter()
        .map(|(meta, _)| meta)
        .collect::<Vec<_>>();
    let json = payload::class_json(
        &parsed.class.to_string(),
        &constructors,
        &methods,
        &parsed.free_symbol,
    )
    .map_err(|error| {
        syn::Error::new_spanned(block, format!("pim: cannot serialize the class: {error}"))
    })?;
    let record = emit::metadata_block(&json, parsed.slots.types());

    let class = &parsed.class;
    let name_literal = parsed.class.to_string();
    let free_symbol = &parsed.free_symbol;
    let free_wrapper = format_ident!("__pim_export_{class}_free");
    let constructor_wrappers = parsed
        .constructors
        .iter()
        .map(|(meta, function)| constructor(class, &meta.symbol, function))
        .collect::<Vec<_>>();
    let method_wrappers = parsed
        .methods
        .iter()
        .map(|(meta, function)| method(class, &meta.symbol, function))
        .collect::<Vec<_>>();

    Ok(quote! {
        #block

        impl<Tag> ::pim_runtime::TypeInfo<Tag> for #class {
            const MODULE: &'static str = ::core::module_path!();
            const NAME: &'static str = #name_literal;
        }

        #record

        #(#constructor_wrappers)*
        #(#method_wrappers)*

        #[unsafe(export_name = #free_symbol)]
        extern "C" fn #free_wrapper(__pim_handle: u64) {
            if __pim_handle != 0 {
                drop(unsafe { ::std::boxed::Box::from_raw(__pim_handle as usize as *mut #class) });
            }
        }
    })
}

fn parse_impl(block: &ItemImpl) -> syn::Result<Parsed<'_>> {
    if !block.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &block.generics,
            "pim: generic classes have no single canonical id",
        ));
    }
    if block.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            block,
            "pim: trait impls cannot be exported",
        ));
    }
    let class = class_ident(&block.self_ty)?;

    let mut slots = Slots::default();
    let mut constructors = Vec::new();
    let mut methods = Vec::new();
    for item in &block.items {
        let ImplItem::Fn(function) = item else {
            return Err(syn::Error::new_spanned(
                item,
                "pim: only methods can be exported from an impl block",
            ));
        };
        if !function.sig.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &function.sig.generics,
                "pim: generic methods have no single symbol",
            ));
        }
        if function.sig.asyncness.is_some() {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "pim: async methods are not supported",
            ));
        }

        match function.sig.inputs.first() {
            Some(FnArg::Receiver(receiver)) => {
                if receiver.reference.is_none() || receiver.mutability.is_some() {
                    return Err(syn::Error::new_spanned(
                        receiver,
                        "pim: only `&self` receivers are supported",
                    ));
                }
                methods.push((method_fn(&class, function, &mut slots, true)?, function));
            }
            _ => {
                constructor_return(&class, function)?;
                constructors.push((method_fn(&class, function, &mut slots, false)?, function));
            }
        }
    }

    Ok(Parsed {
        free_symbol: symbol::export_symbol(&format!("{class}_free")),
        class,
        constructors,
        methods,
        slots,
    })
}

fn class_ident(self_ty: &Type) -> syn::Result<Ident> {
    if let Type::Path(path) = self_ty
        && let Some(ident) = path.path.get_ident()
    {
        return Ok(ident.clone());
    }
    Err(syn::Error::new_spanned(
        self_ty,
        "pim: only plain local types can be exported as classes",
    ))
}

/// The declared `Self` return stays out of the record: a constructor yields a handle.
fn constructor_return(class: &Ident, function: &ImplItemFn) -> syn::Result<()> {
    if let ReturnType::Type(_, ty) = &function.sig.output
        && let Type::Path(path) = &**ty
        && let Some(ident) = path.path.get_ident()
        && (ident == "Self" || ident == class)
    {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        &function.sig.output,
        "pim: an associated function must return `Self` to be a constructor",
    ))
}

fn method_fn(
    class: &Ident,
    function: &ImplItemFn,
    slots: &mut Slots,
    records_return: bool,
) -> syn::Result<MethodFn> {
    let params = function
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => Some(typed),
            FnArg::Receiver(_) => None,
        })
        .map(|typed| {
            let Pat::Ident(pat) = &*typed.pat else {
                return Err(syn::Error::new_spanned(
                    &typed.pat,
                    "pim: only plain parameter names are supported",
                ));
            };
            Ok(Field {
                name: pat.ident.to_string(),
                ty: parse::classify(&typed.ty, slots)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let ret = match (&function.sig.output, records_return) {
        (ReturnType::Type(_, ty), true) => Some(parse::classify(ty, slots)?),
        _ => None,
    };

    let name = function.sig.ident.to_string();
    Ok(MethodFn {
        symbol: symbol::export_symbol(&format!("{class}_{name}")),
        name,
        params,
        ret,
    })
}

fn typed_arguments(function: &ImplItemFn) -> (Vec<Ident>, Vec<&Type>) {
    let types = function
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => Some(&*typed.ty),
            FnArg::Receiver(_) => None,
        })
        .collect::<Vec<_>>();
    let idents = (0..types.len())
        .map(|index| format_ident!("p{index}"))
        .collect();
    (idents, types)
}

fn constructor(class: &Ident, symbol: &str, function: &ImplItemFn) -> TokenStream {
    let method = &function.sig.ident;
    let wrapper = format_ident!("__pim_export_{class}_{method}");
    let (idents, types) = typed_arguments(function);
    let body = quote! {
        #(let #idents = <#types as ::pim_runtime::Codec<crate::PimTag>>::lift(#idents);)*
        ::std::boxed::Box::into_raw(::std::boxed::Box::new(#class::#method(#(#idents),*))) as usize as u64
    };
    let poison = quote! { 0u64 };
    let trapped = emit::trapped(&body, &poison);

    quote! {
        #[unsafe(export_name = #symbol)]
        extern "C" fn #wrapper(
            #(#idents: <#types as ::pim_runtime::Codec<crate::PimTag>>::FfiType,)*
            __pim_status: *mut ::pim_runtime::CallStatus,
        ) -> u64 {
            #trapped
        }
    }
}

fn method(class: &Ident, symbol: &str, function: &ImplItemFn) -> TokenStream {
    let method = &function.sig.ident;
    let wrapper = format_ident!("__pim_export_{class}_{method}");
    let (idents, types) = typed_arguments(function);
    let unit = syn::parse_quote!(());
    let ret: &Type = match &function.sig.output {
        ReturnType::Default => &unit,
        ReturnType::Type(_, ty) => ty,
    };
    let body = quote! {
        assert!(__pim_handle != 0, "null class handle");
        let __pim_receiver = unsafe { &*(__pim_handle as usize as *const #class) };
        #(let #idents = <#types as ::pim_runtime::Codec<crate::PimTag>>::lift(#idents);)*
        <#ret as ::pim_runtime::Codec<crate::PimTag>>::lower(__pim_receiver.#method(#(#idents),*))
    };
    let poison = quote! { <#ret as ::pim_runtime::Codec<crate::PimTag>>::poisoned() };
    let trapped = emit::trapped(&body, &poison);

    quote! {
        #[unsafe(export_name = #symbol)]
        extern "C" fn #wrapper(
            __pim_handle: u64,
            #(#idents: <#types as ::pim_runtime::Codec<crate::PimTag>>::FfiType,)*
            __pim_status: *mut ::pim_runtime::CallStatus,
        ) -> <#ret as ::pim_runtime::Codec<crate::PimTag>>::FfiType {
            #trapped
        }
    }
}
