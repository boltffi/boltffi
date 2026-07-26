use boltffi_ast::RecordDef as SourceRecord;

use crate::{
    ByteAlignment, ByteOffset, ByteSize, DirectFieldType, FieldKey, FieldLayout, Primitive,
    RecordLayout,
};

use super::{LowerError, error::UnsupportedType, primitive};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WideScalarAlignment {
    EightBytes,
    FourBytes,
}

impl WideScalarAlignment {
    const fn bytes(self) -> u64 {
        match self {
            Self::EightBytes => 8,
            Self::FourBytes => 4,
        }
    }
}

/// Computes the byte-level layout of a direct record.
///
/// Walks the fields in source order, advancing the running offset to
/// the alignment each primitive demands and tracking the largest
/// alignment seen as the record's own. The trailing size is rounded up
/// to that alignment so an array of these records lays out without
/// internal padding.
pub fn compute(record: &SourceRecord) -> Result<RecordLayout, LowerError> {
    compute_with_wide_scalar_alignment(record, WideScalarAlignment::EightBytes)
}

/// Reports whether every supported native ABI gives a record the same bytes.
///
/// Most native targets align `i64`, `u64`, and `f64` to eight bytes. The
/// 32-bit x86 ABI aligns those scalars to four bytes instead. Direct records
/// are safe only when both profiles agree on the total byte count and every
/// field offset. Their container alignment may differ: the IR retains the
/// larger alignment so generated allocators always over-align rather than
/// under-align storage.
pub fn has_portable_byte_layout(record: &SourceRecord) -> bool {
    let eight_byte_layout =
        compute_with_wide_scalar_alignment(record, WideScalarAlignment::EightBytes);
    let four_byte_layout =
        compute_with_wide_scalar_alignment(record, WideScalarAlignment::FourBytes);

    matches!(
        (eight_byte_layout, four_byte_layout),
        (Ok(eight_byte), Ok(four_byte))
            if eight_byte.size() == four_byte.size()
                && eight_byte.fields() == four_byte.fields()
    )
}

fn compute_with_wide_scalar_alignment(
    record: &SourceRecord,
    wide_scalar_alignment: WideScalarAlignment,
) -> Result<RecordLayout, LowerError> {
    let (offset, alignment, fields) = record.fields.iter().try_fold(
        (0_u64, 1_u64, Vec::new()),
        |(offset, alignment, mut fields), field| {
            let field_type = primitive::direct_field_type(&field.type_expr)
                .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::RecordField))?;
            let field_alignment = field_alignment(field_type, wide_scalar_alignment).get();
            let field_offset = align_up(offset, field_alignment);
            fields.push(FieldLayout::new(
                FieldKey::from(field),
                ByteOffset::new(field_offset),
            ));
            Ok::<_, LowerError>((
                field_offset + field_type.byte_size().get(),
                alignment.max(field_alignment),
                fields,
            ))
        },
    )?;
    let alignment = ByteAlignment::new(alignment)
        .map_err(|error| LowerError::invalid_alignment(error.bytes()))?;

    Ok(RecordLayout::new(
        ByteSize::new(align_up(offset, alignment.get())),
        alignment,
        fields,
    ))
}

fn field_alignment(
    field_type: DirectFieldType,
    wide_scalar_alignment: WideScalarAlignment,
) -> ByteAlignment {
    let bytes = match field_type.primitive() {
        Primitive::I64 | Primitive::U64 | Primitive::F64 => wide_scalar_alignment.bytes(),
        _ => field_type.byte_alignment().get(),
    };
    ByteAlignment::new(bytes).expect("direct field alignments are powers of two")
}

fn align_up(offset: u64, alignment: u64) -> u64 {
    (offset + alignment - 1) & !(alignment - 1)
}
