use crate::ir::definitions::{DataVariant, EnumDef, EnumRepr, FieldDef, RecordDef, VariantPayload};
use crate::ir::ids::{EnumId, RecordId};
use crate::ir::types::{PrimitiveType, TypeExpr};

pub(super) fn data_enum(id: &str, variants: Vec<DataVariant>) -> EnumDef {
    EnumDef {
        id: EnumId::new(id),
        repr: EnumRepr::Data {
            tag_type: PrimitiveType::I32,
            variants,
        },
        is_error: false,
        constructors: vec![],
        methods: vec![],
        doc: None,
        deprecated: None,
    }
}

pub(super) fn struct_variant(
    name: &str,
    discriminant: i128,
    fields: Vec<(&str, TypeExpr)>,
) -> DataVariant {
    DataVariant {
        name: name.into(),
        discriminant,
        payload: VariantPayload::Struct(
            fields
                .into_iter()
                .map(|(field_name, ty)| FieldDef {
                    name: field_name.into(),
                    type_expr: ty,
                    doc: None,
                    default: None,
                })
                .collect(),
        ),
        doc: None,
    }
}

pub(super) fn record_with_one_field(id: &str, field_name: &str, type_expr: TypeExpr) -> RecordDef {
    RecordDef {
        id: RecordId::new(id),
        is_repr_c: false,
        is_error: false,
        fields: vec![FieldDef {
            name: field_name.into(),
            type_expr,
            doc: None,
            default: None,
        }],
        constructors: vec![],
        methods: vec![],
        doc: None,
        deprecated: None,
    }
}
