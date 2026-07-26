use boltffi_ast::{MethodDef, TraitDef, TraitId};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::marker::{self, Disposition};
use crate::type_expr::Scanner;
use crate::{ModuleScope, ScanError, attributes, name, unsupported};

use super::{signature, stream};

pub fn scan(
    marked: &Marked<'_, syn::ItemTrait>,
    declared_types: &DeclaredTypes,
) -> Result<TraitDef, ScanError> {
    build(marked.item(), marked.scope(), declared_types)
}

fn build(
    item: &syn::ItemTrait,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<TraitDef, ScanError> {
    let item_name = format!("trait {}", item.ident);
    unsupported::generics(&item.generics, &item_name)?;
    unsupported::unsafety(item.unsafety.as_ref(), &item_name)?;
    supertraits(&item.supertraits, &item_name)?;

    let id = TraitId::new(scope.path().qualified(&item.ident.to_string()));
    let mut callback = TraitDef::new(id, name::source(&item.ident));
    let scanner = Scanner::new(declared_types, scope);
    let attrs = Attributes::new(&item.attrs, &scanner);
    callback.source = attributes::source(&item.vis, scope, item.span());
    callback.source_span = callback.source.span.clone();
    callback.doc = attrs.doc();
    callback.deprecated = attrs.deprecated()?;
    callback.user_attrs = attrs.user_attrs();
    callback.methods = methods(item, callback.id.as_str(), &scanner)?;
    Ok(callback)
}

fn methods(
    item: &syn::ItemTrait,
    parent: &str,
    scanner: &Scanner<'_>,
) -> Result<Vec<MethodDef>, ScanError> {
    let trait_name = item.ident.to_string();
    item.items
        .iter()
        .filter_map(|trait_item| match trait_item {
            syn::TraitItem::Fn(method) => match is_exported_method(method) {
                Ok(true) => Some(method_from_signature(method, parent, &trait_name, scanner)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
            other => Some(Err(unsupported_trait_item(other, &trait_name))),
        })
        .collect()
}

fn supertraits(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
    item: &str,
) -> Result<(), ScanError> {
    bounds
        .iter()
        .all(|bound| match bound {
            syn::TypeParamBound::Trait(bound) => is_supported_supertrait(bound),
            _ => false,
        })
        .then_some(())
        .ok_or_else(|| ScanError::UnsupportedSupertraits {
            item: item.to_owned(),
        })
}

fn is_supported_supertrait(bound: &syn::TraitBound) -> bool {
    matches!(bound.modifier, syn::TraitBoundModifier::None)
        && bound.lifetimes.is_none()
        && bound.path.segments.last().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "Send" | "Sync" | "Debug"
            )
        })
}

fn is_exported_method(method: &syn::TraitItemFn) -> Result<bool, ScanError> {
    if stream::Attribute::scan(&method.attrs)?.is_some() {
        return Err(stream::Attribute::invalid_placement("trait method"));
    }
    match marker::disposition(&method.attrs)? {
        Disposition::Skip => Ok(false),
        Disposition::Reject(marker) => Err(marker.invalid_placement("trait method")),
        Disposition::Unmarked => Ok(true),
    }
}

fn method_from_signature(
    method: &syn::TraitItemFn,
    parent: &str,
    trait_name: &str,
    scanner: &Scanner<'_>,
) -> Result<MethodDef, ScanError> {
    if method.default.is_some() {
        return Err(ScanError::UnsupportedTraitMethodBody {
            item: format!("trait {trait_name}::{}", method.sig.ident),
        });
    }
    signature::method(
        &method.sig,
        &method.attrs,
        attributes::public_source(scanner.scope(), method.span()),
        parent,
        scanner,
        signature::MethodReturns::Trait,
    )
}

fn unsupported_trait_item(item: &syn::TraitItem, parent: &str) -> ScanError {
    ScanError::UnsupportedTraitItem {
        item: format!("trait {parent}::{}", trait_item_name(item)),
    }
}

