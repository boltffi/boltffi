use boltffi_ast::{ClassId, StreamDef, StreamId, StreamMode, TypeExpr};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::path::PathExpansion;
use crate::type_expr::Scanner;
use crate::{ModuleScope, ScanError, attributes, name, spelling, unsupported};

use super::{class, impl_methods};

pub fn scan(
    marked: &[Marked<'_, syn::ItemImpl>],
    declared_types: &DeclaredTypes,
) -> Result<Vec<StreamDef>, ScanError> {
    marked.iter().try_fold(Vec::new(), |mut streams, marked| {
        let mut block = block_streams(marked, declared_types)?;
        streams.append(&mut block);
        Ok(streams)
    })
}

pub(super) struct Attribute {
    item: syn::Type,
    mode: StreamMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StreamItem {
    boundary: TypeExpr,
    storage: TypeExpr,
}

impl Attribute {
    pub(super) fn scan(attrs: &[syn::Attribute]) -> Result<Option<Self>, ScanError> {
        attrs
            .iter()
            .filter(|attr| Self::matches(attr))
            .try_fold(None, |found, attr| {
                if found.is_some() {
                    return Err(Self::invalid("duplicate ffi_stream attribute"));
                }
                Ok(Some(Self::parse(attr)?))
            })
    }

    pub(super) fn invalid_placement(item: impl Into<String>) -> ScanError {
        ScanError::InvalidMarkerPlacement {
            marker: "ffi_stream".to_owned(),
            item: item.into(),
        }
    }

    pub(super) fn item(&self) -> &syn::Type {
        &self.item
    }

    pub(super) const fn mode(&self) -> StreamMode {
        self.mode
    }

    fn parse(attr: &syn::Attribute) -> Result<Self, ScanError> {
        let mut item = None;
        let mut mode = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("item") {
                if item.is_some() {
                    return Err(meta.error("duplicate stream item"));
                }
                item = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("mode") {
                if mode.is_some() {
                    return Err(meta.error("duplicate stream mode"));
                }
                let literal = meta.value()?.parse::<syn::LitStr>()?;
                mode = Some(Self::mode_from_literal(&literal, &meta)?);
                return Ok(());
            }
            Err(meta.error("unsupported stream attribute key"))
        })
        .map_err(|error| {
            Self::invalid(format!(
                "`{}` is not a valid stream attribute: {error}",
                spelling::attr(attr)
            ))
        })?;
        Ok(Self {
            item: item.ok_or_else(|| Self::invalid("ffi_stream requires item = <type>"))?,
            mode: mode.unwrap_or(StreamMode::Async),
        })
    }

    pub(super) fn matches(attr: &syn::Attribute) -> bool {
        let segments = attr.path().segments.iter().collect::<Vec<_>>();
        match segments.as_slice() {
            [segment] => segment.ident == "ffi_stream",
            [namespace, marker] => namespace.ident == "boltffi" && marker.ident == "ffi_stream",
            _ => false,
        }
    }

    fn mode_from_literal(
        literal: &syn::LitStr,
        meta: &syn::meta::ParseNestedMeta<'_>,
    ) -> syn::Result<StreamMode> {
        match literal.value().as_str() {
            "async" => Ok(StreamMode::Async),
            "batch" => Ok(StreamMode::Batch),
            "callback" => Ok(StreamMode::Callback),
            _ => Err(meta.error("unsupported stream mode")),
        }
    }

    fn invalid(message: impl Into<String>) -> ScanError {
        ScanError::InvalidStream {
            message: message.into(),
        }
    }
}

fn block_streams(
    marked: &Marked<'_, syn::ItemImpl>,
    declared_types: &DeclaredTypes,
) -> Result<Vec<StreamDef>, ScanError> {
    scan_item(marked.item(), marked.scope(), declared_types)
}

pub(crate) fn scan_item(
    item: &syn::ItemImpl,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<Vec<StreamDef>, ScanError> {
    let owner = class::resolve_id(item, scope, declared_types)?;
    let scanner = Scanner::new(declared_types, scope);
    item.items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(method) => method_stream(method, &owner, scope, &scanner).transpose(),
            _ => None,
        })
        .collect()
}

