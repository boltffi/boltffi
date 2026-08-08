use boltffi_ast::{
    AttributeInput, ConstExpr, DefaultValue, DeprecationInfo, DocComment, Literal, Path, PathRoot,
    PathSegment, Source, UserAttr, Visibility,
};

use crate::const_expr;
use crate::type_expr;
use crate::{ModuleScope, ScanError, spelling, visibility};

pub(super) struct Attributes<'a, 'types> {
    attrs: &'a [syn::Attribute],
    constants: const_expr::Scanner<'a, 'types>,
}

impl<'a, 'types> Attributes<'a, 'types> {
    pub(super) fn new(attrs: &'a [syn::Attribute], types: &'a type_expr::Scanner<'types>) -> Self {
        Self {
            attrs,
            constants: const_expr::Scanner::new(types),
        }
    }

    pub(super) fn doc(&self) -> Option<DocComment> {
        let text = self
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("doc"))
            .filter_map(|attr| match &attr.meta {
                syn::Meta::NameValue(value) => string_literal(&value.value),
                _ => None,
            })
            .map(|line| line.strip_prefix(' ').unwrap_or(&line).to_owned())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();

        (!text.is_empty()).then(|| DocComment::new(text))
    }

    pub(super) fn deprecated(&self) -> Result<Option<DeprecationInfo>, ScanError> {
        self.attrs
            .iter()
            .find(|attr| attr.path().is_ident("deprecated"))
            .map(|attr| self.deprecation(attr))
            .transpose()
    }

    pub(super) fn default(&self) -> Result<Option<DefaultValue>, ScanError> {
        self.attrs
            .iter()
            .filter(|attr| is_default_attr(attr))
            .try_fold(None, |found, attr| {
                if found.is_some() {
                    return Err(ScanError::InvalidDefault {
                        attribute: spelling::attr(attr),
                    });
                }
                self.default_attr(attr).map(Some)
            })
    }

    pub(super) fn user_attrs(&self) -> Vec<UserAttr> {
        self.attrs
            .iter()
            .filter(|attr| keep_user_attr(attr))
            .map(|attr| UserAttr::new(ast_path(attr.path()), self.input(attr)))
            .collect()
    }

    fn deprecation(&self, attr: &syn::Attribute) -> Result<DeprecationInfo, ScanError> {
        match &attr.meta {
            syn::Meta::Path(_) => Ok(DeprecationInfo::new(None, None)),
            syn::Meta::NameValue(value) => string_literal(&value.value)
                .map(|note| DeprecationInfo::new(Some(note), None))
                .ok_or_else(|| invalid_attribute(attr)),
            syn::Meta::List(_) => {
                let mut parts = DeprecationParts::default();
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("note") {
                        parts.note = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                        return Ok(());
                    }
                    if meta.path.is_ident("since") {
                        parts.since = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                        return Ok(());
                    }
                    Err(meta.error("unsupported deprecated attribute key"))
                })
                .map_err(|e| ScanError::InvalidAttribute {
                    attribute: format!("{}: {e}", spelling::attr(attr)),
                })?;
                Ok(DeprecationInfo::new(parts.note, parts.since))
            }
        }
    }

    fn default_attr(&self, attr: &syn::Attribute) -> Result<DefaultValue, ScanError> {
        let expr = match &attr.meta {
            syn::Meta::List(list) => syn::parse2::<syn::Expr>(list.tokens.clone()),
            syn::Meta::NameValue(value) => Ok(value.value.clone()),
            syn::Meta::Path(_) => Err(syn::Error::new_spanned(attr, "missing default value")),
        }
        .map_err(|_| ScanError::InvalidDefault {
            attribute: spelling::attr(attr),
        })?;
        self.default_expr(&expr)
            .ok_or_else(|| ScanError::InvalidDefault {
                attribute: spelling::attr(attr),
            })
    }

    fn default_expr(&self, expr: &syn::Expr) -> Option<DefaultValue> {
        match self.constants.scan(expr) {
            ConstExpr::Literal(Literal::Bool(value)) => Some(DefaultValue::Bool(value)),
            ConstExpr::Literal(Literal::Integer(value)) => Some(DefaultValue::Integer(value)),
            ConstExpr::Literal(Literal::Float(value)) => Some(DefaultValue::Float(value)),
            ConstExpr::Literal(Literal::String(value)) => Some(DefaultValue::String(value)),
            ConstExpr::Literal(Literal::Bytes(value)) => Some(DefaultValue::Bytes(value)),
            ConstExpr::Path(path) if is_none_path(&path) => Some(DefaultValue::None),
            ConstExpr::Path(path) => Some(DefaultValue::Path(path)),
            ConstExpr::Array(_) | ConstExpr::Tuple(_) | ConstExpr::Raw(_) => None,
        }
    }

    fn input(&self, attr: &syn::Attribute) -> AttributeInput {
        match &attr.meta {
            syn::Meta::Path(_) => AttributeInput::Empty,
            syn::Meta::NameValue(value) => AttributeInput::Value(self.constants.scan(&value.value)),
            syn::Meta::List(list) => AttributeInput::Tokens(list.tokens.to_string()),
        }
    }
}

