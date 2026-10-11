//! `#[callback]` on a trait: the foreign side implements it behind a repr(C) vtable. The
//! vtable layout is the free slot followed by the methods in declaration order — the record
//! pins that order. Everything, including the `Codec` for `Box<dyn Trait>`, is derived at
//! the trait's own definition site.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemTrait, Pat, ReturnType, TraitItem, TraitItemFn, Type};

use crate::parse::{self, Field, Slots, TypeNode};
use crate::{emit, payload};

pub struct CallbackFn {
    pub name: String,
    pub params: Vec<Field>,
    pub ret: Option<TypeNode>,
}

pub fn item(definition: &ItemTrait) -> syn::Result<TokenStream> {
    if !definition.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &definition.generics,
            "pim: generic callback traits have no single vtable",
        ));
    }
    if !definition.supertraits.is_empty() {
        return Err(syn::Error::new_spanned(
            &definition.supertraits,
            "pim: callback traits cannot have supertraits",
        ));
    }

    let mut slots = Slots::default();
    let mut methods = Vec::new();
    for item in &definition.items {
        let TraitItem::Fn(function) = item else {
            return Err(syn::Error::new_spanned(
                item,
                "pim: callback traits can only declare methods",
            ));
        };
        methods.push((callback_fn(function, &mut slots)?, function));
    }

    let name_literal = definition.ident.to_string();
    let metas = methods.iter().map(|(meta, _)| meta).collect::<Vec<_>>();
    let json = payload::callback_json(&name_literal, &metas).map_err(|error| {
        syn::Error::new_spanned(
            definition,
            format!("pim: cannot serialize the callback: {error}"),
        )
    })?;
    let record = emit::metadata_block(&json, slots.types());

    let trait_ident = &definition.ident;
    let vtable = format_ident!("Pim{trait_ident}VTable");
    let foreign = format_ident!("PimForeign{trait_ident}");
    let vtable_fields = methods
        .iter()
        .map(|(_, function)| vtable_field(function))
        .collect::<Vec<_>>();
    let foreign_methods = methods
        .iter()
        .map(|(_, function)| foreign_method(function))
        .collect::<Vec<_>>();

    Ok(quote! {
        #definition

        #[repr(C)]
        pub struct #vtable {
            pub free: extern "C" fn(u64),
            #(#vtable_fields)*
        }

        pub struct #foreign {
            data: u64,
            vtable: *const #vtable,
        }

        impl #trait_ident for #foreign {
            #(#foreign_methods)*
        }

        impl Drop for #foreign {
            fn drop(&mut self) {
                (unsafe { &*self.vtable }.free)(self.data);
            }
        }

        impl<Tag> ::pim_runtime::TypeInfo<Tag> for dyn #trait_ident {
            const MODULE: &'static str = ::core::module_path!();
            const NAME: &'static str = #name_literal;
        }

        impl<Tag> ::pim_runtime::Codec<Tag> for ::std::boxed::Box<dyn #trait_ident> {
            type FfiType = ::pim_runtime::CallbackHandle;

            fn lower(self) -> ::pim_runtime::CallbackHandle {
                panic!("callbacks cross only foreign-to-Rust in this prototype")
            }

            fn lift(ffi: ::pim_runtime::CallbackHandle) -> Self {
                ::std::boxed::Box::new(#foreign {
                    data: ffi.data,
                    vtable: ffi.vtable as *const #vtable,
                })
            }

            fn poisoned() -> ::pim_runtime::CallbackHandle {
                ::pim_runtime::CallbackHandle::null()
            }
        }

        #record
    })
}

fn callback_fn(function: &TraitItemFn, slots: &mut Slots) -> syn::Result<CallbackFn> {
    match function.sig.inputs.first() {
        Some(FnArg::Receiver(receiver))
            if receiver.reference.is_some() && receiver.mutability.is_none() => {}
        _ => {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "pim: callback methods must take `&self`",
            ));
        }
    }

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

    let ret = match &function.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(parse::classify(ty, slots)?),
    };

    Ok(CallbackFn {
        name: function.sig.ident.to_string(),
        params,
        ret,
    })
}

fn signature_types(function: &TraitItemFn) -> (Vec<&Type>, Type) {
    let types = function
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => Some(&*typed.ty),
            FnArg::Receiver(_) => None,
        })
        .collect();
    let ret = match &function.sig.output {
        ReturnType::Default => syn::parse_quote!(()),
        ReturnType::Type(_, ty) => (**ty).clone(),
    };
    (types, ret)
}

fn vtable_field(function: &TraitItemFn) -> TokenStream {
    let method = &function.sig.ident;
    let (types, ret) = signature_types(function);

    quote! {
        pub #method: extern "C" fn(
            u64
            #(, <#types as ::pim_runtime::Codec<crate::PimTag>>::FfiType)*
        ) -> <#ret as ::pim_runtime::Codec<crate::PimTag>>::FfiType,
    }
}

fn foreign_method(function: &TraitItemFn) -> TokenStream {
    let method = &function.sig.ident;
    let signature = &function.sig;
    let (types, ret) = signature_types(function);
    let idents = function
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => match &*typed.pat {
                Pat::Ident(pat) => Some(pat.ident.clone()),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .collect::<Vec<_>>();

    quote! {
        #signature {
            let __pim_vtable = unsafe { &*self.vtable };
            <#ret as ::pim_runtime::Codec<crate::PimTag>>::lift((__pim_vtable.#method)(
                self.data
                #(, <#types as ::pim_runtime::Codec<crate::PimTag>>::lower(#idents))*
            ))
        }
    }
}