fn method_stream(
    method: &syn::ImplItemFn,
    owner: &ClassId,
    scope: &ModuleScope,
    scanner: &Scanner<'_>,
) -> Result<Option<StreamDef>, ScanError> {
    let Some(attribute) = Attribute::scan(&method.attrs)? else {
        return Ok(None);
    };
    if !impl_methods::exported_method(&method.attrs, &method.vis, "stream method")? {
        return Err(Attribute::invalid_placement("stream method"));
    }
    let item_type = validate(method, owner.as_str(), scope, scanner, &attribute)?;
    let stream_name = method.sig.ident.to_string();
    let mut stream = StreamDef::new(
        StreamId::new(format!("{}::{stream_name}", owner.as_str())),
        name::source(&method.sig.ident),
        item_type.into_boundary(),
    );
    let attrs = Attributes::new(&method.attrs, scanner);
    stream.owner = Some(owner.clone());
    stream.mode = attribute.mode();
    stream.source = attributes::source(&method.vis, scope, method.span());
    stream.source_span = stream.source.span.clone();
    stream.doc = attrs.doc();
    stream.deprecated = attrs.deprecated()?;
    stream.user_attrs = attrs.user_attrs();
    Ok(Some(stream))
}

fn validate(
    method: &syn::ImplItemFn,
    owner: &str,
    scope: &ModuleScope,
    scanner: &Scanner<'_>,
    attribute: &Attribute,
) -> Result<StreamItem, ScanError> {
    let item = format!("stream {owner}::{}", method.sig.ident);
    unsupported::generics(&method.sig.generics, &item)?;
    unsupported::unsafety(method.sig.unsafety.as_ref(), &item)?;
    unsupported::extern_abi(method.sig.abi.as_ref(), &item)?;
    if method.sig.asyncness.is_some() {
        return Err(Attribute::invalid(format!("`{item}` cannot be async")));
    }
    if !has_shared_receiver_only(method) {
        return Err(Attribute::invalid(format!(
            "`{item}` must be a public `&self` method with no parameters"
        )));
    }
    let returned = subscription_item(&method.sig.output, scope, &item, scanner.is_deferred())?;
    let declared_item = StreamItem::from_boundary(scanner.scan(attribute.item())?);
    let returned_item = scanner.scan(returned)?;
    if declared_item.storage() != &returned_item {
        return Err(Attribute::invalid(format!(
            "`{item}` declares item `{}` but returns `{}`",
            spelling::ty(attribute.item()),
            spelling::ty(returned)
        )));
    }
    Ok(declared_item)
}

impl StreamItem {
    fn from_boundary(boundary: TypeExpr) -> Self {
        let storage = Self::storage_type(&boundary);
        Self { boundary, storage }
    }

    fn storage(&self) -> &TypeExpr {
        &self.storage
    }

    fn into_boundary(self) -> TypeExpr {
        self.boundary
    }

    fn storage_type(boundary: &TypeExpr) -> TypeExpr {
        match boundary {
            TypeExpr::Str => TypeExpr::String,
            TypeExpr::Slice(element) | TypeExpr::Vec(element) => {
                TypeExpr::vec(Self::storage_type(element))
            }
            TypeExpr::Option(inner) => TypeExpr::option(Self::storage_type(inner)),
            TypeExpr::Result { ok, err } => {
                TypeExpr::result(Self::storage_type(ok), Self::storage_type(err))
            }
            TypeExpr::Tuple(elements) => {
                TypeExpr::tuple(elements.iter().map(Self::storage_type).collect())
            }
            TypeExpr::Map { kind, key, value } => {
                TypeExpr::map(*kind, Self::storage_type(key), Self::storage_type(value))
            }
            _ => boundary.clone(),
        }
    }
}

fn has_shared_receiver_only(method: &syn::ImplItemFn) -> bool {
    method.sig.inputs.len() == 1
        && matches!(
            method.sig.inputs.first(),
            Some(syn::FnArg::Receiver(receiver))
                if receiver.reference.is_some() && receiver.mutability.is_none()
        )
}

