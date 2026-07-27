use boltffi_ast::{ConstantDef, ConstantId, ConstantOwner, Primitive, TypeExpr};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::const_expr;
use crate::declared_types::{DeclaredType, DeclaredTypes};
use crate::impl_target;
use crate::marked::Marked;
use crate::marker::{self, Disposition};
use crate::type_expr;
use crate::{ModuleScope, ScanError, attributes, name};

struct DeclarationSyntax<'syntax> {
    attrs: &'syntax [syn::Attribute],
    visibility: &'syntax syn::Visibility,
    ident: &'syntax syn::Ident,
    ty: &'syntax syn::Type,
    expr: &'syntax syn::Expr,
    span: proc_macro2::Span,
}

pub fn scan(
    marked: &Marked<'_, syn::ItemConst>,
    declared_types: &DeclaredTypes,
) -> Result<ConstantDef, ScanError> {
    build_item(marked.item(), marked.scope(), declared_types)
}

pub(crate) fn scan_item(
    item: &syn::ItemConst,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<ConstantDef, ScanError> {
    build_item(item, scope, declared_types)
}

fn build_item(
    item: &syn::ItemConst,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<ConstantDef, ScanError> {
    DeclarationSyntax::from(item).scan(scope, declared_types, None)
}

impl<'syntax> DeclarationSyntax<'syntax> {
    fn scan(
        self,
        scope: &ModuleScope,
        declared_types: &DeclaredTypes,
        owner: Option<ConstantOwner>,
    ) -> Result<ConstantDef, ScanError> {
        if self.ident == "_" {
            return Err(ScanError::AnonymousConstant);
        }
        let types = type_expr::Scanner::new(declared_types, scope);
        let type_expr = constant_type(&types, self.ty)?;
        let value = const_expr::Scanner::new(&types).scan(self.expr);
        let id = owner.as_ref().map_or_else(
            || scope.path().qualified(&self.ident.to_string()),
            |owner| format!("{}::{}", owner.as_str(), self.ident),
        );
        let mut constant = ConstantDef::new(
            ConstantId::new(id),
            name::source(self.ident),
            type_expr,
            value,
        );
        let scanned_attrs = Attributes::new(self.attrs, &types);
        constant.owner = owner;
        constant.source = attributes::source(self.visibility, scope, self.span);
        constant.source_span = constant.source.span.clone();
        constant.doc = scanned_attrs.doc();
        constant.deprecated = scanned_attrs.deprecated()?;
        constant.user_attrs = scanned_attrs.user_attrs();
        Ok(constant)
    }
}

impl<'syntax> From<&'syntax syn::ItemConst> for DeclarationSyntax<'syntax> {
    fn from(item: &'syntax syn::ItemConst) -> Self {
        Self {
            attrs: &item.attrs,
            visibility: &item.vis,
            ident: &item.ident,
            ty: &item.ty,
            expr: &item.expr,
            span: item.span(),
        }
    }
}

impl<'syntax> From<&'syntax syn::ImplItemConst> for DeclarationSyntax<'syntax> {
    fn from(item: &'syntax syn::ImplItemConst) -> Self {
        Self {
            attrs: &item.attrs,
            visibility: &item.vis,
            ident: &item.ident,
            ty: &item.ty,
            expr: &item.expr,
            span: item.span(),
        }
    }
}

pub fn scan_associated<'marked, 'source>(
    impls: impl IntoIterator<Item = &'marked Marked<'source, syn::ItemImpl>>,
    declared_types: &DeclaredTypes,
) -> Result<Vec<ConstantDef>, ScanError>
where
    'source: 'marked,
{
    impls
        .into_iter()
        .try_fold(Vec::new(), |mut constants, marked| {
            let Some(owner) = resolve_owner(marked.item(), marked.scope(), declared_types)? else {
                return Ok(constants);
            };
            let associated = marked
                .item()
                .items
                .iter()
                .filter_map(|item| match item {
                    syn::ImplItem::Const(constant) => scan_associated_constant(
                        constant,
                        owner.clone(),
                        marked.scope(),
                        declared_types,
                    )
                    .transpose(),
                    _ => None,
                })
                .collect::<Result<Vec<_>, ScanError>>()?;
            constants.extend(associated);
            Ok(constants)
        })
}

