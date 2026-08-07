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
    let field_types = direct_field_types(record)?;
    let profile = profile(&field_types, WideScalarAlignment::EightBytes);
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

/// Reports whether every supported native ABI gives a record the same bytes.
///
/// Most native targets align `i64`, `u64`, and `f64` to eight bytes. The
/// 32-bit x86 ABI aligns those scalars to four bytes instead. Direct records
/// are safe only when both profiles agree on the total byte count and every
/// field offset. Their container alignment may differ: the IR retains the
/// larger alignment so generated allocators always over-align rather than
/// under-align storage.
pub fn has_portable_byte_layout(record: &SourceRecord) -> bool {
    direct_field_types(record).is_ok_and(|field_types| portable_direct_fields(&field_types))
}

/// Reports whether fields of these types get the same bytes on every
/// supported native ABI.
///
/// The profile-agreement half of the record classification rule; see
/// [`has_portable_byte_layout`] for the profiles compared.
pub(crate) fn portable_direct_fields(field_types: &[DirectFieldType]) -> bool {
    let natural = profile(field_types, WideScalarAlignment::EightBytes);
    let four_byte = profile(field_types, WideScalarAlignment::FourBytes);
    natural.size == four_byte.size && natural.offsets == four_byte.offsets
}

struct LayoutProfile {
    size: u64,
    alignment: u64,
    offsets: Vec<u64>,
}

fn direct_field_types(record: &SourceRecord) -> Result<Vec<DirectFieldType>, LowerError> {
    record
        .fields
        .iter()
        .map(|field| {
            primitive::direct_field_type(&field.type_expr)
                .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::RecordField))
        })
        .collect()
}

fn profile(
    field_types: &[DirectFieldType],
    wide_scalar_alignment: WideScalarAlignment,
) -> LayoutProfile {
    let mut offset = 0_u64;
    let mut alignment = 1_u64;
    let mut offsets = Vec::with_capacity(field_types.len());
    for field_type in field_types {
        let field_alignment = field_alignment(*field_type, wide_scalar_alignment).get();
        offset = align_up(offset, field_alignment);
        offsets.push(offset);
        offset += field_type.byte_size().get();
        alignment = alignment.max(field_alignment);
    }

    LayoutProfile {
        size: align_up(offset, alignment),
        alignment,
        offsets,
    }
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
