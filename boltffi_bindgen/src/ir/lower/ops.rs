use super::*;

impl<'c> Lowerer<'c> {
    pub(super) fn expand_decode(&self, codec: &CodecPlan) -> ReadSeq {
        self.expand_decode_with_offset(codec, "pos")
    }

    // self is only used in recursive calls, records, enums, vecs all recurse back here,
    // but clippy does not see through the recursion and thinks it is unused.
    #[allow(clippy::only_used_in_recursion)]
    pub(super) fn expand_decode_with_offset(&self, codec: &CodecPlan, base: &str) -> ReadSeq {
        let offset = OffsetExpr::Base;
        match codec {
            CodecPlan::Void => ReadSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            },
            CodecPlan::Primitive(primitive) => ReadSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![ReadOp::Primitive {
                    primitive: *primitive,
                    offset,
                }],
                shape: WireShape::Value,
            },
            CodecPlan::String => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::String { offset }],
                shape: WireShape::Value,
            },
            CodecPlan::Builtin(id) => {
                let size = self
                    .builtin_fixed_size(id)
                    .map(SizeExpr::Fixed)
                    .unwrap_or(SizeExpr::Runtime);
                ReadSeq {
                    size,
                    ops: vec![ReadOp::Builtin {
                        id: id.clone(),
                        offset,
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Option(inner) => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Option {
                    tag_offset: offset,
                    some: Box::new(self.expand_decode_with_offset(inner, "pos")),
                }],
                shape: WireShape::Optional,
            },
            CodecPlan::Vec { element, layout } => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Vec {
                    len_offset: offset,
                    element_type: TypeExpr::from(element.as_ref()),
                    element: Box::new(self.expand_decode_with_offset(element, "pos")),
                    layout: layout.clone(),
                }],
                shape: WireShape::Sequence,
            },
            CodecPlan::Result { ok, err } => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Result {
                    tag_offset: offset,
                    ok: Box::new(self.expand_decode_with_offset(ok, "pos")),
                    err: Box::new(self.expand_decode_with_offset(err, "pos")),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Record { id, layout } => {
                let (fields, size) = match layout {
                    RecordLayout::Blittable { fields, size } => (
                        fields
                            .iter()
                            .map(|field| {
                                let offset_expr = if field.offset == 0 {
                                    OffsetExpr::Base
                                } else {
                                    OffsetExpr::BasePlus(field.offset)
                                };
                                FieldReadOp {
                                    name: field.name.clone(),
                                    seq: ReadSeq {
                                        size: SizeExpr::Fixed(field.primitive.wire_size_bytes()),
                                        ops: vec![ReadOp::Primitive {
                                            primitive: field.primitive,
                                            offset: offset_expr,
                                        }],
                                        shape: WireShape::Value,
                                    },
                                }
                            })
                            .collect(),
                        SizeExpr::Fixed(*size),
                    ),
                    RecordLayout::Encoded { fields } => (
                        fields
                            .iter()
                            .map(|field| FieldReadOp {
                                name: field.name.clone(),
                                seq: self.expand_decode_with_offset(&field.codec, "pos"),
                            })
                            .collect(),
                        SizeExpr::Runtime,
                    ),
                    RecordLayout::Recursive => (vec![], SizeExpr::Runtime),
                };
                ReadSeq {
                    size,
                    ops: vec![ReadOp::Record {
                        id: id.clone(),
                        offset,
                        fields,
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Enum { id, layout } => ReadSeq {
                size: match layout {
                    EnumLayout::CStyle { .. } => SizeExpr::Fixed(4),
                    EnumLayout::Data { .. } | EnumLayout::Recursive => SizeExpr::Runtime,
                },
                ops: vec![ReadOp::Enum {
                    id: id.clone(),
                    offset,
                    layout: layout.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Custom { id, underlying } => {
                let underlying_seq = self.expand_decode_with_offset(underlying, base);
                ReadSeq {
                    size: underlying_seq.size.clone(),
                    ops: vec![ReadOp::Custom {
                        id: id.clone(),
                        underlying: Box::new(underlying_seq),
                    }],
                    shape: WireShape::Value,
                }
            }
        }
    }

    pub(super) fn expand_encode(&self, codec: &CodecPlan, value: ValueExpr) -> WriteSeq {
        match codec {
            CodecPlan::Void => WriteSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            },
            CodecPlan::Primitive(primitive) => WriteSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![WriteOp::Primitive {
                    primitive: *primitive,
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::String => WriteSeq {
                size: SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::StringLen(value.clone())]),
                ops: vec![WriteOp::String {
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Builtin(id) => WriteSeq {
                size: self
                    .builtin_fixed_size(id)
                    .map(SizeExpr::Fixed)
                    .unwrap_or_else(|| {
                        if id.as_str() == "Url" {
                            SizeExpr::Sum(vec![
                                SizeExpr::Fixed(4),
                                SizeExpr::BuiltinSize {
                                    id: id.clone(),
                                    value: value.clone(),
                                },
                            ])
                        } else {
                            SizeExpr::WireSize {
                                value: value.clone(),
                                owner: None,
                            }
                        }
                    }),
                ops: vec![WriteOp::Builtin {
                    id: id.clone(),
                    value: value.clone(),
                }],
                shape: WireShape::Value,
            },
            CodecPlan::Option(inner) => {
                let inner_seq = self.expand_encode(inner, ValueExpr::Var("v".into()));
                WriteSeq {
                    size: SizeExpr::OptionSize {
                        value: value.clone(),
                        inner: Box::new(inner_seq.size.clone()),
                    },
                    ops: vec![WriteOp::Option {
                        value: value.clone(),
                        some: Box::new(inner_seq),
                    }],
                    shape: WireShape::Optional,
                }
            }
            CodecPlan::Vec { element, layout } => {
                let element_seq = self.expand_encode(element, ValueExpr::Var("item".into()));
                let size_expr =
                    if matches!(element.as_ref(), CodecPlan::Primitive(PrimitiveType::U8)) {
                        SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::BytesLen(value.clone())])
                    } else {
                        SizeExpr::VecSize {
                            value: value.clone(),
                            inner: Box::new(element_seq.size.clone()),
                            layout: layout.clone(),
                        }
                    };
                WriteSeq {
                    size: size_expr,
                    ops: vec![WriteOp::Vec {
                        value: value.clone(),
                        element_type: TypeExpr::from(element.as_ref()),
                        element: Box::new(element_seq),
                        layout: layout.clone(),
                    }],
                    shape: WireShape::Sequence,
                }
            }
            CodecPlan::Result { ok, err } => {
                let ok_seq = self.expand_encode(ok, ValueExpr::Var("okVal".into()));
                let err_seq = self.expand_encode(err, ValueExpr::Var("errVal".into()));
                WriteSeq {
                    size: SizeExpr::ResultSize {
                        value: value.clone(),
                        ok: Box::new(ok_seq.size.clone()),
                        err: Box::new(err_seq.size.clone()),
                    },
                    ops: vec![WriteOp::Result {
                        value: value.clone(),
                        ok: Box::new(ok_seq),
                        err: Box::new(err_seq),
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Record { id, layout } => {
                let fields = match layout {
                    RecordLayout::Blittable { fields, .. } => fields
                        .iter()
                        .map(|field| {
                            let field_value = value.field(field.name.clone());
                            FieldWriteOp {
                                name: field.name.clone(),
                                accessor: field_value.clone(),
                                seq: self.expand_encode(
                                    &CodecPlan::Primitive(field.primitive),
                                    field_value,
                                ),
                            }
                        })
                        .collect(),
                    RecordLayout::Encoded { fields } => fields
                        .iter()
                        .map(|field| {
                            let field_value = value.field(field.name.clone());
                            FieldWriteOp {
                                name: field.name.clone(),
                                accessor: field_value.clone(),
                                seq: self.expand_encode(&field.codec, field_value),
                            }
                        })
                        .collect(),
                    RecordLayout::Recursive => vec![],
                };
                let size = match layout {
                    RecordLayout::Blittable { size, .. } => SizeExpr::Fixed(*size),
                    _ => SizeExpr::WireSize {
                        value: value.clone(),
                        owner: Some(WireSizeOwner::Record(id.clone())),
                    },
                };
                WriteSeq {
                    size,
                    ops: vec![WriteOp::Record {
                        id: id.clone(),
                        value: value.clone(),
                        fields,
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Enum { id, layout } => {
                let size = match layout {
                    EnumLayout::CStyle { .. } => SizeExpr::Fixed(4),
                    _ => SizeExpr::WireSize {
                        value: value.clone(),
                        owner: Some(WireSizeOwner::Enum(id.clone())),
                    },
                };
                WriteSeq {
                    size,
                    ops: vec![WriteOp::Enum {
                        id: id.clone(),
                        value: value.clone(),
                        layout: layout.clone(),
                    }],
                    shape: WireShape::Value,
                }
            }
            CodecPlan::Custom { id, underlying } => {
                let underlying_seq = self.expand_encode(underlying, value.clone());
                WriteSeq {
                    size: underlying_seq.size.clone(),
                    ops: vec![WriteOp::Custom {
                        id: id.clone(),
                        value: value.clone(),
                        underlying: Box::new(underlying_seq),
                    }],
                    shape: WireShape::Value,
                }
            }
        }
    }

    pub(super) fn builtin_fixed_size(&self, id: &BuiltinId) -> Option<usize> {
        match id.as_str() {
            "Duration" | "SystemTime" => Some(12),
            "Uuid" => Some(16),
            _ => None,
        }
    }
}
