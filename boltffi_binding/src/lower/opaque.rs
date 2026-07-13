//! Cycle-safe source-AST containment checks for native opaque records.

use std::collections::HashSet;

use boltffi_ast::{RecordEncoding, TypeExpr, VariantPayload};

use super::index::Index;

/// Returns whether `type_expr` contains a native opaque record, following
/// named records, enum payloads, and custom representation types. A visited
/// declaration set makes recursive declaration graphs terminate.
pub(super) fn contains(index: &Index, type_expr: &TypeExpr) -> bool {
    let mut visited = HashSet::new();
    contains_with_visited(index, type_expr, &mut visited)
}

fn contains_with_visited(
    index: &Index,
    type_expr: &TypeExpr,
    visited: &mut HashSet<String>,
) -> bool {
    match type_expr {
        TypeExpr::Record { id, .. } => {
            let key = format!("record:{}", id.as_str());
            if !visited.insert(key) {
                return false;
            }
            let Some(record) = index.record(id) else {
                return false;
            };
            record.encoding == RecordEncoding::NativeOpaque
                || record
                    .fields
                    .iter()
                    .any(|field| contains_with_visited(index, &field.type_expr, visited))
        }
        TypeExpr::Enum { id, .. } => {
            let key = format!("enum:{}", id.as_str());
            if !visited.insert(key) {
                return false;
            }
            index.enumeration(id).is_some_and(|enumeration| {
                enumeration
                    .variants
                    .iter()
                    .any(|variant| match &variant.payload {
                        VariantPayload::Unit => false,
                        VariantPayload::Tuple(items) => items
                            .iter()
                            .any(|item| contains_with_visited(index, item, visited)),
                        VariantPayload::Struct(fields) => fields
                            .iter()
                            .any(|field| contains_with_visited(index, &field.type_expr, visited)),
                    })
            })
        }
        TypeExpr::Custom { id, .. } => {
            let key = format!("custom:{}", id.as_str());
            visited.insert(key)
                && index
                    .custom(id)
                    .is_some_and(|custom| contains_with_visited(index, &custom.repr, visited))
        }
        TypeExpr::Option(inner)
        | TypeExpr::Vec(inner)
        | TypeExpr::Slice(inner)
        | TypeExpr::Boxed(inner)
        | TypeExpr::Arc(inner) => contains_with_visited(index, inner, visited),
        TypeExpr::Result { ok, err } => {
            contains_with_visited(index, ok, visited) || contains_with_visited(index, err, visited)
        }
        TypeExpr::Tuple(items) => items
            .iter()
            .any(|item| contains_with_visited(index, item, visited)),
        TypeExpr::Map { key, value, .. } => {
            contains_with_visited(index, key, visited)
                || contains_with_visited(index, value, visited)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, CustomRemoteType, CustomTypeConverter, CustomTypeConverters, CustomTypeDef,
        CustomTypeId, FieldDef, PackageInfo, Path, RecordDef, RecordId, SourceContract, VariantDef,
        VariantPayload,
    };

    use super::*;

    fn name(value: &str) -> CanonicalName {
        CanonicalName::single(value)
    }

    fn record(id: &str, name_: &str, fields: Vec<FieldDef>) -> RecordDef {
        let mut record = RecordDef::new(RecordId::new(id), name(name_));
        record.fields = fields;
        record
    }

    fn reference(id: &str, name_: &str) -> TypeExpr {
        TypeExpr::record(RecordId::new(id), Path::single(name_))
    }

    fn opaque() -> RecordDef {
        let mut record = record("demo::Opaque", "Opaque", Vec::new());
        record.encoding = RecordEncoding::NativeOpaque;
        record
    }

    fn custom(id: &str, name_: &str, repr: TypeExpr) -> CustomTypeDef {
        CustomTypeDef::new(
            CustomTypeId::new(id),
            name(name_),
            CustomRemoteType::single_path("Remote"),
            repr,
            None,
            CustomTypeConverters::new(
                CustomTypeConverter::path(Path::single("into_ffi")),
                CustomTypeConverter::path(Path::single("try_from_ffi")),
            ),
        )
    }

    #[test]
    fn direct_opaque_record_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(opaque());

        assert!(contains(
            &Index::new(&source),
            &reference("demo::Opaque", "Opaque")
        ));
    }

    #[test]
    fn named_record_directly_containing_opaque_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![
            opaque(),
            record(
                "demo::Envelope",
                "Envelope",
                vec![FieldDef::new(
                    name("opaque"),
                    reference("demo::Opaque", "Opaque"),
                )],
            ),
        ];

        assert!(contains(
            &Index::new(&source),
            &reference("demo::Envelope", "Envelope")
        ));
    }

    #[test]
    fn multihop_named_record_path_to_opaque_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![
            opaque(),
            record(
                "demo::Inner",
                "Inner",
                vec![FieldDef::new(
                    name("opaque"),
                    reference("demo::Opaque", "Opaque"),
                )],
            ),
            record(
                "demo::Outer",
                "Outer",
                vec![FieldDef::new(
                    name("inner"),
                    reference("demo::Inner", "Inner"),
                )],
            ),
        ];

        assert!(contains(
            &Index::new(&source),
            &reference("demo::Outer", "Outer")
        ));
    }

    #[test]
    fn diamond_named_record_path_to_opaque_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![
            opaque(),
            record(
                "demo::Left",
                "Left",
                vec![FieldDef::new(
                    name("opaque"),
                    reference("demo::Opaque", "Opaque"),
                )],
            ),
            record(
                "demo::Right",
                "Right",
                vec![FieldDef::new(name("value"), TypeExpr::String)],
            ),
            record(
                "demo::Root",
                "Root",
                vec![
                    FieldDef::new(name("left"), reference("demo::Left", "Left")),
                    FieldDef::new(name("right"), reference("demo::Right", "Right")),
                ],
            ),
        ];

        assert!(contains(
            &Index::new(&source),
            &reference("demo::Root", "Root")
        ));
    }

    #[test]
    fn non_opaque_recursive_named_record_cycle_does_not_contain_opaque() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![
            record(
                "demo::A",
                "A",
                vec![FieldDef::new(name("b"), reference("demo::B", "B"))],
            ),
            record(
                "demo::B",
                "B",
                vec![FieldDef::new(name("a"), reference("demo::A", "A"))],
            ),
        ];

        assert!(!contains(&Index::new(&source), &reference("demo::A", "A")));
    }

    #[test]
    fn recursive_named_record_cycle_reaching_opaque_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![
            record(
                "demo::A",
                "A",
                vec![FieldDef::new(name("b"), reference("demo::B", "B"))],
            ),
            record(
                "demo::B",
                "B",
                vec![
                    FieldDef::new(name("a"), reference("demo::A", "A")),
                    FieldDef::new(name("opaque"), reference("demo::Opaque", "Opaque")),
                ],
            ),
            opaque(),
        ];

        assert!(contains(&Index::new(&source), &reference("demo::A", "A")));
    }

    #[test]
    fn custom_representation_directly_containing_opaque_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(opaque());
        source.customs.push(custom(
            "demo::OpaqueAlias",
            "OpaqueAlias",
            reference("demo::Opaque", "Opaque"),
        ));

        assert!(contains(
            &Index::new(&source),
            &TypeExpr::custom("demo::OpaqueAlias".into(), Path::single("OpaqueAlias")),
        ));
    }

    #[test]
    fn custom_representation_containing_named_record_with_opaque_is_contained() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![
            opaque(),
            record(
                "demo::Envelope",
                "Envelope",
                vec![FieldDef::new(
                    name("opaque"),
                    reference("demo::Opaque", "Opaque"),
                )],
            ),
        ];
        source.customs.push(custom(
            "demo::EnvelopeAlias",
            "EnvelopeAlias",
            reference("demo::Envelope", "Envelope"),
        ));

        assert!(contains(
            &Index::new(&source),
            &TypeExpr::custom("demo::EnvelopeAlias".into(), Path::single("EnvelopeAlias")),
        ));
    }

    #[test]
    fn enum_tuple_and_struct_payloads_are_followed() {
        let mut enumeration = boltffi_ast::EnumDef::new("demo::Choice".into(), name("Choice"));
        let mut tuple = VariantDef::unit(name("Tuple"));
        tuple.payload = VariantPayload::Tuple(vec![reference("demo::Opaque", "Opaque")]);
        let mut structure = VariantDef::unit(name("Struct"));
        structure.payload = VariantPayload::Struct(vec![FieldDef::new(
            name("opaque"),
            reference("demo::Opaque", "Opaque"),
        )]);
        enumeration.variants = vec![tuple, structure];
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(opaque());
        source.enums.push(enumeration);
        let index = Index::new(&source);
        assert!(contains(
            &index,
            &TypeExpr::enumeration("demo::Choice".into(), Path::single("Choice")),
        ));
    }
}