fn subscription_item<'source>(
    output: &'source syn::ReturnType,
    scope: &ModuleScope,
    item: &str,
    deferred: bool,
) -> Result<&'source syn::Type, ScanError> {
    let syn::ReturnType::Type(_, ty) = output else {
        return Err(Attribute::invalid(format!(
            "`{item}` must return Arc<EventSubscription<T>>"
        )));
    };
    let outer = path_type(ty).ok_or_else(|| {
        Attribute::invalid(format!("`{item}` must return Arc<EventSubscription<T>>"))
    })?;
    if !matches_path(
        scope,
        &outer.path,
        &["std::sync::Arc", "alloc::sync::Arc"],
        deferred,
    ) {
        return Err(Attribute::invalid(format!(
            "`{item}` must return Arc<EventSubscription<T>>"
        )));
    }
    let subscription = generic_type_argument(outer)
        .and_then(path_type)
        .ok_or_else(|| {
            Attribute::invalid(format!("`{item}` must return Arc<EventSubscription<T>>"))
        })?;
    if !matches_path(
        scope,
        &subscription.path,
        &[
            "boltffi::EventSubscription",
            "boltffi_core::EventSubscription",
        ],
        deferred,
    ) {
        return Err(Attribute::invalid(format!(
            "`{item}` must return Arc<EventSubscription<T>>"
        )));
    }
    generic_type_argument(subscription).ok_or_else(|| {
        Attribute::invalid(format!("`{item}` must return Arc<EventSubscription<T>>"))
    })
}

fn path_type(ty: &syn::Type) -> Option<&syn::TypePath> {
    match ty {
        syn::Type::Paren(paren) => path_type(&paren.elem),
        syn::Type::Group(group) => path_type(&group.elem),
        syn::Type::Path(path) if path.qself.is_none() => Some(path),
        _ => None,
    }
}

fn generic_type_argument(path: &syn::TypePath) -> Option<&syn::Type> {
    let segment = path.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.iter().collect::<Vec<_>>().as_slice() {
        [syn::GenericArgument::Type(ty)] => Some(ty),
        _ => None,
    }
}

fn matches_path(scope: &ModuleScope, path: &syn::Path, qualified: &[&str], deferred: bool) -> bool {
    if deferred {
        return suffix_matches(path, qualified);
    }
    match scope.expand(path) {
        PathExpansion::Imported { path, .. } | PathExpansion::Qualified(path) => {
            qualified.iter().any(|candidate| path == *candidate)
        }
        PathExpansion::Relative(_) => raw_path(path).is_some_and(|path| {
            qualified.iter().any(|candidate| path == *candidate)
                || scope
                    .glob_candidates_for_segments(&[path])
                    .iter()
                    .any(|path| qualified.iter().any(|candidate| path == candidate))
        }),
        PathExpansion::Ambiguous | PathExpansion::Unsupported => false,
    }
}

fn suffix_matches(path: &syn::Path, qualified: &[&str]) -> bool {
    let written = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    qualified.iter().any(|candidate| {
        let segments = candidate.split("::").collect::<Vec<_>>();
        written.len() <= segments.len()
            && written
                .iter()
                .rev()
                .zip(segments.iter().rev())
                .all(|(written, segment)| written == segment)
    })
}

