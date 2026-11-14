use boltffi_binding::{SourceLocation, SourcePosition};
use proc_macro2::{Span, TokenStream};
use quote::ToTokens;

use super::error::Error;

/// The error as a `compile_error!` at the part of `item` it names, or at the call site.
pub fn compile_error(item: Option<&syn::Item>, error: &Error) -> TokenStream {
    let message = format!("BoltFFI: {error}");
    match item
        .zip(error.location())
        .and_then(|(item, location)| tokens(item, location))
    {
        Some(tokens) => syn::Error::new_spanned(tokens, message),
        None => syn::Error::new(Span::call_site(), message),
    }
    .to_compile_error()
}

fn tokens(item: &syn::Item, location: &SourceLocation) -> Option<TokenStream> {
    if let Some(declaration) = location.declaration() {
        let name = declaration.rsplit("::").next()?;
        if unraw(&item_name(item)?) != unraw(name) {
            return None;
        }
    }
    match (item, location.member()) {
        (syn::Item::Fn(function), None) => signature(&function.sig, location.position()),
        (syn::Item::Struct(record), None) => fields(&record.fields, location.position()),
        (syn::Item::Impl(block), Some(member)) => block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) if named(&method.sig.ident, member) => Some(&method.sig),
                _ => None,
            })
            .and_then(|sig| signature(sig, location.position())),
        (syn::Item::Trait(source), Some(member)) => source
            .items
            .iter()
            .find_map(|item| match item {
                syn::TraitItem::Fn(method) if named(&method.sig.ident, member) => Some(&method.sig),
                _ => None,
            })
            .and_then(|sig| signature(sig, location.position())),
        (syn::Item::Enum(enumeration), Some(member)) => enumeration
            .variants
            .iter()
            .find(|variant| named(&variant.ident, member))
            .and_then(|variant| match location.position() {
                None => Some(variant.ident.to_token_stream()),
                position => fields(&variant.fields, position),
            }),
        _ => None,
    }
}

fn signature(sig: &syn::Signature, position: Option<SourcePosition>) -> Option<TokenStream> {
    let Some(position) = position else {
        return Some(sig.ident.to_token_stream());
    };
    match position {
        SourcePosition::Receiver => Some(match sig.receiver() {
            Some(receiver) => receiver.to_token_stream(),
            None => sig.ident.to_token_stream(),
        }),
        SourcePosition::Parameter(index) => sig
            .inputs
            .iter()
            .filter_map(|input| match input {
                syn::FnArg::Typed(parameter) => Some(parameter),
                syn::FnArg::Receiver(_) => None,
            })
            .nth(index)
            .map(ToTokens::to_token_stream),
        SourcePosition::Return => Some(match &sig.output {
            syn::ReturnType::Type(..) => sig.output.to_token_stream(),
            syn::ReturnType::Default => sig.ident.to_token_stream(),
        }),
        _ => None,
    }
}

fn fields(fields: &syn::Fields, position: Option<SourcePosition>) -> Option<TokenStream> {
    match position? {
        SourcePosition::Field(index) => fields.iter().nth(index).map(ToTokens::to_token_stream),
        _ => None,
    }
}

fn item_name(item: &syn::Item) -> Option<String> {
    Some(match item {
        syn::Item::Fn(function) => function.sig.ident.to_string(),
        syn::Item::Struct(record) => record.ident.to_string(),
        syn::Item::Enum(enumeration) => enumeration.ident.to_string(),
        syn::Item::Trait(source) => source.ident.to_string(),
        syn::Item::Const(constant) => constant.ident.to_string(),
        syn::Item::Impl(block) => match &*block.self_ty {
            syn::Type::Path(path) => path.path.segments.last()?.ident.to_string(),
            _ => return None,
        },
        _ => return None,
    })
}

fn named(ident: &syn::Ident, name: &str) -> bool {
    unraw(&ident.to_string()) == unraw(name)
}

fn unraw(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}
