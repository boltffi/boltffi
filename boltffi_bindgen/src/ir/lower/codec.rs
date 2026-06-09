use super::*;

impl<'c> Lowerer<'c> {
    pub fn build_codec(&self, type_expr: &TypeExpr) -> CodecPlan {
        match type_expr {
            TypeExpr::Void => CodecPlan::Void,
            TypeExpr::Primitive(primitive) => CodecPlan::Primitive(*primitive),
            TypeExpr::String => CodecPlan::String,
            TypeExpr::Builtin(id) => CodecPlan::Builtin(id.clone()),
            TypeExpr::Option(inner) => CodecPlan::Option(Box::new(self.build_codec(inner))),
            TypeExpr::Vec(inner) => CodecPlan::Vec {
                element: Box::new(self.build_codec(inner)),
                layout: self.vec_layout(inner),
            },
            TypeExpr::Result { ok, err } => CodecPlan::Result {
                ok: Box::new(self.build_codec(ok)),
                err: Box::new(self.build_codec(err)),
            },
            TypeExpr::Record(id) => CodecPlan::Record {
                id: id.clone(),
                layout: self.record_layout(id),
            },
            TypeExpr::Enum(id) => CodecPlan::Enum {
                id: id.clone(),
                layout: self.enum_layout(id),
            },
            TypeExpr::Custom(id) => {
                let definition = self
                    .contract
                    .catalog
                    .resolve_custom(id)
                    .expect("custom type should be resolved");
                CodecPlan::Custom {
                    id: id.clone(),
                    underlying: Box::new(self.build_codec(&definition.repr)),
                }
            }
            TypeExpr::Handle(_) | TypeExpr::Callback(_) => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    pub(super) fn record_layout(&self, id: &RecordId) -> RecordLayout {
        if self.record_stack.borrow().contains(id) {
            return RecordLayout::Recursive;
        }

        self.record_stack.borrow_mut().insert(id.clone());

        let definition = self
            .contract
            .catalog
            .resolve_record(id)
            .expect("record should be resolved");

        let layout = if self.is_blittable_record(definition) {
            self.build_blittable_record_layout(definition)
        } else {
            self.build_encoded_record_layout(definition)
        };

        self.record_stack.borrow_mut().remove(id);
        layout
    }

    pub(super) fn is_blittable_record(&self, definition: &RecordDef) -> bool {
        let field_primitives: Vec<_> = definition
            .fields
            .iter()
            .filter_map(|field| match &field.type_expr {
                TypeExpr::Primitive(primitive) => Some(primitive.to_field_primitive()),
                _ => None,
            })
            .collect();
        let all_primitive = field_primitives.len() == definition.fields.len();
        let classify_fields = if all_primitive {
            &field_primitives[..]
        } else {
            &[]
        };
        matches!(
            classification::classify_struct(definition.is_repr_c, classify_fields),
            PassableCategory::Blittable,
        )
    }

    pub(super) fn build_blittable_record_layout(&self, definition: &RecordDef) -> RecordLayout {
        let (size, fields) = compute_blittable_layout(definition);
        RecordLayout::Blittable { size, fields }
    }

    pub(super) fn build_encoded_record_layout(&self, definition: &RecordDef) -> RecordLayout {
        let fields = definition
            .fields
            .iter()
            .map(|field| EncodedField {
                name: field.name.clone(),
                codec: self.build_codec(&field.type_expr),
            })
            .collect();

        RecordLayout::Encoded { fields }
    }

    pub(super) fn enum_layout(&self, id: &EnumId) -> EnumLayout {
        if self.enum_stack.borrow().contains(id) {
            return EnumLayout::Recursive;
        }

        self.enum_stack.borrow_mut().insert(id.clone());

        let definition = self
            .contract
            .catalog
            .resolve_enum(id)
            .expect("enum should be resolved");

        let layout = match &definition.repr {
            EnumRepr::CStyle { tag_type, .. } => EnumLayout::CStyle {
                tag_type: *tag_type,
                tag_strategy: EnumTagStrategy::OrdinalIndex,
                is_error: definition.is_error,
            },
            EnumRepr::Data { tag_type, variants } => EnumLayout::Data {
                tag_type: *tag_type,
                tag_strategy: EnumTagStrategy::OrdinalIndex,
                variants: variants
                    .iter()
                    .map(|variant| VariantLayout {
                        name: variant.name.clone(),
                        discriminant: variant.discriminant,
                        payload: self.variant_payload_layout(&variant.payload),
                    })
                    .collect(),
            },
        };

        self.enum_stack.borrow_mut().remove(id);
        layout
    }

    pub(super) fn variant_payload_layout(&self, payload: &VariantPayload) -> VariantPayloadLayout {
        match payload {
            VariantPayload::Unit => VariantPayloadLayout::Unit,
            VariantPayload::Tuple(types) => VariantPayloadLayout::Fields(
                types
                    .iter()
                    .enumerate()
                    .map(|(index, type_expr)| EncodedField {
                        name: FieldName::new(format!("value_{}", index)),
                        codec: self.build_codec(type_expr),
                    })
                    .collect(),
            ),
            VariantPayload::Struct(fields) => VariantPayloadLayout::Fields(
                fields
                    .iter()
                    .map(|field| EncodedField {
                        name: field.name.clone(),
                        codec: self.build_codec(&field.type_expr),
                    })
                    .collect(),
            ),
        }
    }

    pub(super) fn vec_layout(&self, element: &TypeExpr) -> VecLayout {
        match element {
            TypeExpr::Primitive(primitive) => match primitive.size_bytes() {
                Some(size) => VecLayout::Blittable { element_size: size },
                None => VecLayout::Encoded,
            },
            TypeExpr::Record(id) => match self.contract.catalog.resolve_record(id) {
                Some(definition) if self.is_blittable_record(definition) => VecLayout::Blittable {
                    element_size: self.blittable_record_size(definition),
                },
                _ => VecLayout::Encoded,
            },
            _ => VecLayout::Encoded,
        }
    }

    pub(super) fn blittable_record_size(&self, definition: &RecordDef) -> usize {
        let (size, _) = compute_blittable_layout(definition);
        size
    }
}

fn align_up(offset: usize, alignment: usize) -> usize {
    (offset + alignment - 1) & !(alignment - 1)
}

pub(super) fn compute_blittable_layout(definition: &RecordDef) -> (usize, Vec<BlittableField>) {
    let (final_offset, fields) =
        definition
            .fields
            .iter()
            .fold((0usize, Vec::new()), |(offset, mut fields), field| {
                let TypeExpr::Primitive(primitive) = &field.type_expr else {
                    panic!("blittable record should only have primitive fields");
                };

                let alignment = primitive
                    .alignment()
                    .expect("blittable field must have fixed-size alignment");
                let size = primitive
                    .size_bytes()
                    .expect("blittable field must have fixed size");
                let aligned_offset = align_up(offset, alignment);

                fields.push(BlittableField {
                    name: field.name.clone(),
                    offset: aligned_offset,
                    primitive: *primitive,
                });

                (aligned_offset + size, fields)
            });

    let max_align = definition
        .fields
        .iter()
        .filter_map(|field| match &field.type_expr {
            TypeExpr::Primitive(primitive) => primitive.alignment(),
            _ => None,
        })
        .max()
        .unwrap_or(1);

    let size = align_up(final_offset, max_align);
    (size, fields)
}
