use super::{Alignment, CLayout, Enumeration, Layout, Offset, Primitive, Size, StructLayout};

#[derive(Debug, Clone)]
pub struct DataEnumLayout {
    struct_size: Size,
    payload_offset: Offset,
    variants: Vec<DataEnumVariantLayout>,
}

#[derive(Debug, Clone)]
struct DataEnumVariantLayout {
    field_offsets: Vec<Offset>,
    payload_layout: Layout,
}

impl DataEnumLayout {
    pub fn from_enum(enumeration: &Enumeration) -> Option<Self> {
        let tag_layout = Primitive::I32.c_layout();

        if enumeration.is_c_style() {
            let variants: Vec<DataEnumVariantLayout> = enumeration
                .variants
                .iter()
                .map(|_| DataEnumVariantLayout {
                    field_offsets: Vec::new(),
                    payload_layout: Layout::new(0, 1),
                })
                .collect();

            return Some(Self {
                struct_size: tag_layout.size,
                payload_offset: Offset::ZERO + tag_layout.size,
                variants,
            });
        }

        let variants: Vec<DataEnumVariantLayout> = enumeration
            .variants
            .iter()
            .map(|variant| {
                if variant.fields.is_empty() {
                    DataEnumVariantLayout {
                        field_offsets: Vec::new(),
                        payload_layout: Layout::new(0, 1),
                    }
                } else {
                    let struct_layout = StructLayout::from_layouts(variant.fields.iter().map(|field| field.field_type.c_layout()));

                    DataEnumVariantLayout {
                        field_offsets: struct_layout.offsets().collect(),
                        payload_layout: Layout {
                            size: struct_layout.total_size(),
                            alignment: struct_layout.alignment(),
                        },
                    }
                }
            })
            .collect();

        let union_alignment = variants
            .iter()
            .map(|variant| variant.payload_layout.alignment)
            .fold(Alignment::new(1), |current, next| current.max(next));

        let union_size_unpadded = variants.iter().map(|variant| variant.payload_layout.size.as_usize()).max().unwrap_or(0);

        let union_size = Size::new(union_size_unpadded).padded_to(union_alignment);
        let payload_offset = (Offset::ZERO + tag_layout.size).aligned_to(union_alignment);
        let struct_alignment = tag_layout.alignment.max(union_alignment);
        let struct_size = Size::new(payload_offset.as_usize() + union_size.as_usize()).padded_to(struct_alignment);

        Some(Self {
            struct_size,
            payload_offset,
            variants,
        })
    }

    pub fn struct_size(&self) -> Size {
        self.struct_size
    }

    pub fn payload_offset(&self) -> Offset {
        self.payload_offset
    }

    pub fn field_offset(&self, variant_index: usize, field_index: usize) -> Option<Offset> {
        self.variants.get(variant_index).and_then(|variant| variant.field_offsets.get(field_index)).copied()
    }
}
