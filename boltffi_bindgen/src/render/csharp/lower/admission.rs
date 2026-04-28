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
                        .all(|f| is_field_type_supported(ffi, &f.type_expr, &records, &enums))
                })
                .map(|r| r.id.clone())
                .collect();
            let enum_additions: Vec<EnumId> = ffi
                .catalog
                .all_enums()
                .filter(|e| matches!(e.repr, EnumRepr::Data { .. }))
                .filter(|e| !enums.contains(&e.id))
                .filter(|e| enum_variant_fields_supported(ffi, e, &records, &enums))
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
    ffi: &FfiContract,
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
            .all(|t| is_field_type_supported(ffi, t, records, enums)),
        VariantPayload::Struct(fields) => fields
            .iter()
            .all(|f| is_field_type_supported(ffi, &f.type_expr, records, enums)),
    })
}

/// Whether `ty` is a supported field/element type, given the current
/// admission state of records and enums. `Custom` resolves through to
/// its `repr` so the gate matches what the lowerer will normalize to.
fn is_field_type_supported(
    ffi: &FfiContract,
    ty: &TypeExpr,
    records: &HashSet<RecordId>,
    enums: &HashSet<EnumId>,
) -> bool {
    match ty {
        TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
        TypeExpr::Record(id) => records.contains(id),
        TypeExpr::Enum(id) => enums.contains(id),
        TypeExpr::Custom(id) => ffi
            .catalog
            .resolve_custom(id)
            .is_some_and(|custom| is_field_type_supported(ffi, &custom.repr, records, enums)),
        TypeExpr::Vec(inner) => is_field_type_supported(ffi, inner, records, enums),
        // C# models `Option<T>` as `T?`, so `Option<Option<T>>` would
        // need `T??`, which the language rejects and which can't be
        // flattened without losing the `Some(None)` state.
        TypeExpr::Option(inner) => {
            !matches!(inner.as_ref(), TypeExpr::Option(_))
                && is_field_type_supported(ffi, inner, records, enums)
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
    use crate::ir::definitions::{
        CStyleVariant, CustomTypeDef, FunctionDef, ParamDef, ParamPassing, ReturnDef,
    };
    use crate::ir::ids::{ConverterPath, CustomTypeId, FunctionId, ParamName, QualifiedName};
    use crate::ir::types::PrimitiveType;
    use boltffi_ffi_rules::callable::ExecutionKind;

    use super::super::super::CSharpOptions;

    fn datetime_custom_type() -> CustomTypeDef {
        CustomTypeDef {
            id: CustomTypeId::new("UtcDateTime"),
            rust_type: QualifiedName::new("chrono::DateTime<Utc>"),
            repr: TypeExpr::Primitive(PrimitiveType::I64),
            converters: ConverterPath {
                into_ffi: QualifiedName::new("test_into_ffi"),
                try_from_ffi: QualifiedName::new("test_try_from_ffi"),
            },
            doc: None,
        }
    }

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

    /// A record carrying a `Custom` field (here `UtcDateTime`, repr =
    /// i64) must admit. The lowerer normalizes `Custom` to `repr` before
    /// emitting, so the admission gate has to see through the wrapper or
    /// the entire record gets silently dropped — which is the pre-fix
    /// behavior issue #146 calls out.
    #[test]
    fn record_with_custom_field_is_admitted() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_custom(datetime_custom_type());
        contract.catalog.insert_record(record_with_one_field(
            "event",
            "timestamp",
            TypeExpr::Custom(CustomTypeId::new("UtcDateTime")),
        ));

        let (records, _enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            records.contains(&RecordId::new("event")),
            "expecting a record with a Custom<i64> field to be admitted",
        );
    }

    /// `Vec<Custom>` whose underlying repr is a primitive must take the
    /// blittable pinned-array fast path. The macro already classifies
    /// `Vec<UtcDateTime>` as `Vec<i64>` ABI-side; if `is_blittable_vec_element`
    /// didn't look through `Custom`, the C# side would mismatch by trying
    /// to wire-encode a buffer the macro emits as a direct `*const i64`.
    #[test]
    fn blittable_vec_element_resolves_through_custom() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_custom(datetime_custom_type());

        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let custom = TypeExpr::Custom(CustomTypeId::new("UtcDateTime"));
        assert!(
            lowerer.is_blittable_vec_element(&custom),
            "expecting Vec<Custom<i64>> to qualify for the pinned-array fast path",
        );
        assert!(
            lowerer.is_supported_type(&custom),
            "expecting bare Custom<i64> to admit as a param/return type",
        );
        assert!(
            lowerer.is_supported_vec_element(&custom),
            "expecting Custom<i64> to admit as a Vec element",
        );
    }
}