pub(super) fn source(
    visibility: &syn::Visibility,
    scope: &ModuleScope,
    span: proc_macro2::Span,
) -> Source {
    Source::new(visibility::kind(visibility), scope.source_span(span))
}

pub(super) fn public_source(scope: &ModuleScope, span: proc_macro2::Span) -> Source {
    Source::new(Visibility::Public, scope.source_span(span))
}

#[derive(Default)]
struct DeprecationParts {
    note: Option<String>,
    since: Option<String>,
}

fn string_literal(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(literal),
            ..
        }) => Some(literal.value()),
        _ => None,
    }
}

fn keep_user_attr(attr: &syn::Attribute) -> bool {
    !is_doc_comment_attr(attr)
        && !attr.path().is_ident("deprecated")
        && !attr.path().is_ident("repr")
        && !is_boltffi_owned_attr(attr)
}

fn is_doc_comment_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("doc") && matches!(attr.meta, syn::Meta::NameValue(_))
}

fn is_boltffi_owned_attr(attr: &syn::Attribute) -> bool {
    matches!(
        attr_name(attr).as_deref(),
        Some(
            "data"
                | "error"
                | "export"
                | "skip"
                | "default"
                | "ffi_stream"
                | "name"
                | "custom_ffi"
                | "custom_type",
        )
    )
}

fn is_default_attr(attr: &syn::Attribute) -> bool {
    matches!(attr_name(attr).as_deref(), Some("default"))
}

fn attr_name(attr: &syn::Attribute) -> Option<String> {
    let segments = attr.path().segments.iter().collect::<Vec<_>>();
    match segments.as_slice() {
        [segment] => Some(segment.ident.to_string()),
        [namespace, marker] if namespace.ident == "boltffi" => Some(marker.ident.to_string()),
        _ => None,
    }
}

fn ast_path(path: &syn::Path) -> Path {
    let (root, segments) = ast_path_parts(path);
    Path::new(root, segments)
}

fn ast_path_parts(path: &syn::Path) -> (PathRoot, Vec<PathSegment>) {
    let segments = path.segments.iter().collect::<Vec<_>>();
    if path.leading_colon.is_some() {
        return (PathRoot::Absolute, path_segments(segments.into_iter()));
    }
    match segments.as_slice() {
        [first, rest @ ..] if first.ident == "crate" => {
            (PathRoot::Crate, path_segments(rest.iter().copied()))
        }
        [first, rest @ ..] if first.ident == "self" => {
            (PathRoot::Self_, path_segments(rest.iter().copied()))
        }
        [first, ..] if first.ident == "super" => {
            let levels = segments
                .iter()
                .take_while(|segment| segment.ident == "super")
                .count();
            (
                PathRoot::Super(
                    std::num::NonZeroUsize::new(levels)
                        .expect("super path has at least one parent segment"),
                ),
                path_segments(segments.into_iter().skip(levels)),
            )
        }
        _ => (PathRoot::Relative, path_segments(segments.into_iter())),
    }
}

fn path_segments<'a>(segments: impl Iterator<Item = &'a syn::PathSegment>) -> Vec<PathSegment> {
    segments
        .map(|segment| PathSegment::new(segment.ident.to_string()))
        .collect()
}

fn is_none_path(path: &Path) -> bool {
    matches!(path.root, PathRoot::Relative)
        && path.segments.len() == 1
        && path.segments[0].name.as_str() == "None"
        && path.segments[0].arguments.is_empty()
}

