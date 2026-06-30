use boltffi_ast::{ConstantDef, ConstantId, Primitive, TypeExpr};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::const_expr;
use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::type_expr;
use crate::{ModuleScope, ScanError, attributes, name};

pub fn scan(
    marked: &Marked<'_, syn::ItemConst>,
    declared_types: &DeclaredTypes,
) -> Result<ConstantDef, ScanError> {
    build(marked.item(), marked.scope(), declared_types)
}

fn build(
    item: &syn::ItemConst,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<ConstantDef, ScanError> {
    let ident = &item.ident;
    if ident == "_" {
        return Err(ScanError::AnonymousConstant);
    }
    let types = type_expr::Scanner::new(declared_types, scope);
    let rust_type = constant_type(&types, &item.ty)?;
    let value = const_expr::Scanner::new(&types).scan(&item.expr);
    let mut constant = ConstantDef::new(
        ConstantId::new(scope.path().qualified(&ident.to_string())),
        name::source(ident),
        rust_type,
        value,
    );
    let attrs = Attributes::new(&item.attrs, &types);
    constant.source = attributes::source(&item.vis, scope, item.span());
    constant.source_span = constant.source.span.clone();
    constant.doc = attrs.doc();
    constant.deprecated = attrs.deprecated()?;
    constant.user_attrs = attrs.user_attrs();
    Ok(constant)
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
        super::build(
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
        let constant = super::build(
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
        let accessor = super::build(
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

        let payload_literal = super::build(
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
        let error = super::build(
            &parse("pub const ORIGIN: &Point = &Point::ORIGIN;"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect_err("borrowed record constant rejects");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling.contains("&Point")
        ));
    }

    #[test]
    fn rejects_mutable_borrowed_string_constant_type() {
        let error = scan("pub const NAME: &mut str = todo!();").expect_err("type rejects");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling.contains("&mut str")
        ));
    }

    #[test]
    fn rejects_anonymous_exported_constant_before_building_an_empty_name() {
        let error = scan("pub const _: u32 = 42;").expect_err("anonymous const rejects");

        assert_eq!(error, ScanError::AnonymousConstant);
    }
}
