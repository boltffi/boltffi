use boltffi_ast::{FieldDef, RecordDef, RecordEncoding, RecordId};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::unsupported::UnsupportedFeature;
use crate::{ModuleScope, ScanError, attributes, name, repr, unsupported};

pub fn scan(
    marked: &Marked<'_, syn::ItemStruct>,
    declared_types: &DeclaredTypes,
) -> Result<RecordDef, ScanError> {
    let encoding = match marked.marker() {
        crate::marker::Marker::DataOpaque => RecordEncoding::NativeOpaque,
        _ => RecordEncoding::Standard,
    };
    let mut record = build(marked.item(), marked.scope(), declared_types, encoding)?;
    marked.marker().append_value_attrs(&mut record.user_attrs);
    Ok(record)
}

fn build(
    item: &syn::ItemStruct,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
    encoding: RecordEncoding,
) -> Result<RecordDef, ScanError> {
    unsupported::generics(&item.generics, &format!("record {}", item.ident))?;
    let id = RecordId::new(scope.path().qualified(&item.ident.to_string()));
    let mut record = RecordDef::new(id, name::source(&item.ident));
    let scanner = Scanner::new(declared_types, scope);
    let attrs = Attributes::new(&item.attrs, &scanner);
    record.repr = repr::scan(&item.attrs);
    record.encoding = encoding;
    record.source = attributes::source(&item.vis, scope, item.span());
    record.source_span = record.source.span.clone();
    record.doc = attrs.doc();
    record.deprecated = attrs.deprecated()?;
    record.user_attrs = attrs.user_attrs();
    record.fields = record_fields(&item.fields, &scanner, encoding)?;
    Ok(record)
}

fn record_fields(
    fields: &syn::Fields,
    scanner: &Scanner<'_>,
    encoding: RecordEncoding,
) -> Result<Vec<FieldDef>, ScanError> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|field| record_field(field, scanner, encoding))
            .collect(),
        syn::Fields::Unnamed(_) => Err(unsupported::feature(UnsupportedFeature::TupleStruct)),
        syn::Fields::Unit => Err(unsupported::feature(UnsupportedFeature::UnitStruct)),
    }
}