fn raw_path(path: &syn::Path) -> Option<String> {
    path.leading_colon.is_none().then(|| {
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleScope;
    use boltffi_ast::{Path, Primitive, RecordId, TypeExpr};

    fn attr(source: &str) -> syn::Attribute {
        syn::parse_str::<syn::ImplItemFn>(source)
            .expect("valid method")
            .attrs
            .into_iter()
            .find(Attribute::matches)
            .expect("stream attribute")
    }

    fn method(source: &str) -> syn::ImplItemFn {
        syn::parse_str(source).expect("valid method")
    }

    fn validate_method(method: &syn::ImplItemFn, owner: &str) -> Result<(), ScanError> {
        let declared_types = DeclaredTypes::new();
        let scope = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &scope);
        let attribute = Attribute::scan(&method.attrs)?.expect("stream attribute");
        validate(method, owner, &scope, &scanner, &attribute).map(|_| ())
    }

    #[test]
    fn scans_item_type_and_default_mode() {
        let attribute =
            Attribute::parse(&attr("#[ffi_stream(item = i32)] pub fn events(&self) {}"))
                .expect("attribute");

        assert!(matches!(attribute.item(), syn::Type::Path(_)));
        assert_eq!(attribute.mode(), StreamMode::Async);
    }

    #[test]
    fn scans_each_stream_mode() {
        let batch = Attribute::parse(&attr(
            r#"#[ffi_stream(item = i32, mode = "batch")] pub fn events(&self) {}"#,
        ))
        .expect("batch attribute");
        let callback = Attribute::parse(&attr(
            r#"#[ffi_stream(item = i32, mode = "callback")] pub fn events(&self) {}"#,
        ))
        .expect("callback attribute");
        let async_mode = Attribute::parse(&attr(
            r#"#[ffi_stream(item = i32, mode = "async")] pub fn events(&self) {}"#,
        ))
        .expect("async attribute");

        assert_eq!(batch.mode(), StreamMode::Batch);
        assert_eq!(callback.mode(), StreamMode::Callback);
        assert_eq!(async_mode.mode(), StreamMode::Async);
    }

    #[test]
    fn rejects_stream_attribute_without_item_type() {
        let error = match Attribute::parse(&attr(
            r#"#[ffi_stream(mode = "batch")] pub fn events(&self) {}"#,
        )) {
            Ok(_) => panic!("item is required"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message: "ffi_stream requires item = <type>".to_owned()
            }
        );
    }

    #[test]
    fn rejects_unknown_stream_mode() {
        let error = match Attribute::parse(&attr(
            r#"#[ffi_stream(item = i32, mode = "fast")] pub fn events(&self) {}"#,
        )) {
            Ok(_) => panic!("mode is checked"),
            Err(error) => error,
        };

        assert!(matches!(error, ScanError::InvalidStream { .. }));
    }

    #[test]
    fn rejects_static_stream_methods() {
        let method = method(
            "#[ffi_stream(item = i32)] pub fn events() -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }",
        );

        let error = validate_method(&method, "demo::Engine").expect_err("receiver is required");

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message:
                    "`stream demo::Engine::events` must be a public `&self` method with no parameters"
                        .to_owned()
            }
        );
    }

    #[test]
    fn rejects_parameterized_stream_methods() {
        let method = method(
            "#[ffi_stream(item = i32)] pub fn events(&self, limit: u32) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }",
        );

        let error = validate_method(&method, "demo::Engine").expect_err("parameters are rejected");

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message:
                    "`stream demo::Engine::events` must be a public `&self` method with no parameters"
                        .to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_methods_without_subscription_return() {
        let method = method("#[ffi_stream(item = i32)] pub fn events(&self) -> i32 { todo!() }");

        let error = validate_method(&method, "demo::Engine").expect_err("return shape rejected");

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message: "`stream demo::Engine::events` must return Arc<EventSubscription<T>>"
                    .to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_methods_whose_item_differs_from_return_item() {
        let method = method(
            "#[ffi_stream(item = i32)] pub fn events(&self) -> std::sync::Arc<boltffi::EventSubscription<u32>> { todo!() }",
        );

        let error = validate_method(&method, "demo::Engine").expect_err("item mismatch rejected");

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message: "`stream demo::Engine::events` declares item `i32` but returns `u32`"
                    .to_owned()
            }
        );
    }

    #[test]
    fn scans_class_owned_streams() {
        let source = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "use std::sync::Arc; \
                 use boltffi::EventSubscription; \
                 pub struct Engine; \
                 #[data] pub struct Point { pub x: f64 } \
                 #[export] impl Engine { \
                    #[ffi_stream(item = Point, mode = \"batch\")] \
                    pub fn points(&self) -> Arc<EventSubscription<Point>> { todo!() } \
                 }",
            )
            .expect("file")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source).expect("marked items");
        let declared_types = DeclaredTypes::index(&source, &marked).expect("declared types");
        let streams = scan(marked.classes(), &declared_types).expect("streams");

        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].id, StreamId::new("demo::Engine::points"));
        assert_eq!(streams[0].owner, Some(ClassId::new("demo::Engine")));
        assert_eq!(streams[0].mode, StreamMode::Batch);
        assert_eq!(
            streams[0].item_type,
            TypeExpr::record(RecordId::new("demo::Point"), Path::single("Point"))
        );
    }

    #[test]
    fn scans_class_owned_streams_with_qualified_runtime_types() {
        let source = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "pub struct Engine; \
                 #[export] impl Engine { \
                    #[ffi_stream(item = i32)] \
                    pub fn values(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() } \
                 }",
            )
            .expect("file")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source).expect("marked items");
        let declared_types = DeclaredTypes::index(&source, &marked).expect("declared types");
        let streams = scan(marked.classes(), &declared_types).expect("streams");

        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].item_type, TypeExpr::Primitive(Primitive::I32));
    }

    #[test]
    fn scans_str_stream_items_with_string_storage() {
        let source = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "use std::sync::Arc; \
                 use boltffi::EventSubscription; \
                 pub struct Engine; \
                 #[export] impl Engine { \
                    #[ffi_stream(item = str)] \
                    pub fn lines(&self) -> Arc<EventSubscription<String>> { todo!() } \
                 }",
            )
            .expect("file")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source).expect("marked items");
        let declared_types = DeclaredTypes::index(&source, &marked).expect("declared types");
        let streams = scan(marked.classes(), &declared_types).expect("streams");

        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].item_type, TypeExpr::Str);
    }

    #[test]
    fn scans_slice_stream_items_with_vec_storage() {
        let source = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "use std::sync::Arc; \
                 use boltffi::EventSubscription; \
                 pub struct Engine; \
                 #[export] impl Engine { \
                    #[ffi_stream(item = [u8])] \
                    pub fn chunks(&self) -> Arc<EventSubscription<Vec<u8>>> { todo!() } \
                 }",
            )
            .expect("file")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source).expect("marked items");
        let declared_types = DeclaredTypes::index(&source, &marked).expect("declared types");
        let streams = scan(marked.classes(), &declared_types).expect("streams");

        assert_eq!(streams.len(), 1);
        assert_eq!(
            streams[0].item_type,
            TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))
        );
    }

    #[test]
    fn rejects_stream_return_when_arc_resolves_to_local_type() {
        let source = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "pub struct Arc<T>(T); \
                 pub struct Engine; \
                 #[export] impl Engine { \
                    #[ffi_stream(item = i32)] \
                    pub fn values(&self) -> Arc<boltffi::EventSubscription<i32>> { todo!() } \
                 }",
            )
            .expect("file")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source).expect("marked items");
        let declared_types = DeclaredTypes::index(&source, &marked).expect("declared types");

        let error = scan(marked.classes(), &declared_types).expect_err("local Arc rejected");

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message: "`stream demo::Engine::values` must return Arc<EventSubscription<T>>"
                    .to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_return_when_subscription_resolves_to_other_type() {
        let source = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "use std::sync::Arc; \
                 use other::EventSubscription; \
                 pub struct Engine; \
                 #[export] impl Engine { \
                    #[ffi_stream(item = i32)] \
                    pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() } \
                 }",
            )
            .expect("file")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source).expect("marked items");
        let declared_types = DeclaredTypes::index(&source, &marked).expect("declared types");

        let error =
            scan(marked.classes(), &declared_types).expect_err("other subscription rejected");

        assert_eq!(
            error,
            ScanError::InvalidStream {
                message: "`stream demo::Engine::values` must return Arc<EventSubscription<T>>"
                    .to_owned()
            }
        );
    }

    #[test]
    fn rejects_private_stream_methods() {
        let method = method(
            "#[ffi_stream(item = i32)] fn events(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }",
        );
        let declared_types = DeclaredTypes::new();
        let scope = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &scope);

        let error = method_stream(&method, &ClassId::new("demo::Engine"), &scope, &scanner)
            .expect_err("private stream method rejected");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "ffi_stream".to_owned(),
                item: "stream method".to_owned()
            }
        );
    }
}
