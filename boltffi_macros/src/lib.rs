use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{DeriveInput, parse_macro_input};

mod capture;
mod cfg_eval;
mod custom;
mod data;
mod expansion;
mod interned_string;
mod lane;

/// Checks that a type has exactly `#[repr(C)]`; it generates no code.
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

/// Exposes an exported method returning `Arc<EventSubscription<T>>` as a native stream of `item`.
///
/// See [Streaming](https://boltffi.dev/docs/streaming).
#[proc_macro_attribute]
pub fn ffi_stream(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    misplaced(STREAM_PLACEMENT, item)
}

const STREAM_PLACEMENT: &str = "`#[ffi_stream]` goes on a method of an `#[export] impl` block";

const DEFAULT_PLACEMENT: &str =
    "`#[boltffi::default]` goes on a parameter or on a field of a `#[data]` type";

/// Refuses a helper attribute that expanded on its own, so no BoltFFI marker read it.
fn misplaced(message: &str, item: TokenStream) -> TokenStream {
    let error = syn::Error::new(proc_macro2::Span::call_site(), message).to_compile_error();
    let item = proc_macro2::TokenStream::from(item);
    quote! { #error #item }.into()
}

/// Errors for `#[ffi_stream]` and `#[boltffi::default]` on `item` itself, where the marker
/// that expands first would drop them unread.
fn misplaced_helpers(item: &syn::Item) -> proc_macro2::TokenStream {
    let attrs = match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        _ => return proc_macro2::TokenStream::new(),
    };
    attrs
        .iter()
        .filter(|attr| is_boltffi_helper_attr(attr))
        .filter_map(
            |attr| match attr.path().segments.last()?.ident.to_string().as_str() {
                "ffi_stream" => Some(syn::Error::new_spanned(attr, STREAM_PLACEMENT)),
                "default" => Some(syn::Error::new_spanned(attr, DEFAULT_PLACEMENT)),
                _ => None,
            },
        )
        .map(|error| error.to_compile_error())
        .collect()
}

/// Exposes a type you own through its `CustomFfiConvertible` impl; put it on that impl.
///
/// See [Custom types](https://boltffi.dev/docs/custom-types).
#[proc_macro_attribute]
pub fn custom_ffi(attribute: TokenStream, item: TokenStream) -> TokenStream {
    invocation(lane::Kind::CustomFfi, attribute, item)
}

