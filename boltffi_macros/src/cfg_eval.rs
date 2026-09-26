//! Conditional members, evaluated by the compiler before an invocation describes them.
//!
//! An attribute macro sees `#[cfg]` and `#[cfg_attr]` unevaluated, but a derive sees its
//! input after they are evaluated. An item that uses either therefore also emits a hidden
//! enum with one variant per member and per member `cfg_attr`, each carrying the
//! predicate it depends on. A derive on that enum learns which survived, rebuilds the item
//! as compiled, and continues the invocation with it.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{Attribute, Item, Meta, Token};

use crate::lane::Kind;

const HELPER: &str = "boltffi_cfg_eval";

/// Whether `item` or any of its members depends on configuration.
pub(crate) fn is_conditional(item: &Item) -> bool {
    use syn::visit::Visit;
    #[derive(Default)]
    struct Conditional(bool);
    impl<'ast> Visit<'ast> for Conditional {
        fn visit_attribute(&mut self, attribute: &'ast Attribute) {
            self.0 |= is_cfg(attribute) || is_cfg_attr(attribute);
        }
    }
    let mut conditional = Conditional::default();
    conditional.visit_item(item);
    conditional.0
}

/// Defers `item`'s invocation until the compiler has evaluated its configuration.
pub(crate) fn defer(
    kind: Kind,
    attribute: TokenStream,
    raw: TokenStream,
    item: &Item,
) -> TokenStream {
    if let Some(attribute) = conditional_parameter(item) {
        return syn::Error::new_spanned(
            attribute,
            "BoltFFI does not support `#[cfg]` on parameters; gate the whole function instead",
        )
        .to_compile_error();
    }
    let facade = crate::capture::facade();
    let keyword = format_ident!("{}", kind.keyword());
    let name = format_ident!(
        "__BoltffiCfgEval_{}_{}_{}",
        kind.keyword(),
        item_name(item),
        crate::lane::LANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let item_cfgs = item_attrs(item)
        .iter()
        .filter_map(presence)
        .collect::<Vec<_>>();
    let variants = members(item)
        .into_iter()
        .flat_map(|(key, attrs)| {
            let presence = attrs.iter().filter_map(presence).collect::<Vec<_>>();
            let member = format_ident!("{key}");
            let member_variant = quote! { #(#presence)* #member };
            let attribute_variants = attrs
                .iter()
                .filter(|attribute| is_cfg_attr(attribute))
                .enumerate()
                .filter_map(|(index, attribute)| {
                    let (predicate, _) = split_cfg_attr(attribute)?;
                    let variant = format_ident!("{key}A{index}");
                    Some(quote! { #[cfg(#predicate)] #variant })
                })
                .collect::<Vec<_>>();
            std::iter::once(member_variant).chain(attribute_variants)
        })
        .collect::<Vec<_>>();
    quote! {
        #(#item_cfgs)*
        #[doc(hidden)]
        #[allow(dead_code, non_camel_case_types)]
        #[derive(#facade::__private::CfgEval)]
        #[boltffi_cfg_eval(#keyword { #attribute } { #raw })]
        enum #name {
            __BoltffiItem,
            #(#variants,)*
        }
    }
}

/// The deferred invocation, arriving with the members the compiler kept.
pub(crate) struct Evaluated {
    pub(crate) kind: Kind,
    pub(crate) attribute: TokenStream,
    pub(crate) item: Item,
}

/// Rebuilds the deferred item from what survived configuration.
pub(crate) fn evaluate(input: syn::DeriveInput) -> syn::Result<Evaluated> {
    let syn::Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input,
            "BoltFFI configuration probe is not an enum",
        ));
    };
    let active = data
        .variants
        .iter()
        .map(|variant| variant.ident.to_string())
        .collect::<HashSet<_>>();
    let helper = input
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident(HELPER))
        .ok_or_else(|| {
            syn::Error::new_spanned(&input, "BoltFFI configuration probe lost its item")
        })?;
    let (kind, attribute, raw) = helper.parse_args_with(parse_helper)?;
    let mut item = syn::parse2::<Item>(raw)?;
    strip_item_level(&mut item);
    retain_members(&mut item, &active);
    Ok(Evaluated {
        kind,
        attribute,
        item,
    })
}