fn record_field(
    field: &syn::Field,
    scanner: &Scanner<'_>,
    _encoding: RecordEncoding,
) -> Result<FieldDef, ScanError> {
    let ident = field
        .ident
        .as_ref()
        .ok_or_else(|| unsupported::feature(UnsupportedFeature::TupleStruct))?;
    let type_expr = scanner.scan(&field.ty)?;
    let mut scanned = FieldDef::new(name::source(ident), type_expr);
    let attrs = Attributes::new(&field.attrs, scanner);
    scanned.source = attributes::source(&field.vis, scanner.scope(), field.span());
    scanned.source_span = scanned.source.span.clone();
    scanned.doc = attrs.doc();
    scanned.default = attrs.default()?;
    scanned.user_attrs = attrs.user_attrs();
    Ok(scanned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        CanonicalName, FieldDef, NamePart, Path, Primitive, ReprItem, Source, SourceName, TypeExpr,
        Visibility,
    };

    fn parse(source: &str) -> syn::ItemStruct {
        syn::parse_str(source).expect("valid struct source")
    }

    fn scan(source: &str) -> Result<RecordDef, ScanError> {
        super::build(
            &parse(source),
            &ModuleScope::root("demo"),
            &DeclaredTypes::new(),
            RecordEncoding::Standard,
        )
    }

    fn scan_opaque(source: &str) -> Result<RecordDef, ScanError> {
        super::build(
            &parse(source),
            &ModuleScope::root("demo"),
            &DeclaredTypes::new(),
            RecordEncoding::NativeOpaque,
        )
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn scans_complete_named_field_record_contract() {
        let record = scan("pub struct Point { pub x: f64, pub y: f64 }").expect("scan");
        let mut expected = RecordDef::new(
            RecordId::new("demo::Point"),
            SourceName::new("Point", name(&["point"])),
        );
        expected.source = Source::new(Visibility::Public, None);
        expected.fields = vec![
            FieldDef::new(name(&["x"]), TypeExpr::Primitive(Primitive::F64)),
            FieldDef::new(name(&["y"]), TypeExpr::Primitive(Primitive::F64)),
        ];

        assert_eq!(record, expected);
    }

    #[test]
    fn tuple_and_unit_structs_are_rejected() {
        let tuple = scan("pub struct Pair(i32, i32);").expect_err("tuple struct must reject");
        let unit = scan("pub struct Marker;").expect_err("unit struct must reject");

        assert_eq!(tuple, unsupported::feature(UnsupportedFeature::TupleStruct));
        assert_eq!(unit, unsupported::feature(UnsupportedFeature::UnitStruct));
    }

    #[test]
    fn scans_record_repr() {
        let record = scan("#[repr(C, align(8))] pub struct Point { pub x: f64 }").expect("scan");

        assert_eq!(record.repr.items, vec![ReprItem::C, ReprItem::Align(8)]);
    }

    #[test]
    fn scans_record_and_field_visibility() {
        let record = scan("pub(crate) struct Point { pub x: f64, y: f64 }").expect("scan");

        assert_eq!(
            record.source.visibility,
            Visibility::Restricted("crate".to_owned())
        );
        assert_eq!(record.fields[0].source.visibility, Visibility::Public);
        assert_eq!(record.fields[1].source.visibility, Visibility::Private);
        assert_eq!(record.fields[0].source.span, None);
        assert_eq!(record.fields[1].source.span, None);
    }

    #[test]
    fn scans_multi_word_names_as_parts() {
        let record = scan("pub struct HTTPRequest { pub user_id: i32 }").expect("scan");

        assert_eq!(record.id, RecordId::new("demo::HTTPRequest"));
        assert_eq!(record.name.canonical(), &name(&["http", "request"]));
        assert_eq!(record.fields[0].name.canonical(), &name(&["user", "id"]));
    }

    #[test]
    fn field_type_errors_keep_source_spelling() {
        let error = scan("pub struct Shape { pub point: Point }").expect_err("field type rejected");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn scans_string_and_collection_fields() {
        let record =
            scan("pub struct Person { pub name: String, pub scores: Vec<i32> }").expect("scan");

        assert_eq!(record.fields[0].type_expr, TypeExpr::String);
        assert_eq!(
            record.fields[1].type_expr,
            TypeExpr::vec(TypeExpr::Primitive(Primitive::I32))
        );
    }

    #[test]
    fn resolves_record_typed_field_against_registry() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let record = super::build(
            &parse("pub struct Shape { pub center: Point }"),
            &ModuleScope::root("demo"),
            &declared_types,
            RecordEncoding::Standard,
        )
        .expect("scan");

        assert_eq!(
            record.fields[0].type_expr,
            TypeExpr::record(RecordId::new("demo::Point"), Path::single("Point"))
        );
    }

    #[test]
    fn opaque_record_scans_owned_fields() {
        let record =
            scan_opaque("pub struct Snapshot { pub name: Option<String>, pub major: u32 }")
                .expect("opaque record");

        assert_eq!(record.encoding, RecordEncoding::NativeOpaque);
        assert_eq!(
            record.fields[0].type_expr,
            TypeExpr::option(TypeExpr::String)
        );
        assert_eq!(
            record.fields[1].type_expr,
            TypeExpr::Primitive(Primitive::U32)
        );
    }

    #[test]
    fn opaque_record_rejects_generics() {
        let error = scan_opaque("pub struct Snapshot<T> { pub value: T }")
            .expect_err("type generic rejected");
        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "record Snapshot".to_owned()
            }
        );
    }

    #[test]
    fn opaque_record_rejects_lifetime_generics() {
        let error = scan_opaque("pub struct Snapshot<'a> { pub name: &'a str }")
            .expect_err("lifetime generic rejected");
        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "record Snapshot".to_owned()
            }
        );
    }

    #[test]
    fn rejects_generic_record_before_erasing_type_parameters() {
        let error = scan("pub struct Array<const N: usize> { pub len: usize }")
            .expect_err("generic rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "record Array".to_owned()
            }
        );
    }
}