fn trait_item_name(item: &syn::TraitItem) -> String {
    match item {
        syn::TraitItem::Const(item) => item.ident.to_string(),
        syn::TraitItem::Type(item) => item.ident.to_string(),
        syn::TraitItem::Macro(item) => item
            .mac
            .path
            .segments
            .last()
            .map_or_else(|| "macro".to_owned(), |segment| segment.ident.to_string()),
        _ => "item".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        CanonicalName, ExecutionKind, NamePart, Primitive, Receiver, ReturnDef, Source, TraitId,
        TypeExpr, Visibility,
    };

    fn parse(source: &str) -> syn::ItemTrait {
        syn::parse_str(source).expect("valid trait source")
    }

    fn scan(source: &str) -> Result<TraitDef, ScanError> {
        build(
            &parse(source),
            &ModuleScope::root("demo"),
            &DeclaredTypes::new(),
        )
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn scans_complete_callback_trait_contract() {
        let callback = scan("pub trait ValueCallback { fn on_value(&self, value: i32) -> i64; }")
            .expect("scan");

        assert_eq!(callback.id, TraitId::new("demo::ValueCallback"));
        assert_eq!(callback.name.canonical(), &name(&["value", "callback"]));
        assert_eq!(callback.source, Source::new(Visibility::Public, None));
        assert_eq!(callback.methods.len(), 1);
        assert_eq!(
            callback.methods[0].id,
            "demo::ValueCallback::on_value".into()
        );
        assert_eq!(
            callback.methods[0].name.canonical(),
            &name(&["on", "value"])
        );
        assert_eq!(callback.methods[0].receiver, Receiver::Shared);
        assert_eq!(callback.methods[0].execution, ExecutionKind::Sync);
        assert_eq!(
            callback.methods[0].parameters[0].type_expr,
            TypeExpr::Primitive(Primitive::I32)
        );
        assert_eq!(
            callback.methods[0].returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::I64))
        );
    }

    #[test]
    fn scans_async_callback_method_execution() {
        let callback =
            scan("trait Loader { async fn load(&self, key: u32) -> u64; }").expect("scan");

        assert_eq!(callback.methods[0].execution, ExecutionKind::Async);
    }

    #[test]
    fn rejects_borrowed_string_callback_return() {
        assert!(scan("trait Labeler { fn label(&self) -> &'static str; }").is_err());
    }

    #[test]
    fn accepts_supported_callback_supertraits() {
        let callback =
            scan("trait Listener: Send + Sync + Debug { fn call(&self); }").expect("scan");

        assert_eq!(callback.id, TraitId::new("demo::Listener"));
        assert_eq!(callback.methods.len(), 1);
    }

    #[test]
    fn skips_explicitly_skipped_callback_methods() {
        let callback =
            scan("trait Listener { #[skip] fn local(&self); fn remote(&self); }").expect("scan");

        assert_eq!(callback.methods.len(), 1);
        assert_eq!(callback.methods[0].name.canonical(), &name(&["remote"]));
    }

    #[test]
    fn rejects_stream_marker_on_callback_methods() {
        let error = scan(
            "trait Listener { #[ffi_stream(item = i32)] fn values(&self) -> Arc<EventSubscription<i32>>; }",
        )
        .expect_err("stream marker belongs to class methods");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "ffi_stream".to_owned(),
                item: "trait method".to_owned()
            }
        );
    }

    #[test]
    fn rejects_trait_shapes_erased_by_the_ast_contract() {
        let generic =
            scan("trait Listener<T> { fn call(&self, value: T); }").expect_err("generic rejected");
        let unsafe_trait =
            scan("unsafe trait Listener { fn call(&self); }").expect_err("unsafe rejected");
        let supertrait =
            scan("trait Listener: Display { fn call(&self); }").expect_err("supertrait rejected");

        assert_eq!(
            generic,
            ScanError::UnsupportedGenerics {
                item: "trait Listener".to_owned()
            }
        );
        assert_eq!(
            unsafe_trait,
            ScanError::UnsupportedUnsafe {
                item: "trait Listener".to_owned()
            }
        );
        assert_eq!(
            supertrait,
            ScanError::UnsupportedSupertraits {
                item: "trait Listener".to_owned()
            }
        );
    }

    #[test]
    fn rejects_unrepresentable_trait_items() {
        let associated_type =
            scan("trait Listener { type Value; fn call(&self); }").expect_err("type rejected");
        let associated_const = scan("trait Listener { const LIMIT: usize; fn call(&self); }")
            .expect_err("const rejected");
        let default_body = scan("trait Listener { fn call(&self) {} }").expect_err("body rejected");

        assert_eq!(
            associated_type,
            ScanError::UnsupportedTraitItem {
                item: "trait Listener::Value".to_owned()
            }
        );
        assert_eq!(
            associated_const,
            ScanError::UnsupportedTraitItem {
                item: "trait Listener::LIMIT".to_owned()
            }
        );
        assert_eq!(
            default_body,
            ScanError::UnsupportedTraitMethodBody {
                item: "trait Listener::call".to_owned()
            }
        );
    }

    #[test]
    fn rejects_non_skip_boltffi_markers_on_trait_methods() {
        let error = scan("trait Listener { #[export] fn local(&self); }")
            .expect_err("misplaced marker must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "trait method".to_owned()
            }
        );
    }
}