/// Maps a type from another crate, such as `uuid::Uuid`, to an FFI representation and back.
///
/// See [Custom types](https://boltffi.dev/docs/custom-types).
#[proc_macro]
pub fn custom_type(item: TokenStream) -> TokenStream {
    let captured = records(
        lane::Kind::CustomType,
        &proc_macro2::TokenStream::new(),
        item.clone(),
    );
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
    let captured = records(
        lane::Kind::Pool,
        &proc_macro2::TokenStream::new(),
        item.clone(),
    );
    let lane = lane::start(
        lane::Kind::Pool,
        proc_macro2::TokenStream::new(),
        item.clone().into(),
    );
    let expanded = proc_macro2::TokenStream::from(interned_string::interned_string_pool_impl(item));
    append_capture(quote!(#expanded #lane).into(), captured)
}

/// Marks a struct or enum as a record, a value type copied across the boundary.
///
/// `#[data(impl)]` on its `impl` block exports its methods.
///
/// See [Records](https://boltffi.dev/docs/records) and [Types](https://boltffi.dev/docs/types).
#[proc_macro_attribute]
pub fn data(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if attribute.to_string().trim() == "impl" {
        return invocation(lane::Kind::DataImpl, attribute, item);
    }
    syn::parse::<syn::Item>(item.clone())
        .ok()
        .and_then(|mut parsed| {
            refuse_data_on_impl(&parsed, &attribute, &item)
                .or_else(|| refuse_export_item(lane::Kind::Data, &parsed, &item))
                .or_else(|| refuse_second_marker(lane::Kind::Data, &attribute, &mut parsed))
        })
        .unwrap_or_else(|| match attribute.is_empty() {
            true => invocation(lane::Kind::Data, attribute, item),
            false => refuse_arguments(
                lane::Kind::Data,
                proc_macro2::TokenStream::from(attribute),
                item,
                "`#[data]` takes no arguments; `#[data(impl)]` goes on a record's `impl` block",
            ),
        })
}

/// Refuses bare `#[data]` on an `impl` block, and expands the block as `#[data(impl)]`.
fn refuse_data_on_impl(
    parsed: &syn::Item,
    attribute: &TokenStream,
    item: &TokenStream,
) -> Option<TokenStream> {
    let syn::Item::Impl(_) = parsed else {
        return None;
    };
    if !attribute.is_empty() {
        return None;
    }
    let error = syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[data]` on an `impl` block is written `#[data(impl)]`",
    )
    .to_compile_error();
    let expanded = proc_macro2::TokenStream::from(invocation(
        lane::Kind::DataImpl,
        quote!(impl).into(),
        item.clone(),
    ));
    Some(quote! { #error #expanded }.into())
}

/// Refuses arguments `kind`'s marker does not take, and expands the item without them.
fn refuse_arguments(
    kind: lane::Kind,
    arguments: impl ToTokens,
    item: TokenStream,
    message: impl std::fmt::Display,
) -> TokenStream {
    let error = syn::Error::new_spanned(arguments, message).to_compile_error();
    let expanded = proc_macro2::TokenStream::from(invocation(kind, TokenStream::new(), item));
    quote! { #error #expanded }.into()
}

/// The first `#[export]` argument a class does not take, or the whole list when it is not
/// a list of names.
fn unknown_class_argument(attribute: &TokenStream) -> Option<proc_macro2::TokenStream> {
    let attribute = proc_macro2::TokenStream::from(attribute.clone());
    let parser = syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated;
    match syn::parse::Parser::parse2(parser, attribute.clone()) {
        Ok(arguments) => arguments
            .into_iter()
            .find(|argument| argument != "single_threaded" && argument != "thread_unsafe")
            .map(|argument| argument.to_token_stream()),
        Err(_) => Some(attribute),
    }
}

/// Refuses `#[data]` or `#[error]` on a function, trait, constant or union.
fn refuse_export_item(
    kind: lane::Kind,
    parsed: &syn::Item,
    item: &TokenStream,
) -> Option<TokenStream> {
    let advice = match parsed {
        syn::Item::Fn(_) => "use `#[export]` on a function",
        syn::Item::Trait(_) => "use `#[export]` on a trait",
        syn::Item::Const(_) => "use `#[export]` on a constant",
        syn::Item::Union(_) => "BoltFFI can't pass a union",
        _ => return None,
    };
    let error = syn::Error::new(
        proc_macro2::Span::call_site(),
        format!(
            "`#[{}]` declares a struct or enum; {advice}",
            kind.keyword()
        ),
    )
    .to_compile_error();
    let item = proc_macro2::TokenStream::from(strip_boltffi_attrs(item.clone()));
    Some(quote! { #error #item }.into())
}

/// The BoltFFI markers that each declare what a struct or enum is.
const TYPE_MARKERS: [&str; 3] = ["data", "error", "export"];

/// Refuses a struct or enum carrying a second type marker besides the one expanding as
/// `kind`, and expands it once with that marker dropped, since it has not expanded yet.
fn refuse_second_marker(
    kind: lane::Kind,
    attribute: &TokenStream,
    item: &mut syn::Item,
) -> Option<TokenStream> {
    let own = match kind {
        lane::Kind::Data => "data",
        lane::Kind::Error => "error",
        _ => "export",
    };
    let (noun, attrs) = match item {
        syn::Item::Struct(item) => ("struct", &mut item.attrs),
        syn::Item::Enum(item) => ("enum", &mut item.attrs),
        _ => return None,
    };
    let position = attrs
        .iter()
        .position(|attr| type_marker(attr).is_some_and(|marker| marker != own))?;
    let other = type_marker(&attrs[position])?;
    let marker = attrs.remove(position);
    let role = |marker: &str| match (marker, noun) {
        ("data", "struct") => "a `#[data]` record".to_owned(),
        ("data", _) => "a `#[data]` enum".to_owned(),
        ("error", _) => format!("an `#[error]` {noun}"),
        _ => "an `#[export]` class".to_owned(),
    };
    let rank = |marker: &str| TYPE_MARKERS.iter().position(|name| *name == marker);
    let (first, second) = match rank(own) < rank(other) {
        true => (own, other),
        false => (other, own),
    };
    let article = match noun {
        "enum" => "an",
        _ => "a",
    };
    let error = syn::Error::new_spanned(
        marker,
        format!(
            "{article} {noun} is either {} or {}, not both",
            role(first),
            role(second)
        ),
    )
    .to_compile_error();
    let expanded = proc_macro2::TokenStream::from(invocation(
        kind,
        attribute.clone(),
        item.to_token_stream().into(),
    ));
    Some(quote! { #error #expanded }.into())
}

/// The type marker `attr` spells, bare or under `boltffi::`. A bare `error(..)` is
/// thiserror's message, not BoltFFI's marker.
fn type_marker(attr: &syn::Attribute) -> Option<&'static str> {
    let path = attr.path();
    if !is_boltffi_helper_path(path) {
        return None;
    }
    let ident = &path.segments.last()?.ident;
    let message = path.get_ident().is_some() && matches!(attr.meta, syn::Meta::List(_));
    TYPE_MARKERS
        .into_iter()
        .find(|marker| ident == marker && !(message && *marker == "error"))
}

/// Marks a struct or enum as an error, which targets throw natively when it is returned as `Err`.
///
/// See [Errors](https://boltffi.dev/docs/errors).
#[proc_macro_attribute]
pub fn error(attribute: TokenStream, item: TokenStream) -> TokenStream {
    syn::parse::<syn::Item>(item.clone())
        .ok()
        .and_then(|mut parsed| {
            refuse_export_item(lane::Kind::Error, &parsed, &item)
                .or_else(|| refuse_second_marker(lane::Kind::Error, &attribute, &mut parsed))
        })
        .unwrap_or_else(|| match attribute.is_empty() {
            true => invocation(lane::Kind::Error, attribute, item),
            false => refuse_arguments(
                lane::Kind::Error,
                proc_macro2::TokenStream::from(attribute),
                item,
                "`#[error]` takes no arguments",
            ),
        })
}

/// Exports a function, constant, class (on its struct or enum), `impl` block or callback trait.
///
/// See [Functions](https://boltffi.dev/docs/functions),
/// [Classes](https://boltffi.dev/docs/classes), [Callbacks](https://boltffi.dev/docs/callbacks)
/// and [Constants](https://boltffi.dev/docs/constants).
#[proc_macro_attribute]
pub fn export(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let mut parsed = match syn::parse::<syn::Item>(item.clone()) {
        Ok(parsed) => parsed,
        Err(error) => return error.to_compile_error().into(),
    };
    if let Some(refused) = refuse_second_marker(lane::Kind::Export, &attribute, &mut parsed) {
        return refused;
    }
    match parsed {
        syn::Item::Impl(block) if !attribute.is_empty() => {
            let attribute = proc_macro2::TokenStream::from(attribute);
            let name = match &*block.self_ty {
                syn::Type::Path(path) => path
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string()),
                _ => None,
            }
            .unwrap_or_else(|| block.self_ty.to_token_stream().to_string());
            let advice = match syn::parse2::<syn::Ident>(attribute.clone()) {
                Ok(argument) if argument == "single_threaded" => format!(
                    "; write `#[export(single_threaded)]` on `{name}`, which declares the class"
                ),
                Ok(argument) if argument == "methods" => format!(
                    "; every `#[export] impl` adds methods to the class that `#[export]` on \
                     `{name}` declares"
                ),
                _ => String::new(),
            };
            syn::Error::new_spanned(
                &attribute,
                format!("`#[export] impl` takes no arguments{advice}"),
            )
            .to_compile_error()
            .into()
        }
        syn::Item::Impl(_) => invocation(lane::Kind::ExportMethods, attribute, item),
        syn::Item::Const(_) | syn::Item::Fn(_) | syn::Item::Trait(_) if !attribute.is_empty() => {
            let noun = match parsed {
                syn::Item::Const(_) => "constant",
                syn::Item::Fn(_) => "function",
                _ => "callback trait",
            };
            refuse_arguments(
                lane::Kind::Export,
                proc_macro2::TokenStream::from(attribute),
                item,
                format!("`#[export]` on a {noun} takes no arguments"),
            )
        }
        syn::Item::Enum(_) | syn::Item::Struct(_) => match unknown_class_argument(&attribute) {
            Some(argument) => refuse_arguments(
                lane::Kind::Export,
                &argument,
                item,
                format!(
                    "`{argument}` is not an `#[export]` argument; a class takes `single_threaded`"
                ),
            ),
            None => invocation(lane::Kind::Export, attribute, item),
        },
        syn::Item::Const(_) | syn::Item::Fn(_) | syn::Item::Trait(_) => {
            invocation(lane::Kind::Export, attribute, item)
        }
        item => syn::Error::new_spanned(
            item,
            "export can only be applied to const, enum, fn, impl, struct, or trait",
        )
        .to_compile_error()
        .into(),
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
    let misplaced = parsed.as_ref().map(misplaced_helpers).unwrap_or_default();
    let emitted = match kind {
        lane::Kind::Data | lane::Kind::Error => match data::repr::materialize(item.clone()) {
            Ok(item) => strip_boltffi_attrs(item),
            Err(error) => return error,
        },
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
    quote!(#misplaced #emitted #identity #described).into()
}

/// The source records bindgen reads for one invocation.
fn records(
    kind: lane::Kind,
    attribute: &proc_macro2::TokenStream,
    item: TokenStream,
) -> proc_macro2::TokenStream {
    match kind {
        lane::Kind::Data
        | lane::Kind::Error
        | lane::Kind::Export
        | lane::Kind::DataImpl
        | lane::Kind::ExportMethods => {
            capture::item_tokens(item, kind.capture(attribute), kind == lane::Kind::Error)
        }
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

/// Call once at the root of every crate that uses BoltFFI; the other macros expand against it.
///
/// See [Getting started](https://boltffi.dev/docs/getting-started).
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

/// Keeps a method or associated constant of an exported `impl` block or callback trait out of the
/// bindings.
///
/// See [Classes](https://boltffi.dev/docs/classes).
#[proc_macro_attribute]
pub fn skip(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Accepted on exported items and currently has no effect.
#[proc_macro_attribute]
pub fn name(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Gives a record field, enum variant field or parameter a default value, which targets with
/// default arguments make optional.
///
/// See [Records](https://boltffi.dev/docs/records) and
/// [Functions](https://boltffi.dev/docs/functions).
#[proc_macro_attribute]
pub fn default(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    misplaced(DEFAULT_PLACEMENT, item)
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
