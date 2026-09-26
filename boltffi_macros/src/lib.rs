use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

mod capture;
mod cfg_eval;
mod custom;
mod data;
mod expansion;
mod interned_string;
mod lane;

#[proc_macro_derive(FfiType)]
pub fn derive_ffi_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let has_repr_c = input.attrs.iter().any(|attribute| {
        attribute.path().is_ident("repr")
            && attribute
                .parse_args::<syn::Ident>()
                .is_ok_and(|identifier| identifier == "C")
    });
    if has_repr_c {
        TokenStream::new()
    } else {
        syn::Error::new_spanned(&input, "FfiType requires #[repr(C)]")
            .to_compile_error()
            .into()
    }
}

#[proc_macro_attribute]
pub fn ffi_stream(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn custom_ffi(attribute: TokenStream, item: TokenStream) -> TokenStream {
    invocation(lane::Kind::CustomFfi, attribute, item)
}

#[proc_macro]
pub fn custom_type(item: TokenStream) -> TokenStream {
    let captured = capture::custom_type_tokens(item.clone());
    let lane = lane::start(
        lane::Kind::CustomType,
        proc_macro2::TokenStream::new(),
        item.clone().into(),
    );
    let expanded = proc_macro2::TokenStream::from(custom::r#type::custom_type_impl(item));
    append_capture(quote!(#expanded #lane).into(), captured)
}

#[proc_macro]
pub fn interned_string_pool(item: TokenStream) -> TokenStream {
    let captured = capture::interned_string_pool_tokens(item.clone());
    let lane = lane::start(
        lane::Kind::Pool,
        proc_macro2::TokenStream::new(),
        item.clone().into(),
    );
    let expanded = proc_macro2::TokenStream::from(interned_string::interned_string_pool_impl(item));
    append_capture(quote!(#expanded #lane).into(), captured)
}

#[proc_macro_attribute]
pub fn data(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if attribute.to_string().trim() == "impl" {
        invocation(lane::Kind::DataImpl, attribute, item)
    } else {
        invocation(lane::Kind::Data, attribute, item)
    }
}

#[proc_macro_attribute]
pub fn error(attribute: TokenStream, item: TokenStream) -> TokenStream {
    invocation(lane::Kind::Error, attribute, item)
}

#[proc_macro_attribute]
pub fn export(attribute: TokenStream, item: TokenStream) -> TokenStream {
    match syn::parse::<syn::Item>(item.clone()) {
        Ok(syn::Item::Const(_) | syn::Item::Fn(_) | syn::Item::Impl(_) | syn::Item::Trait(_)) => {
            invocation(lane::Kind::Export, attribute, item)
        }
        Ok(item) => syn::Error::new_spanned(
            item,
            "export can only be applied to const, fn, impl, or trait",
        )
        .to_compile_error()
        .into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[doc(hidden)]
#[proc_macro_derive(CfgEval, attributes(boltffi_cfg_eval))]
pub fn cfg_eval(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match cfg_eval::evaluate(input) {
        Ok(evaluated) => {
            let item = &evaluated.item;
            let item = quote!(#item);
            let records = records(evaluated.kind, &evaluated.attribute, item.clone().into());
            let chain = lane::start(evaluated.kind, evaluated.attribute, item);
            quote!(#records #chain).into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

/// One invocation: the item as written, plus its records and lane chain once the
/// compiler has settled which of its members exist.
fn invocation(kind: lane::Kind, attribute: TokenStream, item: TokenStream) -> TokenStream {
    let parsed = syn::parse::<syn::Item>(item.clone()).ok();
    let emitted = match kind {
        lane::Kind::Data | lane::Kind::Error => {
            strip_boltffi_attrs(data::repr::materialize(item.clone()))
        }
        _ => strip_boltffi_attrs(item.clone()),
    };
    let emitted = proc_macro2::TokenStream::from(emitted);
    let identity = match &parsed {
        Some(syn::Item::Trait(item)) => capture::trait_identity_tokens(item),
        _ => proc_macro2::TokenStream::new(),
    };
    let attribute = proc_macro2::TokenStream::from(attribute);
    let described = match &parsed {
        Some(parsed) if cfg_eval::is_conditional(parsed) => {
            cfg_eval::defer(kind, attribute, item.into(), parsed)
        }
        _ => {
            let records = records(kind, &attribute, item.clone());
            let chain = lane::start(kind, attribute, item.into());
            quote!(#records #chain)
        }
    };
    quote!(#emitted #identity #described).into()
}

/// The source records bindgen reads for one invocation.
fn records(
    kind: lane::Kind,
    attribute: &proc_macro2::TokenStream,
    item: TokenStream,
) -> proc_macro2::TokenStream {
    match kind {
        lane::Kind::Data => capture::item_tokens(
            item,
            capture::ImplCapture::Class(proc_macro2::TokenStream::new()),
        ),
        lane::Kind::Error => capture::error_item_tokens(item),
        lane::Kind::Export => {
            capture::item_tokens(item, capture::ImplCapture::Class(attribute.clone()))
        }
        lane::Kind::DataImpl => capture::item_tokens(item, capture::ImplCapture::Methods),
        lane::Kind::CustomFfi => capture::custom_ffi_tokens(item),
        lane::Kind::CustomType => capture::custom_type_tokens(item),
        lane::Kind::Pool => capture::interned_string_pool_tokens(item),
    }
}

#[doc(hidden)]
#[proc_macro]
pub fn lane_resume(input: TokenStream) -> TokenStream {
    lane::resume(input.into()).into()
}

#[proc_macro]
pub fn scaffolding(item: TokenStream) -> TokenStream {
    if !item.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "boltffi::scaffolding! takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    capture::scaffolding_tokens().into()
}

fn append_capture(expanded: TokenStream, captured: proc_macro2::TokenStream) -> TokenStream {
    let expanded = proc_macro2::TokenStream::from(expanded);
    quote!(#expanded #captured).into()
}

#[proc_macro_attribute]
pub fn skip(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn name(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn default(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

fn strip_boltffi_attrs(item: TokenStream) -> TokenStream {
    let Ok(mut item) = syn::parse::<syn::Item>(item.clone()) else {
        return item;
    };
    strip_item_attrs(&mut item);
    TokenStream::from(quote!(#item))
}

fn strip_item_attrs(item: &mut syn::Item) {
    match item {
        syn::Item::Const(item) => strip_attrs(&mut item.attrs),
        syn::Item::Enum(item) => {
            strip_attrs(&mut item.attrs);
            item.variants.iter_mut().for_each(|variant| {
                strip_attrs(&mut variant.attrs);
                strip_fields_attrs(&mut variant.fields);
            });
        }
        syn::Item::Fn(item) => {
            strip_attrs(&mut item.attrs);
            strip_signature_attrs(&mut item.sig);
        }
        syn::Item::Impl(item) => {
            strip_attrs(&mut item.attrs);
            item.items.iter_mut().for_each(strip_impl_item_attrs);
        }
        syn::Item::Struct(item) => {
            strip_attrs(&mut item.attrs);
            strip_fields_attrs(&mut item.fields);
        }
        syn::Item::Trait(item) => {
            strip_attrs(&mut item.attrs);
            item.items.iter_mut().for_each(strip_trait_item_attrs);
        }
        _ => {}
    }
}

fn strip_fields_attrs(fields: &mut syn::Fields) {
    match fields {
        syn::Fields::Named(fields) => fields
            .named
            .iter_mut()
            .for_each(|field| strip_attrs(&mut field.attrs)),
        syn::Fields::Unnamed(fields) => fields
            .unnamed
            .iter_mut()
            .for_each(|field| strip_attrs(&mut field.attrs)),
        syn::Fields::Unit => {}
    }
}

fn strip_impl_item_attrs(item: &mut syn::ImplItem) {
    match item {
        syn::ImplItem::Const(item) => strip_attrs(&mut item.attrs),
        syn::ImplItem::Fn(item) => {
            strip_attrs(&mut item.attrs);
            strip_signature_attrs(&mut item.sig);
        }
        syn::ImplItem::Type(item) => strip_attrs(&mut item.attrs),
        _ => {}
    }
}

fn strip_trait_item_attrs(item: &mut syn::TraitItem) {
    match item {
        syn::TraitItem::Const(item) => strip_attrs(&mut item.attrs),
        syn::TraitItem::Fn(item) => {
            strip_attrs(&mut item.attrs);
            strip_signature_attrs(&mut item.sig);
        }
        syn::TraitItem::Type(item) => strip_attrs(&mut item.attrs),
        _ => {}
    }
}

fn strip_signature_attrs(signature: &mut syn::Signature) {
    signature.inputs.iter_mut().for_each(|input| match input {
        syn::FnArg::Receiver(receiver) => strip_attrs(&mut receiver.attrs),
        syn::FnArg::Typed(argument) => strip_attrs(&mut argument.attrs),
    });
}

fn strip_attrs(attributes: &mut Vec<syn::Attribute>) {
    attributes.retain(|attribute| !is_boltffi_helper_attr(attribute));
}

fn is_boltffi_helper_attr(attribute: &syn::Attribute) -> bool {
    let path = attribute.path();
    if !is_boltffi_helper_path(path) {
        return false;
    }
    match path.segments.last().map(|segment| &segment.ident) {
        Some(identifier)
            if identifier == "skip" || identifier == "name" || identifier == "ffi_stream" =>
        {
            true
        }
        Some(identifier) if identifier == "default" => {
            path.segments.len() == 2 || matches!(attribute.meta, syn::Meta::List(_))
        }
        _ => false,
    }
}

fn is_boltffi_helper_path(path: &syn::Path) -> bool {
    match path.segments.len() {
        1 => true,
        2 => path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "boltffi"),
        _ => false,
    }
}
