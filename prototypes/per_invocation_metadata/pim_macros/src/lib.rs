//! Stand-in for `#[boltffi::data]`: emits the type's canonical id and its metadata record.

mod callback;
mod class;
mod emit;
mod export;
mod parse;
mod payload;
mod stream;
mod symbol;

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemStruct, ItemTrait, Path, parse_macro_input};

/// Declares the crate's tag type. Must sit at the crate root; every emitted reference names it.
/// Also exports the crate's buffer-free function, its symbol carried by a scaffolding record.
#[proc_macro]
pub fn scaffolding(_input: TokenStream) -> TokenStream {
    let symbol = symbol::export_symbol("free_buffer");
    let json = match payload::scaffolding_json(&symbol) {
        Ok(json) => json,
        Err(error) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("pim: cannot serialize the scaffolding record: {error}"),
            )
            .into_compile_error()
            .into();
        }
    };
    let record = emit::metadata_block(&json, &[]);

    quote! {
        pub struct PimTag;

        #record

        #[unsafe(export_name = #symbol)]
        extern "C" fn __pim_free_buffer(buffer: ::pim_runtime::RawBuffer) {
            if !buffer.ptr.is_null() {
                drop(buffer.into_vec());
            }
        }
    }
    .into()
}

/// Gives a foreign type a canonical id. Legal only because `PimTag` is local to this crate.
#[proc_macro]
pub fn custom_type(input: TokenStream) -> TokenStream {
    let path = parse_macro_input!(input as Path);
    let mut segments = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();

    let Some(name) = segments.pop() else {
        return syn::Error::new_spanned(&path, "pim: expected a type path")
            .into_compile_error()
            .into();
    };
    let module = segments.join("::");

    quote! {
        impl ::pim_runtime::TypeInfo<crate::PimTag> for #path {
            const MODULE: &'static str = #module;
            const NAME: &'static str = #name;
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn data(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemStruct);

    match parse::record(&item).and_then(|record| emit::item(&item, &record)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Stand-in for `#[boltffi::export]`: emits the adjacent wrappers and the invocation's own
/// record — for a function, a `PimStream`-returning function, or a class impl block.
#[proc_macro_attribute]
pub fn export(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::Item);

    let tokens = match &item {
        syn::Item::Fn(function) => match stream::returns_stream(function) {
            Some(item_ty) => stream::item(function, item_ty),
            None => export::item(function),
        },
        syn::Item::Impl(block) => class::item(block),
        other => Err(syn::Error::new_spanned(
            other,
            "pim: only functions and impl blocks can be exported",
        )),
    };
    match tokens {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Stand-in for a callback attribute: the foreign side implements the trait via a vtable.
#[proc_macro_attribute]
pub fn callback(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemTrait);

    match callback::item(&item) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// A proc macro that emits a `#[data]` item — the case bindgen's source scan cannot see.
#[proc_macro]
pub fn define_record(input: TokenStream) -> TokenStream {
    let body = proc_macro2::TokenStream::from(input);

    quote! {
        #[::pim_macros::data]
        pub struct #body
    }
    .into()
}
