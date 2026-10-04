use boltffi_ast::{EnumDef, EnumId, FieldDef, VariantDef, VariantPayload};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::unsupported::UnsupportedFeature;
use crate::{ModuleScope, ScanError, attributes, name, repr, unsupported};

pub fn scan(
    marked: &Marked<'_, syn::ItemEnum>,
    declared_types: &DeclaredTypes,
) -> Result<EnumDef, ScanError> {
    let mut enumeration = build(marked.item(), marked.scope(), declared_types)?;
    marked
        .marker()
        .append_value_attrs(&mut enumeration.user_attrs);
    Ok(enumeration)
}

fn build(
    item: &syn::ItemEnum,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<EnumDef, ScanError> {
    unsupported::generics(&item.generics, &format!("enum {}", item.ident))?;
    let id = EnumId::new(scope.path().qualified(&item.ident.to_string()));
    let mut enumeration = EnumDef::new(id, name::source(&item.ident));
    let scanner = Scanner::new(declared_types, scope);
    let attrs = Attributes::new(&item.attrs, &scanner);
    enumeration.repr = repr::scan(&item.attrs);
    enumeration.source = attributes::source(&item.vis, scope, item.span());
    enumeration.source_span = enumeration.source.span.clone();
    enumeration.doc = attrs.doc();
    enumeration.deprecated = attrs.deprecated()?;
    enumeration.user_attrs = attrs.user_attrs();
    enumeration.variants = item
        .variants
        .iter()
        .map(|variant| variant_def(variant, &scanner))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(enumeration)
}

fn variant_def(variant: &syn::Variant, scanner: &Scanner<'_>) -> Result<VariantDef, ScanError> {
    let mut declaration = VariantDef::unit(name::source(&variant.ident));
    let attrs = Attributes::new(&variant.attrs, scanner);
    declaration.discriminant = discriminant(variant)?;
    declaration.payload = payload(&variant.fields, scanner)?;
    declaration.transparent = attrs.transparent()?;
    declaration.source = attributes::public_source(scanner.scope(), variant.span());
    declaration.source_span = declaration.source.span.clone();
    declaration.doc = attrs.doc();
    declaration.user_attrs = attrs.user_attrs();
    Ok(declaration)
}

fn payload(fields: &syn::Fields, scanner: &Scanner<'_>) -> Result<VariantPayload, ScanError> {
    match fields {
        syn::Fields::Unit => Ok(VariantPayload::Unit),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .map(|field| scanner.scan(&field.ty))
            .collect::<Result<Vec<_>, _>>()
            .map(VariantPayload::Tuple),
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|field| named_field(field, scanner))
            .collect::<Result<Vec<_>, _>>()
            .map(VariantPayload::Struct),
    }
}

fn named_field(field: &syn::Field, scanner: &Scanner<'_>) -> Result<FieldDef, ScanError> {
    let ident = field
        .ident
        .as_ref()
        .expect("named variant field carries an identifier");
    let mut field_def = FieldDef::new(name::source(ident), scanner.scan(&field.ty)?);
    let attrs = Attributes::new(&field.attrs, scanner);
    field_def.source = attributes::source(&field.vis, scanner.scope(), field.span());
    field_def.source_span = field_def.source.span.clone();
    field_def.doc = attrs.doc();
    field_def.default = attrs.default()?;
    field_def.user_attrs = attrs.user_attrs();
    Ok(field_def)
}

fn discriminant(variant: &syn::Variant) -> Result<Option<i128>, ScanError> {
    match &variant.discriminant {
        None => Ok(None),
        Some((_, expr)) => discriminant_value(expr).map(Some),
    }
}

fn discriminant_value(expr: &syn::Expr) -> Result<i128, ScanError> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(literal),
            ..
        }) => literal
            .base10_parse::<i128>()
            .map_err(|_| unsupported::feature(UnsupportedFeature::NonLiteralEnumDiscriminant)),
        syn::Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => Ok(-discriminant_value(expr)?),
        _ => Err(unsupported::feature(
            UnsupportedFeature::NonLiteralEnumDiscriminant,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{CanonicalName, NamePart, Path, Primitive, RecordId, ReprItem, TypeExpr};

    fn parse(source: &str) -> syn::ItemEnum {
        syn::parse_str(source).expect("valid enum source")
    }

    fn scan(source: &str) -> Result<EnumDef, ScanError> {
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
    fn scans_unit_variants_with_repr_and_discriminants() {
        let enumeration = scan("#[repr(u8)] pub enum Mode { Fast = 0, Slow = 1 }").expect("scan");

        assert_eq!(enumeration.id, EnumId::new("demo::Mode"));
        assert_eq!(enumeration.name.canonical(), &name(&["mode"]));
        assert_eq!(
            enumeration.repr.items,
            vec![ReprItem::Primitive(Primitive::U8)]
        );
        assert_eq!(enumeration.variants.len(), 2);
        assert_eq!(enumeration.variants[0].name.canonical(), &name(&["fast"]));
        assert_eq!(enumeration.variants[0].discriminant, Some(0));
        assert_eq!(enumeration.variants[0].payload, VariantPayload::Unit);
        assert_eq!(enumeration.variants[1].discriminant, Some(1));
    }

    #[test]
    fn scans_tuple_and_struct_variant_payloads() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let enumeration = super::build(
            &parse("pub enum Shape { Dot(Point), Rect { width: f64, height: f64 } }"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect("scan");

        assert_eq!(
            enumeration.variants[0].payload,
            VariantPayload::Tuple(vec![TypeExpr::record(
                RecordId::new("demo::Point"),
                Path::single("Point")
            )])
        );
        match &enumeration.variants[1].payload {
            VariantPayload::Struct(fields) => {
                assert_eq!(fields[0].name.canonical(), &name(&["width"]));
                assert_eq!(fields[0].type_expr, TypeExpr::Primitive(Primitive::F64));
            }
            other => panic!("expected struct payload, got {other:?}"),
        }
    }

    #[test]
    fn negative_discriminant_is_captured() {
        let enumeration = scan("pub enum Sign { Neg = -1, Zero = 0 }").expect("scan");

        assert_eq!(enumeration.variants[0].discriminant, Some(-1));
        assert_eq!(enumeration.variants[1].discriminant, Some(0));
    }

    #[test]
    fn non_literal_discriminant_is_rejected() {
        let error = scan("pub enum Mask { A = 1 << 2 }").expect_err("non-literal must reject");

        assert_eq!(
            error,
            unsupported::feature(UnsupportedFeature::NonLiteralEnumDiscriminant)
        );
    }

    #[test]
    fn transparent_attribute_is_captured() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let enumeration = super::build(
            &parse("pub enum Shape { #[boltffi::transparent] Dot(Point), Empty }"),
            &ModuleScope::root("demo"),
            &declared_types,
        )
        .expect("scan");

        assert!(enumeration.variants[0].transparent);
        assert!(!enumeration.variants[1].transparent);
    }

    #[test]
    fn transparent_attribute_with_input_is_rejected() {
        let error = scan("pub enum Shape { #[boltffi::transparent(1)] Dot(u32) }")
            .expect_err("transparent takes no input");

        assert!(matches!(error, ScanError::InvalidAttribute { .. }));
    }

    #[test]
    fn rejects_generic_enum_before_erasing_type_parameters() {
        let error = scan("pub enum Boxed<T> { Value(T) }").expect_err("generic rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "enum Boxed".to_owned()
            }
        );
    }
}