fn parse_helper(input: syn::parse::ParseStream) -> syn::Result<(Kind, TokenStream, TokenStream)> {
    let keyword: syn::Ident = input.parse()?;
    let kind = Kind::parse(&keyword.to_string())
        .ok_or_else(|| syn::Error::new(keyword.span(), "unknown BoltFFI invocation"))?;
    let attribute;
    syn::braced!(attribute in input);
    let raw;
    syn::braced!(raw in input);
    Ok((kind, attribute.parse()?, raw.parse()?))
}

fn item_name(item: &Item) -> String {
    let name = match item {
        Item::Struct(item) => item.ident.to_string(),
        Item::Enum(item) => item.ident.to_string(),
        Item::Trait(item) => item.ident.to_string(),
        Item::Fn(item) => item.sig.ident.to_string(),
        Item::Const(item) => item.ident.to_string(),
        Item::Impl(item) => match &*item.self_ty {
            syn::Type::Path(path) => path
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        },
        _ => String::new(),
    };
    name.trim_start_matches("r#").to_owned()
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Struct(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Const(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        _ => &[],
    }
}

/// Every member with its attributes, keyed the way the probe enum names it.
fn members(item: &Item) -> Vec<(String, &[Attribute])> {
    match item {
        Item::Struct(item) => item
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| (format!("F{index}"), field.attrs.as_slice()))
            .collect(),
        Item::Enum(item) => item
            .variants
            .iter()
            .enumerate()
            .flat_map(|(index, variant)| {
                std::iter::once((format!("V{index}"), variant.attrs.as_slice())).chain(
                    variant
                        .fields
                        .iter()
                        .enumerate()
                        .map(move |(field, value)| {
                            (format!("V{index}F{field}"), value.attrs.as_slice())
                        }),
                )
            })
            .collect(),
        Item::Impl(item) => item
            .items
            .iter()
            .enumerate()
            .map(|(index, member)| (format!("I{index}"), impl_item_attrs(member)))
            .collect(),
        Item::Trait(item) => item
            .items
            .iter()
            .enumerate()
            .map(|(index, member)| (format!("I{index}"), trait_item_attrs(member)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The first parameter attribute that decides whether its parameter exists.
fn conditional_parameter(item: &Item) -> Option<&Attribute> {
    let signatures: Vec<&syn::Signature> = match item {
        Item::Fn(item) => vec![&item.sig],
        Item::Impl(item) => item
            .items
            .iter()
            .filter_map(|member| match member {
                syn::ImplItem::Fn(member) => Some(&member.sig),
                _ => None,
            })
            .collect(),
        Item::Trait(item) => item
            .items
            .iter()
            .filter_map(|member| match member {
                syn::TraitItem::Fn(member) => Some(&member.sig),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    signatures
        .into_iter()
        .flat_map(|signature| &signature.inputs)
        .flat_map(|input| match input {
            syn::FnArg::Receiver(receiver) => &receiver.attrs,
            syn::FnArg::Typed(typed) => &typed.attrs,
        })
        .find(|attribute| presence(attribute).is_some())
}

fn impl_item_attrs(member: &syn::ImplItem) -> &[Attribute] {
    match member {
        syn::ImplItem::Const(member) => &member.attrs,
        syn::ImplItem::Fn(member) => &member.attrs,
        syn::ImplItem::Type(member) => &member.attrs,
        syn::ImplItem::Macro(member) => &member.attrs,
        _ => &[],
    }
}

fn trait_item_attrs(member: &syn::TraitItem) -> &[Attribute] {
    match member {
        syn::TraitItem::Const(member) => &member.attrs,
        syn::TraitItem::Fn(member) => &member.attrs,
        syn::TraitItem::Type(member) => &member.attrs,
        syn::TraitItem::Macro(member) => &member.attrs,
        _ => &[],
    }
}

fn is_cfg(attribute: &Attribute) -> bool {
    attribute.path().is_ident("cfg")
}

fn is_cfg_attr(attribute: &Attribute) -> bool {
    attribute.path().is_ident("cfg_attr")
}

/// The part of an attribute that decides whether its member exists at all.
fn presence(attribute: &Attribute) -> Option<TokenStream> {
    if is_cfg(attribute) {
        return Some(attribute.to_token_stream());
    }
    let (predicate, metas) = split_cfg_attr(attribute)?;
    let cfgs = metas
        .into_iter()
        .filter(|meta| meta.path().is_ident("cfg"))
        .collect::<Vec<_>>();
    (!cfgs.is_empty()).then(|| quote! { #[cfg_attr(#predicate, #(#cfgs),*)] })
}

fn split_cfg_attr(attribute: &Attribute) -> Option<(Meta, Vec<Meta>)> {
    if !is_cfg_attr(attribute) {
        return None;
    }
    let metas = attribute
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;
    let mut metas = metas.into_iter();
    let predicate = metas.next()?;
    Some((predicate, metas.collect()))
}

fn strip_item_level(item: &mut Item) {
    let attrs = match item {
        Item::Struct(item) => &mut item.attrs,
        Item::Enum(item) => &mut item.attrs,
        Item::Trait(item) => &mut item.attrs,
        Item::Fn(item) => &mut item.attrs,
        Item::Const(item) => &mut item.attrs,
        Item::Impl(item) => &mut item.attrs,
        _ => return,
    };
    attrs.retain(|attribute| !is_cfg(attribute) && !is_cfg_attr(attribute));
}

/// Keeps the members the compiler kept, with their `cfg_attr`s applied as compiled.
fn retain_members(item: &mut Item, active: &HashSet<String>) {
    match item {
        Item::Struct(item) => {
            let mut index = 0;
            retain_fields(&mut item.fields, active, |_| {
                let key = format!("F{index}");
                index += 1;
                key
            });
        }
        Item::Enum(item) => {
            let variants = std::mem::take(&mut item.variants);
            item.variants = variants
                .into_iter()
                .enumerate()
                .filter_map(|(index, mut variant)| {
                    let key = format!("V{index}");
                    if !active.contains(&key) {
                        return None;
                    }
                    apply(&mut variant.attrs, &key, active);
                    let mut field = 0;
                    retain_fields(&mut variant.fields, active, |_| {
                        let key = format!("V{index}F{field}");
                        field += 1;
                        key
                    });
                    Some(variant)
                })
                .collect();
        }
        Item::Impl(item) => {
            let members = std::mem::take(&mut item.items);
            item.items = members
                .into_iter()
                .enumerate()
                .filter_map(|(index, mut member)| {
                    let key = format!("I{index}");
                    active.contains(&key).then(|| {
                        if let Some(attrs) = impl_item_attrs_mut(&mut member) {
                            apply(attrs, &key, active);
                        }
                        member
                    })
                })
                .collect();
        }
        Item::Trait(item) => {
            let members = std::mem::take(&mut item.items);
            item.items = members
                .into_iter()
                .enumerate()
                .filter_map(|(index, mut member)| {
                    let key = format!("I{index}");
                    active.contains(&key).then(|| {
                        if let Some(attrs) = trait_item_attrs_mut(&mut member) {
                            apply(attrs, &key, active);
                        }
                        member
                    })
                })
                .collect();
        }
        _ => {}
    }
}

fn retain_fields(
    fields: &mut syn::Fields,
    active: &HashSet<String>,
    mut key: impl FnMut(&syn::Field) -> String,
) {
    let mut retain = |fields: Punctuated<syn::Field, Token![,]>| {
        fields
            .into_iter()
            .filter_map(|mut field| {
                let key = key(&field);
                active.contains(&key).then(|| {
                    apply(&mut field.attrs, &key, active);
                    field
                })
            })
            .collect()
    };
    match fields {
        syn::Fields::Named(named) => named.named = retain(std::mem::take(&mut named.named)),
        syn::Fields::Unnamed(unnamed) => {
            unnamed.unnamed = retain(std::mem::take(&mut unnamed.unnamed));
        }
        syn::Fields::Unit => {}
    }
}

/// Drops `cfg`s that already held and expands each `cfg_attr` the way the compiler did.
fn apply(attrs: &mut Vec<Attribute>, key: &str, active: &HashSet<String>) {
    let mut cfg_attr = 0;
    let evaluated = std::mem::take(attrs)
        .into_iter()
        .flat_map(|attribute| {
            if is_cfg(&attribute) {
                return Vec::new();
            }
            let Some((_, metas)) = split_cfg_attr(&attribute) else {
                return vec![attribute];
            };
            let holds = active.contains(&format!("{key}A{cfg_attr}"));
            cfg_attr += 1;
            if !holds {
                return Vec::new();
            }
            metas
                .into_iter()
                .filter(|meta| !meta.path().is_ident("cfg"))
                .map(|meta| syn::parse_quote!(#[#meta]))
                .collect()
        })
        .collect();
    *attrs = evaluated;
}

fn impl_item_attrs_mut(member: &mut syn::ImplItem) -> Option<&mut Vec<Attribute>> {
    match member {
        syn::ImplItem::Const(member) => Some(&mut member.attrs),
        syn::ImplItem::Fn(member) => Some(&mut member.attrs),
        syn::ImplItem::Type(member) => Some(&mut member.attrs),
        syn::ImplItem::Macro(member) => Some(&mut member.attrs),
        _ => None,
    }
}

fn trait_item_attrs_mut(member: &mut syn::TraitItem) -> Option<&mut Vec<Attribute>> {
    match member {
        syn::TraitItem::Const(member) => Some(&mut member.attrs),
        syn::TraitItem::Fn(member) => Some(&mut member.attrs),
        syn::TraitItem::Type(member) => Some(&mut member.attrs),
        syn::TraitItem::Macro(member) => Some(&mut member.attrs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{conditional_parameter, defer};
    use crate::lane::Kind;

    #[test]
    fn method_blocks_on_one_type_get_distinct_probes() {
        let item: syn::Item = syn::parse_quote! {
            impl Point {
                #[cfg(any())]
                pub fn hidden(&self) {}
            }
        };
        let probe_name = |tokens: proc_macro2::TokenStream| {
            syn::parse2::<syn::File>(tokens)
                .expect("probe parses")
                .items
                .into_iter()
                .find_map(|item| match item {
                    syn::Item::Enum(probe) => Some(probe.ident),
                    _ => None,
                })
                .expect("probe enum")
        };

        let first = probe_name(defer(
            Kind::DataImpl,
            quote::quote!(impl),
            quote::quote!(#item),
            &item,
        ));
        let second = probe_name(defer(
            Kind::DataImpl,
            quote::quote!(impl),
            quote::quote!(#item),
            &item,
        ));

        assert_ne!(first, second);
    }

    #[test]
    fn parameter_cfgs_are_found_in_functions_methods_and_traits() {
        let function: syn::Item = syn::parse_quote! {
            pub fn take(#[cfg(any())] a: u32, b: u32) -> u32 { b }
        };
        let method: syn::Item = syn::parse_quote! {
            impl Counter {
                pub fn take(&self, #[cfg_attr(feature = "x", cfg(any()))] a: u32) {}
            }
        };
        let required: syn::Item = syn::parse_quote! {
            pub trait Sink {
                fn take(&self, #[cfg(any())] a: u32);
            }
        };
        let lint_only: syn::Item = syn::parse_quote! {
            pub fn take(#[cfg_attr(test, allow(unused))] a: u32) {}
        };

        assert!(conditional_parameter(&function).is_some());
        assert!(conditional_parameter(&method).is_some());
        assert!(conditional_parameter(&required).is_some());
        assert!(conditional_parameter(&lint_only).is_none());
    }
}