fn invalid_attribute(attr: &syn::Attribute) -> ScanError {
    ScanError::InvalidAttribute {
        attribute: spelling::attr(attr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleScope;
    use crate::declared_types::DeclaredTypes;
    use boltffi_ast::{FloatLiteral, IntegerLiteral};

    fn attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemStruct>(source)
            .expect("struct")
            .attrs
    }

    fn with_attrs<T>(attrs: &[syn::Attribute], scan: impl FnOnce(Attributes<'_, '_>) -> T) -> T {
        let declared = DeclaredTypes::new();
        let scope = ModuleScope::root("demo");
        let scanner = type_expr::Scanner::new(&declared, &scope);
        scan(Attributes::new(attrs, &scanner))
    }

    #[test]
    fn scans_rustdoc_lines_into_one_comment() {
        let attrs = attrs("/// First\n///\n/// Second\nstruct Point;");

        assert_eq!(
            with_attrs(&attrs, |scanned| scanned
                .doc()
                .map(|doc| doc.as_str().to_owned())),
            Some("First\n\nSecond".to_owned())
        );
    }

    #[test]
    fn scans_deprecated_attribute_forms() {
        let plain = attrs("#[deprecated]\nstruct Point;");
        let note = attrs("#[deprecated = \"use Shape\"]\nstruct Point;");
        let full = attrs("#[deprecated(since = \"1.2\", note = \"use Shape\")]\nstruct Point;");

        assert_eq!(
            with_attrs(&plain, |scanned| scanned.deprecated()),
            Ok(Some(DeprecationInfo::new(None, None)))
        );
        assert_eq!(
            with_attrs(&note, |scanned| scanned.deprecated()),
            Ok(Some(DeprecationInfo::new(
                Some("use Shape".to_owned()),
                None
            )))
        );
        assert_eq!(
            with_attrs(&full, |scanned| scanned.deprecated()),
            Ok(Some(DeprecationInfo::new(
                Some("use Shape".to_owned()),
                Some("1.2".to_owned())
            )))
        );
    }

    #[test]
    fn scans_default_value_families() {
        let integer = attrs("#[default(-7)] struct Point;");
        let float = attrs("#[boltffi::default(1.5)] struct Point;");
        let string = attrs("#[default = \"demo\"] struct Point;");
        let none = attrs("#[default(None)] struct Point;");
        let path = attrs("#[default(Mode::Fast)] struct Point;");

        assert_eq!(
            with_attrs(&integer, |scanned| scanned.default()),
            Ok(Some(DefaultValue::Integer(IntegerLiteral::new(-7, "-7"))))
        );
        assert_eq!(
            with_attrs(&float, |scanned| scanned.default()),
            Ok(Some(DefaultValue::Float(FloatLiteral::new("1.5"))))
        );
        assert_eq!(
            with_attrs(&string, |scanned| scanned.default()),
            Ok(Some(DefaultValue::String("demo".to_owned())))
        );
        assert_eq!(
            with_attrs(&none, |scanned| scanned.default()),
            Ok(Some(DefaultValue::None))
        );
        assert_eq!(
            with_attrs(&path, |scanned| scanned.default()),
            Ok(Some(DefaultValue::Path(Path::new(
                PathRoot::Relative,
                vec![PathSegment::new("Mode"), PathSegment::new("Fast")]
            ))))
        );
    }

    #[test]
    fn preserves_unowned_attributes_without_scanner_owned_metadata() {
        let attrs = attrs(
            "#[derive(Clone)] #[serde(rename = \"point\")] #[doc(hidden)] #[repr(C)] #[deprecated] #[default(1)] struct Point;",
        );
        let scanned = with_attrs(&attrs, |scanned| scanned.user_attrs());

        assert_eq!(scanned.len(), 3);
        assert_eq!(
            scanned[0].path,
            Path::new(PathRoot::Relative, vec![PathSegment::new("derive")])
        );
        assert_eq!(scanned[0].input, AttributeInput::Tokens("Clone".to_owned()));
        assert_eq!(
            scanned[1].path,
            Path::new(PathRoot::Relative, vec![PathSegment::new("serde")])
        );
        assert_eq!(
            scanned[1].input,
            AttributeInput::Tokens("rename = \"point\"".to_owned())
        );
        assert_eq!(scanned[2].path, Path::single("doc"));
        assert_eq!(
            scanned[2].input,
            AttributeInput::Tokens("hidden".to_owned())
        );
    }

    #[test]
    fn path_roots_are_preserved_in_user_attributes() {
        let attrs = attrs("#[crate::ffi::tag] #[::serde::transparent] struct Point;");
        let scanned = with_attrs(&attrs, |scanned| scanned.user_attrs());

        assert_eq!(
            scanned[0].path,
            Path::new(
                PathRoot::Crate,
                vec![PathSegment::new("ffi"), PathSegment::new("tag")]
            )
        );
        assert_eq!(
            scanned[1].path,
            Path::new(
                PathRoot::Absolute,
                vec![PathSegment::new("serde"), PathSegment::new("transparent")]
            )
        );
    }

    #[test]
    fn deprecated_with_unknown_key_includes_reason_in_error() {
        let attrs = attrs("#[deprecated(reason = \"typo\")] struct Point;");
        let err = with_attrs(&attrs, |scanned| scanned.deprecated()).unwrap_err();
        let ScanError::InvalidAttribute { attribute } = err else {
            panic!("expected InvalidAttribute, got {err:?}");
        };
        // The error must include the attribute spelling AND the syn parse error so
        // the author knows which key was wrong, not just that parsing failed.
        assert!(attribute.contains("deprecated"), "must name the attribute");
        assert!(
            attribute.contains("unsupported deprecated attribute key"),
            "must include the syn error reason; got: `{attribute}`"
        );
    }

    #[test]
    fn default_rejects_unrepresentable_expressions() {
        let attrs = attrs("#[default([1, 2])] struct Point;");

        assert_eq!(
            with_attrs(&attrs, |scanned| scanned.default()),
            Err(ScanError::InvalidDefault {
                attribute: "default([1 , 2])".to_owned()
            })
        );
    }
}
