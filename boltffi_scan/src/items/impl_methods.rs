use boltffi_ast::MethodDef;
use syn::spanned::Spanned;

use crate::declared_types::DeclaredTypes;
use crate::marker::{self, Disposition};
use crate::type_expr::Scanner;
use crate::{ModuleScope, ScanError, attributes};

use super::{signature, stream};

pub(super) fn class_methods(
    item: &syn::ItemImpl,
    parent: &str,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<Vec<MethodDef>, ScanError> {
    scan(item, parent, scope, declared_types, StreamMethods::Separate)
}

pub(super) fn value_methods(
    item: &syn::ItemImpl,
    parent: &str,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<Vec<MethodDef>, ScanError> {
    scan(item, parent, scope, declared_types, StreamMethods::Reject)
}

fn scan(
    item: &syn::ItemImpl,
    parent: &str,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
    stream_methods: StreamMethods,
) -> Result<Vec<MethodDef>, ScanError> {
    let scanner = Scanner::new(declared_types, scope);
    item.items
        .iter()
        .filter_map(|impl_item| scan_item(impl_item, parent, &scanner, stream_methods))
        .collect()
}

#[derive(Clone, Copy)]
enum StreamMethods {
    Separate,
    Reject,
}

impl StreamMethods {
    fn classify(self, method: &syn::ImplItemFn) -> Result<MethodKind, ScanError> {
        match (self, stream::Attribute::scan(&method.attrs)?) {
            (Self::Reject, Some(_)) => Err(stream::Attribute::invalid_placement("method")),
            (Self::Separate, Some(_)) => {
                if exported_method(&method.attrs, &method.vis, "stream method")? {
                    Ok(MethodKind::Stream)
                } else {
                    Err(stream::Attribute::invalid_placement("stream method"))
                }
            }
            (_, None) => Ok(MethodKind::Normal),
        }
    }
}

enum MethodKind {
    Normal,
    Stream,
}

fn scan_item(
    item: &syn::ImplItem,
    parent: &str,
    scanner: &Scanner<'_>,
    stream_methods: StreamMethods,
) -> Option<Result<MethodDef, ScanError>> {
    match item {
        syn::ImplItem::Fn(method) => match stream_methods.classify(method) {
            Ok(MethodKind::Normal) => match exported_method(&method.attrs, &method.vis, "method") {
                Ok(true) => Some(signature::method(
                    &method.sig,
                    &method.attrs,
                    attributes::source(&method.vis, scanner.scope(), method.span()),
                    parent,
                    scanner,
                    signature::MethodReturns::Export,
                )),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
            Ok(MethodKind::Stream) => None,
            Err(error) => Some(Err(error)),
        },
        syn::ImplItem::Const(item) => non_method(
            &item.attrs,
            Some(&item.vis),
            parent,
            item.ident.to_string(),
            "associated const",
        ),
        syn::ImplItem::Type(item) => non_method(
            &item.attrs,
            Some(&item.vis),
            parent,
            item.ident.to_string(),
            "associated type",
        ),
        syn::ImplItem::Macro(item) => non_method(
            &item.attrs,
            None,
            parent,
            item.mac
                .path
                .segments
                .last()
                .map_or_else(|| "macro".to_owned(), |segment| segment.ident.to_string()),
            "macro",
        ),
        _ => Some(Err(ScanError::UnsupportedImplItem {
            item: format!("{parent}::item"),
        })),
    }
}

pub(super) fn exported_method(
    attrs: &[syn::Attribute],
    visibility: &syn::Visibility,
    item: &str,
) -> Result<bool, ScanError> {
    match marker::disposition(attrs)? {
        Disposition::Skip => Ok(false),
        Disposition::Reject(marker) => Err(marker.invalid_placement(item)),
        Disposition::Unmarked => Ok(matches!(visibility, syn::Visibility::Public(_))),
    }
}

fn non_method(
    attrs: &[syn::Attribute],
    visibility: Option<&syn::Visibility>,
    parent: &str,
    name: String,
    item: &str,
) -> Option<Result<MethodDef, ScanError>> {
    match stream::Attribute::scan(attrs) {
        Ok(Some(_)) => return Some(Err(stream::Attribute::invalid_placement(item))),
        Ok(None) => {}
        Err(error) => return Some(Err(error)),
    }
    match marker::disposition(attrs) {
        Ok(Disposition::Skip) => None,
        Ok(Disposition::Reject(marker)) => Some(Err(marker.invalid_placement(item))),
        Ok(Disposition::Unmarked)
            if visibility
                .is_some_and(|visibility| !matches!(visibility, syn::Visibility::Public(_))) =>
        {
            None
        }
        Ok(Disposition::Unmarked) => Some(Err(ScanError::UnsupportedImplItem {
            item: format!("{parent}::{name}"),
        })),
        Err(error) => Some(Err(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleScope;
    use crate::declared_types::DeclaredTypes;
    use boltffi_ast::{
        CanonicalName, MethodDef, MethodId, NamePart, Path, Primitive, Receiver, RecordId,
        ReturnDef, TypeExpr,
    };

    fn parse(source: &str) -> syn::ItemImpl {
        syn::parse_str(source).expect("valid impl block")
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    fn scan(
        source: &str,
        parent: &str,
        declared_types: &DeclaredTypes,
    ) -> Result<Vec<MethodDef>, ScanError> {
        super::value_methods(
            &parse(source),
            parent,
            &ModuleScope::root("demo"),
            declared_types,
        )
    }

    fn scan_class(
        source: &str,
        parent: &str,
        declared_types: &DeclaredTypes,
    ) -> Result<Vec<MethodDef>, ScanError> {
        super::class_methods(
            &parse(source),
            parent,
            &ModuleScope::root("demo"),
            declared_types,
        )
    }

    #[test]
    fn scans_borrowing_method_with_resolved_param_and_return() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(boltffi_ast::RecordId::new("demo::Point"));
        let methods = scan(
            "impl Point { pub fn distance(&self, other: Point) -> f64 { 0.0 } }",
            "demo::Point",
            &declared_types,
        )
        .expect("scan");

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].id, MethodId::new("demo::Point::distance"));
        assert_eq!(methods[0].name.canonical(), &name(&["distance"]));
        assert_eq!(methods[0].receiver, Receiver::Shared);
        assert_eq!(
            methods[0].parameters[0].type_expr,
            TypeExpr::record(RecordId::new("demo::Point"), Path::single("Point"))
        );
        assert_eq!(
            methods[0].returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64))
        );
    }

    #[test]
    fn associated_function_returning_self_has_no_receiver() {
        let methods = scan(
            "impl Point { pub fn origin() -> Self { todo!() } }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods[0].receiver, Receiver::None);
        assert_eq!(methods[0].returns, ReturnDef::value(TypeExpr::SelfType));
    }

    #[test]
    fn captures_each_receiver_shape() {
        let methods = scan(
            "impl Point { \
                pub fn shared(&self) {} \
                pub fn exclusive(&mut self) {} \
                pub fn consuming(self) {} \
            }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods[0].receiver, Receiver::Shared);
        assert_eq!(methods[1].receiver, Receiver::Mutable);
        assert_eq!(methods[2].receiver, Receiver::Owned);
    }

    #[test]
    fn skips_private_and_explicitly_skipped_methods() {
        let methods = scan(
            "impl Point { \
                pub fn exported(&self) {} \
                fn helper(&self) {} \
                #[skip] pub fn skipped(&self) {} \
                #[boltffi::skip] pub fn also_skipped(&self) {} \
            }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name.canonical(), &name(&["exported"]));
    }

    #[test]
    fn class_method_scan_skips_stream_methods() {
        let methods = scan_class(
            "impl Engine { \
                #[ffi_stream(item = i32)] \
                pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() } \
                pub fn version(&self) -> u32 { 1 } \
            }",
            "demo::Engine",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].id, MethodId::new("demo::Engine::version"));
    }

    #[test]
    fn class_method_scan_rejects_private_stream_methods() {
        let error = scan_class(
            "impl Engine { \
                #[ffi_stream(item = i32)] \
                fn values(&self) -> Arc<EventSubscription<i32>> { todo!() } \
            }",
            "demo::Engine",
            &DeclaredTypes::new(),
        )
        .expect_err("private stream methods are invalid");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "ffi_stream".to_owned(),
                item: "stream method".to_owned()
            }
        );
    }

    #[test]
    fn value_method_scan_rejects_stream_methods() {
        let error = scan(
            "impl Point { \
                #[ffi_stream(item = i32)] \
                pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() } \
            }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("value methods cannot be streams");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "ffi_stream".to_owned(),
                item: "method".to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_marker_on_non_method_items() {
        let error = scan_class(
            "impl Engine { #[ffi_stream(item = i32)] pub const VALUES: u32 = 0; }",
            "demo::Engine",
            &DeclaredTypes::new(),
        )
        .expect_err("stream marker belongs to methods");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "ffi_stream".to_owned(),
                item: "associated const".to_owned()
            }
        );
    }

    #[test]
    fn rejects_malformed_skip_marker_on_method() {
        let error = scan(
            "impl Point { #[skip(reason)] pub fn hidden(&self) {} }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("malformed skip marker must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarker {
                attribute: "skip(reason)".to_owned()
            }
        );
    }

    #[test]
    fn rejects_non_skip_boltffi_markers_on_methods() {
        let error = scan(
            "impl Point { #[export] pub fn hidden(&self) {} }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("method marker rejected");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "method".to_owned(),
            }
        );
    }

    #[test]
    fn rejects_public_non_method_items_before_dropping_them() {
        let associated_const = scan(
            "impl Point { pub const VERSION: u32 = 1; }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("associated const rejected");
        let associated_type = scan(
            "impl Point { pub type Value = u32; }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("associated type rejected");

        assert_eq!(
            associated_const,
            ScanError::UnsupportedImplItem {
                item: "demo::Point::VERSION".to_owned(),
            }
        );
        assert_eq!(
            associated_type,
            ScanError::UnsupportedImplItem {
                item: "demo::Point::Value".to_owned(),
            }
        );
    }

    #[test]
    fn ignores_private_and_skipped_non_method_items() {
        let methods = scan(
            "impl Point { \
                const PRIVATE: u32 = 1; \
                #[skip] pub const PUBLIC: u32 = 2; \
                pub fn exported(&self) {} \
            }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name.canonical(), &name(&["exported"]));
    }

    #[test]
    fn rejects_impl_macros_because_their_exports_are_not_syntactic() {
        let error = scan(
            "impl Point { exported_methods!(); }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("macro rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedImplItem {
                item: "demo::Point::exported_methods".to_owned(),
            }
        );
    }
}
