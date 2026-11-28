use boltffi_ast::{ClassId, StreamDef, StreamId, StreamMode, TypeExpr};

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::type_expr::Scanner;
use crate::{ScanError, attributes, name, spelling, unsupported};

use super::impl_methods;

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
            .at_span(error.span())
        })?;
        Ok(Self {
            item: item
                .ok_or_else(|| Self::invalid("ffi_stream requires item = <type>").at(attr))?,
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

pub(crate) fn scan_owned(
    item: &syn::ItemImpl,
    owner: &ClassId,
    declared_types: &DeclaredTypes,
) -> Result<Vec<StreamDef>, ScanError> {
    let scanner = Scanner::new(declared_types);
    item.items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(method) => method_stream(method, owner, &scanner).transpose(),
            _ => None,
        })
        .collect()
}

fn method_stream(
    method: &syn::ImplItemFn,
    owner: &ClassId,
    scanner: &Scanner<'_>,
) -> Result<Option<StreamDef>, ScanError> {
    let Some(attribute) = Attribute::scan(&method.attrs)? else {
        return Ok(None);
    };
    if !impl_methods::exported_method(&method.attrs, &method.vis, "stream method")? {
        return Err(Attribute::invalid_placement("stream method"));
    }
    let item_type = validate(method, scanner, &attribute)?;
    let stream_name = method.sig.ident.to_string();
    let mut stream = StreamDef::new(
        StreamId::new(format!("{}::{stream_name}", owner.as_str())),
        name::source(&method.sig.ident),
        item_type.into_boundary(),
    );
    let attrs = Attributes::new(&method.attrs, scanner);
    stream.owner = Some(owner.clone());
    stream.mode = attribute.mode();
    stream.source = attributes::source(&method.vis);
    stream.doc = attrs.doc();
    stream.deprecated = attrs.deprecated()?;
    stream.user_attrs = attrs.user_attrs();
    Ok(Some(stream))
}

fn validate(
    method: &syn::ImplItemFn,
    scanner: &Scanner<'_>,
    attribute: &Attribute,
) -> Result<StreamItem, ScanError> {
    let item = format!("stream {}", method.sig.ident);
    unsupported::generics(&method.sig.generics, &item)?;
    unsupported::safety(&method.sig.safety, &item)?;
    unsupported::extern_abi(method.sig.abi.as_ref(), &item)?;
    if let Some(asyncness) = method.sig.asyncness {
        return Err(Attribute::invalid(format!("`{item}` cannot be async")).at(asyncness));
    }
    if !has_shared_receiver_only(method) {
        return Err(Attribute::invalid(format!(
            "`{item}` must be a public `&self` method with no parameters"
        ))
        .at(&method.sig.inputs)
        .at(&method.sig.ident));
    }
    let returned = subscription_item(&method.sig.output, &item)
        .map_err(|error| error.at(&method.sig.output).at(&method.sig.ident))?;
    let declared_item = StreamItem::from_boundary(scanner.scan(attribute.item())?);
    let returned_item = scanner.scan(returned)?;
    if declared_item.storage() != &returned_item {
        return Err(Attribute::invalid(format!(
            "`{item}` declares item `{}` but returns `{}`",
            spelling::ty(attribute.item()),
            spelling::ty(returned)
        ))
        .at(returned));
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
                if matches!(receiver.kind, syn::ReceiverKind::Reference(_, _, None))
        )
}

fn subscription_item<'source>(
    output: &'source syn::ReturnType,
    item: &str,
) -> Result<&'source syn::Type, ScanError> {
    let syn::ReturnType::Type(_, ty) = output else {
        return Err(Attribute::invalid(format!(
            "`{item}` must return Arc<EventSubscription<T>>"
        )));
    };
    let outer = path_type(ty).ok_or_else(|| {
        Attribute::invalid(format!("`{item}` must return Arc<EventSubscription<T>>"))
    })?;
    if !suffix_matches(&outer.path, &["std::sync::Arc", "alloc::sync::Arc"]) {
        return Err(Attribute::invalid(format!(
            "`{item}` must return Arc<EventSubscription<T>>"
        )));
    }
    let subscription = generic_type_argument(outer)
        .and_then(path_type)
        .ok_or_else(|| {
            Attribute::invalid(format!("`{item}` must return Arc<EventSubscription<T>>"))
        })?;
    if !suffix_matches(
        &subscription.path,
        &[
            "boltffi::EventSubscription",
            "boltffi_core::EventSubscription",
        ],
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

#[cfg(test)]
mod tests {
    use super::*;

    use boltffi_ast::{Primitive, TypeExpr};

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

    fn validate_method(method: &syn::ImplItemFn) -> Result<(), ScanError> {
        let declared_types = DeclaredTypes::new();
        let scanner = Scanner::new(&declared_types);
        let attribute = Attribute::scan(&method.attrs)?.expect("stream attribute");
        validate(method, &scanner, &attribute).map(|_| ())
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
            error.unlocated().clone(),
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

        assert!(matches!(error.unlocated(), ScanError::InvalidStream { .. }));
    }

    #[test]
    fn rejects_static_stream_methods() {
        let method = method(
            "#[ffi_stream(item = i32)] pub fn events() -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }",
        );

        let error = validate_method(&method).expect_err("receiver is required");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::InvalidStream {
                message: "`stream events` must be a public `&self` method with no parameters"
                    .to_owned()
            }
        );
    }

    #[test]
    fn rejects_parameterized_stream_methods() {
        let method = method(
            "#[ffi_stream(item = i32)] pub fn events(&self, limit: u32) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }",
        );

        let error = validate_method(&method).expect_err("parameters are rejected");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::InvalidStream {
                message: "`stream events` must be a public `&self` method with no parameters"
                    .to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_methods_without_subscription_return() {
        let method = method("#[ffi_stream(item = i32)] pub fn events(&self) -> i32 { todo!() }");

        let error = validate_method(&method).expect_err("return shape rejected");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::InvalidStream {
                message: "`stream events` must return Arc<EventSubscription<T>>".to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_methods_whose_item_differs_from_return_item() {
        let method = method(
            "#[ffi_stream(item = i32)] pub fn events(&self) -> std::sync::Arc<boltffi::EventSubscription<u32>> { todo!() }",
        );

        let error = validate_method(&method).expect_err("item mismatch rejected");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::InvalidStream {
                message: "`stream events` declares item `i32` but returns `u32`".to_owned()
            }
        );
    }

    fn stream_item(source: &str) -> TypeExpr {
        let declared_types = DeclaredTypes::new();
        let scanner = Scanner::new(&declared_types);
        method_stream(&method(source), &ClassId::new("demo::Engine"), &scanner)
            .expect("stream scans")
            .expect("stream method")
            .item_type
    }

    #[test]
    fn scans_str_stream_items_with_string_storage() {
        assert_eq!(
            stream_item(
                "#[ffi_stream(item = str)] pub fn lines(&self) -> Arc<EventSubscription<String>> { todo!() }"
            ),
            TypeExpr::Str
        );
    }

    #[test]
    fn scans_slice_stream_items_with_vec_storage() {
        assert_eq!(
            stream_item(
                "#[ffi_stream(item = [u8])] pub fn chunks(&self) -> Arc<EventSubscription<Vec<u8>>> { todo!() }"
            ),
            TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))
        );
    }

    #[test]
    fn rejects_private_stream_methods() {
        let method = method(
            "#[ffi_stream(item = i32)] fn events(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }",
        );
        let declared_types = DeclaredTypes::new();
        let scanner = Scanner::new(&declared_types);

        let error = method_stream(&method, &ClassId::new("demo::Engine"), &scanner)
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
