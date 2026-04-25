use std::collections::HashSet;

use crate::ir::FfiContract;
use crate::ir::definitions::{EnumDef, EnumRepr, VariantPayload};
use crate::ir::ids::{EnumId, RecordId};
use crate::ir::types::TypeExpr;

use super::super::ast::CSharpEnumUnderlyingType;
use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Computes which records and enums the backend can render, jointly.
    ///
    /// Records and enums can reference each other (a record field may be a
    /// data enum; a data-enum variant field may be a record), so neither set
    /// can be computed independently. The two sets grow together in one
    /// fixed-point loop: each pass tries to admit every not-yet-supported
    /// record and data enum against the current state of both sets, until a
    /// pass produces no new admissions. C-style enums seed the enum set up
    /// front (any whose `repr` is a legal C# enum backing type) since they
    /// carry no variant payload.
    ///
    /// Termination: each progressing iteration admits at least one new
    /// record or data enum, both catalogs are finite, and admissions are
    /// monotonic.
    ///
    /// Mutually recursive types whose admission requires each other to be
    /// admitted first never make progress: the first pass finds neither
    /// admissible, the loop exits, and both fall out of the supported sets.
    pub(super) fn compute_supported_sets(
        ffi: &FfiContract,
    ) -> (HashSet<RecordId>, HashSet<EnumId>) {
        let mut enums: HashSet<EnumId> = ffi
            .catalog
            .all_enums()
            .filter(|e| match &e.repr {
                EnumRepr::CStyle { tag_type, .. } => {
                    CSharpEnumUnderlyingType::for_primitive(*tag_type).is_some()
                }
                EnumRepr::Data { .. } => false,
            })
            .map(|e| e.id.clone())
            .collect();
        let mut records: HashSet<RecordId> = HashSet::new();

        loop {
            let record_additions: Vec<RecordId> = ffi
                .catalog
                .all_records()
                .filter(|r| !records.contains(&r.id))
                .filter(|r| {
                    r.fields
                        .iter()
                        .all(|f| is_field_type_supported(&f.type_expr, &records, &enums))
                })
                .map(|r| r.id.clone())
                .collect();
            let enum_additions: Vec<EnumId> = ffi
                .catalog
                .all_enums()
                .filter(|e| matches!(e.repr, EnumRepr::Data { .. }))
                .filter(|e| !enums.contains(&e.id))
                .filter(|e| enum_variant_fields_supported(e, &records, &enums))
                .map(|e| e.id.clone())
                .collect();
            if record_additions.is_empty() && enum_additions.is_empty() {
                break;
            }
            records.extend(record_additions);
            enums.extend(enum_additions);
        }
        (records, enums)
    }
}

/// Whether every variant's payload field type is supported. Vacuously
/// true for non-Data enums.
fn enum_variant_fields_supported(
    enum_def: &EnumDef,
    records: &HashSet<RecordId>,
    enums: &HashSet<EnumId>,
) -> bool {
    let EnumRepr::Data { variants, .. } = &enum_def.repr else {
        return true;
    };
    variants.iter().all(|v| match &v.payload {
        VariantPayload::Unit => true,
        VariantPayload::Tuple(types) => types
            .iter()
            .all(|t| is_field_type_supported(t, records, enums)),
        VariantPayload::Struct(fields) => fields
            .iter()
            .all(|f| is_field_type_supported(&f.type_expr, records, enums)),
    })
}