pub(crate) fn scan_associated_in_impl(
    item: &syn::ItemImpl,
    owner: &ConstantOwner,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<Vec<ConstantDef>, ScanError> {
    item.items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Const(constant) => {
                scan_associated_constant(constant, owner.clone(), scope, declared_types).transpose()
            }
            _ => None,
        })
        .collect()
}

fn scan_associated_constant(
    constant: &syn::ImplItemConst,
    owner: ConstantOwner,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<Option<ConstantDef>, ScanError> {
    match marker::disposition(&constant.attrs)? {
        Disposition::Skip => return Ok(None),
        Disposition::Reject(marker) => {
            return Err(marker.invalid_placement("associated const"));
        }
        Disposition::Unmarked if !matches!(constant.vis, syn::Visibility::Public(_)) => {
            return Ok(None);
        }
        Disposition::Unmarked => {}
    }
    if let Some(error) = super::misplaced_stream_marker(&constant.attrs, "associated const")? {
        return Err(error);
    }
    DeclarationSyntax::from(constant)
        .scan(scope, declared_types, Some(owner))
        .map(Some)
}

fn resolve_owner(
    item: &syn::ItemImpl,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<Option<ConstantOwner>, ScanError> {
    let target = impl_target::Target::scan(item);
    let Some(path) = declared_types.resolve_impl_target(scope, &target)? else {
        return Ok(None);
    };
    Ok(match declared_types.resolve(&path) {
        Some(DeclaredType::Record(id)) => Some(ConstantOwner::Record(id.clone())),
        Some(DeclaredType::Enum(id)) => Some(ConstantOwner::Enum(id.clone())),
        Some(DeclaredType::Class(id)) => Some(ConstantOwner::Class(id.clone())),
        _ => None,
    })
}

fn constant_type(types: &type_expr::Scanner<'_>, ty: &syn::Type) -> Result<TypeExpr, ScanError> {
    match type_expr::unwrapped(ty) {
        syn::Type::Reference(reference) if reference.mutability.is_none() => {
            borrowed_constant_type(types, ty, &reference.elem)
        }
        _ => types.scan(ty),
    }
}

fn borrowed_constant_type(
    types: &type_expr::Scanner<'_>,
    source: &syn::Type,
    element: &syn::Type,
) -> Result<TypeExpr, ScanError> {
    match types.scan(element)? {
        TypeExpr::Str => Ok(TypeExpr::Str),
        TypeExpr::Slice(inner) if matches!(inner.as_ref(), TypeExpr::Primitive(Primitive::U8)) => {
            Ok(TypeExpr::Slice(inner))
        }
        _ => Err(ScanError::unsupported_type(source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        CanonicalName, ConstExpr, EnumId, FloatLiteral, IntegerLiteral, Literal, NamePart, Path,
        PathRoot, PathSegment, Primitive, RecordId, Source, SourceName, TypeExpr, Visibility,
    };

    fn parse(source: &str) -> syn::ItemConst {
        syn::parse_str(source).expect("valid constant source")
    }

    fn scan(source: &str) -> Result<ConstantDef, ScanError> {
        super::build_item(
            &parse(source),
            &ModuleScope::root("demo"),
            &DeclaredTypes::new(),
        )
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn scans_complete_integer_constant_contract() {
        let constant = scan("pub const ANSWER: u32 = 42;").expect("scan");
        let mut expected = ConstantDef::new(
            ConstantId::new("demo::ANSWER"),
            SourceName::new("ANSWER", name(&["answer"])),
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(42, "42"))),
        );
        expected.source = Source::new(Visibility::Public, None);

        assert_eq!(constant, expected);
    }

    #[test]
    fn scans_string_float_bytes_array_tuple_and_raw_values() {
        let borrowed_string = scan("pub const NAME: &str = \"bolt\";").expect("scan");
        assert_eq!(borrowed_string.type_expr, TypeExpr::Str);
        assert_eq!(
            borrowed_string.value,
            ConstExpr::Literal(Literal::String("bolt".to_owned()))
        );
        assert_eq!(
            scan("pub const STATIC_NAME: &'static str = \"bolt\";")
                .expect("scan")
                .type_expr,
            TypeExpr::Str
        );
        assert_eq!(
            scan("pub const RATIO: f64 = -1.5f64;").expect("scan").value,
            ConstExpr::Literal(Literal::Float(FloatLiteral::new("-1.5f64")))
        );
        assert_eq!(
            scan("pub const MAGIC: Vec<u8> = b\"ffi\";")
                .expect("scan")
                .value,
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec()))
        );
        let bytes = scan("pub const BYTES: &'static [u8] = b\"ffi\";").expect("scan");
        assert_eq!(
            bytes.type_expr,
            TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))
        );
        assert_eq!(
            bytes.value,
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec()))
        );
        assert_eq!(
            scan("pub const PAIR: (bool, u8) = (true, 7);")
                .expect("scan")
                .value,
            ConstExpr::Tuple(vec![
                ConstExpr::Literal(Literal::Bool(true)),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(7, "7"))),
            ])
        );
        assert_eq!(
            scan("pub const FLAGS: Vec<u8> = [1, 2];")
                .expect("scan")
                .value,
            ConstExpr::Array(vec![
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ])
        );
        assert_eq!(
            scan("pub const MASK: u32 = 1 << 2;").expect("scan").value,
            ConstExpr::Raw("1 << 2".to_owned())
        );
    }

    #[test]
    fn scans_enum_path_constant_against_declared_type() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_enum(EnumId::new("demo::Mode"));
        let constant = super::build_item(
            &parse("pub const DEFAULT_MODE: Mode = Mode::Fast;"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect("scan");

        assert_eq!(
            constant.type_expr,
            TypeExpr::enumeration(EnumId::new("demo::Mode"), Path::single("Mode"))
        );
        assert_eq!(
            constant.value,
            ConstExpr::Path(Path::new(
                PathRoot::Relative,
                vec![PathSegment::new("Mode"), PathSegment::new("Fast")]
            ))
        );
    }

    #[test]
    fn scans_accessor_backed_enum_constant_expressions() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_enum(EnumId::new("demo::Shape"));
        let accessor = super::build_item(
            &parse("pub const DEFAULT: Shape = make_default_shape();"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect("scan accessor-backed enum constant");

        assert_eq!(
            accessor.type_expr,
            TypeExpr::enumeration(EnumId::new("demo::Shape"), Path::single("Shape"))
        );
        assert_eq!(
            accessor.value,
            ConstExpr::Raw("make_default_shape ()".to_owned())
        );

        let payload_literal = super::build_item(
            &parse("pub const CIRCLE: Shape = Shape::Circle { radius: 1.0 };"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect("scan payload enum constant as accessor-backed raw");

        assert_eq!(
            payload_literal.value,
            ConstExpr::Raw("Shape :: Circle { radius : 1.0 }".to_owned())
        );
    }

    #[test]
    fn scans_multi_word_name_and_restricted_visibility() {
        let constant = scan("pub(crate) const DEFAULT_LIMIT: u32 = 8;").expect("scan");

        assert_eq!(constant.id, ConstantId::new("demo::DEFAULT_LIMIT"));
        assert_eq!(constant.name.canonical(), &name(&["default", "limit"]));
        assert_eq!(
            constant.source.visibility,
            Visibility::Restricted("crate".to_owned())
        );
    }

    #[test]
    fn rejects_unknown_constant_type_before_losing_the_declaration() {
        let error = scan("pub const ORIGIN: Point = Point::ORIGIN;").expect_err("type rejects");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn rejects_non_string_and_non_byte_reference_constants() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let error = super::build_item(
            &parse("pub const ORIGIN: &Point = &Point::ORIGIN;"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect_err("borrowed record constant rejects");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "&Point"
        ));
    }

    #[test]
    fn rejects_mutable_borrowed_string_constant_type() {
        let error = scan("pub const NAME: &mut str = todo!();").expect_err("type rejects");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "&mut str"
        ));
    }

    #[test]
    fn rejects_anonymous_exported_constant_before_building_an_empty_name() {
        let error = scan("pub const _: u32 = 42;").expect_err("anonymous const rejects");

        assert_eq!(error, ScanError::AnonymousConstant);
    }
}
