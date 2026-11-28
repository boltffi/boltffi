use boltffi_ast::{FieldDef, RecordDef, RecordId};

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::type_expr::Scanner;
use crate::unsupported::UnsupportedFeature;
use crate::{ScanError, attributes, name, repr, unsupported};

pub(crate) fn scan_item(
    item: &syn::ItemStruct,
    declared_types: &DeclaredTypes,
) -> Result<RecordDef, ScanError> {
    unsupported::generics(&item.generics, &format!("record {}", item.ident))?;
    let id = RecordId::new(name::self_id(&item.ident));
    let mut record = RecordDef::new(id, name::source(&item.ident));
    let scanner = Scanner::new(declared_types);
    let attrs = Attributes::new(&item.attrs, &scanner);
    record.repr = repr::scan(&item.attrs);
    record.source = attributes::source(&item.vis);
    record.doc = attrs.doc();
    record.deprecated = attrs.deprecated()?;
    record.user_attrs = attrs.user_attrs();
    record.fields = record_fields(&item.fields, &scanner).map_err(|error| error.at(&item.ident))?;
    Ok(record)
}

fn record_fields(fields: &syn::Fields, scanner: &Scanner<'_>) -> Result<Vec<FieldDef>, ScanError> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|field| record_field(field, scanner))
            .collect(),
        syn::Fields::Unnamed(unnamed) => {
            Err(unsupported::feature(UnsupportedFeature::TupleStruct).at(unnamed))
        }
        syn::Fields::Unit => Err(unsupported::feature(UnsupportedFeature::UnitStruct)),
    }
}

fn record_field(field: &syn::Field, scanner: &Scanner<'_>) -> Result<FieldDef, ScanError> {
    let ident = field
        .ident
        .as_ref()
        .ok_or_else(|| unsupported::feature(UnsupportedFeature::TupleStruct))?;
    let mut scanned = FieldDef::new(name::source(ident), scanner.scan(&field.ty)?);
    let attrs = Attributes::new(&field.attrs, scanner);
    scanned.source = attributes::source(&field.vis);
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
        super::scan_item(&parse(source), &DeclaredTypes::new())
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn scans_complete_named_field_record_contract() {
        let record = scan("pub struct Point { pub x: f64, pub y: f64 }").expect("scan");
        let mut expected = RecordDef::new(
            RecordId::new("$self::Point"),
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

        assert_eq!(
            tuple.unlocated().clone(),
            unsupported::feature(UnsupportedFeature::TupleStruct)
        );
        assert_eq!(
            unit.unlocated().clone(),
            unsupported::feature(UnsupportedFeature::UnitStruct)
        );
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

        assert_eq!(record.id, RecordId::new("$self::HTTPRequest"));
        assert_eq!(record.name.canonical(), &name(&["http", "request"]));
        assert_eq!(record.fields[0].name.canonical(), &name(&["user", "id"]));
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
    fn defers_record_typed_field_to_a_slot() {
        let declared_types = DeclaredTypes::new();
        let record = super::scan_item(
            &parse("pub struct Shape { pub center: Point }"),
            &declared_types,
        )
        .expect("scan");

        assert_eq!(
            record.fields[0].type_expr,
            TypeExpr::record(RecordId::new("$slot:0"), Path::single("Point"))
        );
    }

    #[test]
    fn rejects_generic_record_before_erasing_type_parameters() {
        let error = scan("pub struct Array<const N: usize> { pub len: usize }")
            .expect_err("generic rejected");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::UnsupportedGenerics {
                item: "record Array".to_owned()
            }
        );
    }
}