/// Whether `ty` is a supported field/element type, given the current
/// admission state of records and enums.
fn is_field_type_supported(
    ty: &TypeExpr,
    records: &HashSet<RecordId>,
    enums: &HashSet<EnumId>,
) -> bool {
    match ty {
        TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
        TypeExpr::Record(id) => records.contains(id),
        TypeExpr::Enum(id) => enums.contains(id),
        TypeExpr::Vec(inner) => is_field_type_supported(inner, records, enums),
        // C# models `Option<T>` as `T?`, so `Option<Option<T>>` would
        // need `T??`, which the language rejects and which can't be
        // flattened without losing the `Some(None)` state.
        TypeExpr::Option(inner) => {
            !matches!(inner.as_ref(), TypeExpr::Option(_))
                && is_field_type_supported(inner, records, enums)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{data_enum, record_with_one_field, struct_variant};
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::PackageInfo;
    use crate::ir::definitions::{CStyleVariant, FunctionDef, ParamDef, ParamPassing, ReturnDef};
    use crate::ir::ids::{FunctionId, ParamName};
    use crate::ir::types::PrimitiveType;
    use boltffi_ffi_rules::callable::ExecutionKind;

    use super::super::super::CSharpOptions;

    /// A record field that points at a data enum must still let the
    /// record qualify as supported. Records and data enums are computed
    /// in a joint fixed-point precisely so a record can wait a pass for
    /// the data enum it references, and vice versa.
    #[test]
    fn record_referencing_data_enum_is_admitted_jointly() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_enum(data_enum(
            "shape",
            vec![struct_variant(
                "Circle",
                0,
                vec![("radius", TypeExpr::Primitive(PrimitiveType::F64))],
            )],
        ));
        contract.catalog.insert_record(record_with_one_field(
            "holder",
            "shape",
            TypeExpr::Enum(EnumId::new("shape")),
        ));

        let (records, enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            enums.contains(&EnumId::new("shape")),
            "expecting the data enum to be admitted first so the record can reference it",
        );
        assert!(
            records.contains(&RecordId::new("holder")),
            "expecting the record with a data-enum field to be admitted once the enum joins the set",
        );
    }

    /// A data enum whose variant carries another data enum must still be
    /// admitted to `supported_enums`. The fixed-point lets `outer` join
    /// on the iteration after `inner` is admitted, even though they're
    /// declared in a single pass.
    #[test]
    fn data_enum_referencing_another_data_enum_is_admitted() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_enum(data_enum(
            "inner",
            vec![struct_variant(
                "Value",
                0,
                vec![("n", TypeExpr::Primitive(PrimitiveType::I32))],
            )],
        ));
        contract.catalog.insert_enum(data_enum(
            "outer",
            vec![struct_variant(
                "Wrap",
                0,
                vec![("inner", TypeExpr::Enum(EnumId::new("inner")))],
            )],
        ));

        let (_records, enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            enums.contains(&EnumId::new("inner")),
            "expecting the leaf data enum to be admitted",
        );
        assert!(
            enums.contains(&EnumId::new("outer")),
            "expecting the data enum referencing another data enum to join on a later fixed-point iteration",
        );
    }

    /// C# enums only support fixed-width integral backing types. A Rust
    /// `#[repr(usize)]` C-style enum therefore stays out of the supported
    /// set so the backend never tries to render an illegal `enum : nuint`.
    #[test]
    fn c_style_enum_with_usize_repr_is_not_admitted() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("platform_status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::USize,
                variants: vec![CStyleVariant {
                    name: "Ready".into(),
                    discriminant: 0,
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let (_records, enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            !enums.contains(&EnumId::new("platform_status")),
            "expecting repr(usize) C-style enums to stay unsupported until the backend has a legal C# projection",
        );
    }

    /// C# projects `Option<T>` as `T?`, so `Option<Option<i32>>` would
    /// need `int??`, which does not parse. Reject the shape at the
    /// backend support gate rather than silently emitting uncompilable
    /// code or flattening away the `Some(None)` state.
    #[test]
    fn nested_option_shapes_are_rejected() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        let nested_option = TypeExpr::Option(Box::new(TypeExpr::Option(Box::new(
            TypeExpr::Primitive(PrimitiveType::I32),
        ))));
        contract.catalog.insert_record(record_with_one_field(
            "holder",
            "value",
            nested_option.clone(),
        ));
        contract.functions.push(FunctionDef {
            id: FunctionId::new("echo_nested_option"),
            params: vec![ParamDef {
                name: ParamName::new("value"),
                type_expr: nested_option.clone(),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Value(nested_option.clone()),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);
        let (records, _enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            !records.contains(&RecordId::new("holder")),
            "expecting a record with Option<Option<i32>> field to stay unsupported because it would render as int??",
        );
        assert!(
            !lowerer.is_supported_type(&nested_option),
            "expecting Option<Option<i32>> to fail the C# support gate before lowering",
        );
        assert!(
            lowerer.lower_function(&contract.functions[0]).is_none(),
            "expecting a function with nested Option param/return to be dropped rather than emitting int??",
        );
    }
}
