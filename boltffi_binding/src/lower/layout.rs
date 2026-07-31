use boltffi_ast::{RecordDef as SourceRecord, TypeExpr};
use boltffi_ffi_rules::classification;

use crate::{ByteAlignment, ByteOffset, ByteSize, FieldKey, FieldLayout, RecordLayout};

use super::{LowerError, error::UnsupportedType, primitive};

/// Computes the byte-level layout of a direct record.
///
/// Delegates the offset walk to [`classification::layout_profile`] under the
/// natural alignment profile, so the layout emitted here cannot drift from
/// the one the shared classifier called blittable.
pub fn compute(record: &SourceRecord) -> Result<RecordLayout, LowerError> {
    let field_primitives = record
        .fields
        .iter()
        .map(|field| match &field.type_expr {
            TypeExpr::Primitive(field_primitive) => {
                Ok(primitive::classification_primitive(*field_primitive))
            }
            _ => Err(LowerError::unsupported_type(UnsupportedType::RecordField)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let profile = classification::layout_profile(&field_primitives, u64::MAX)
        .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::RecordField))?;
    let alignment = ByteAlignment::new(profile.alignment)
        .map_err(|error| LowerError::invalid_alignment(error.bytes()))?;
    let fields = record
        .fields
        .iter()
        .zip(&profile.offsets)
        .map(|(field, offset)| FieldLayout::new(FieldKey::from(field), ByteOffset::new(*offset)))
        .collect();

    Ok(RecordLayout::new(
        ByteSize::new(profile.size),
        alignment,
        fields,
    ))
}
