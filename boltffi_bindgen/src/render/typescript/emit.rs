use boltffi_ffi_rules::naming::snake_to_camel as camel_case;

use crate::ir::codec::VecLayout;
use crate::ir::ids::BuiltinId;
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, ValueExpr, WireSizeOwner, WriteOp, WriteSeq};
use crate::ir::plan::CompositeLayout;
use crate::ir::types::{PrimitiveType, TypeExpr};

fn dataview_get(primitive: PrimitiveType, offset_expr: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("v.getUint8({offset_expr}) !== 0"),
        PrimitiveType::I8 => format!("v.getInt8({offset_expr})"),
        PrimitiveType::U8 => format!("v.getUint8({offset_expr})"),
        PrimitiveType::I16 => format!("v.getInt16({offset_expr}, true)"),
        PrimitiveType::U16 => format!("v.getUint16({offset_expr}, true)"),
        PrimitiveType::I32 => format!("v.getInt32({offset_expr}, true)"),
        PrimitiveType::U32 => format!("v.getUint32({offset_expr}, true)"),
        PrimitiveType::I64 | PrimitiveType::ISize => format!("v.getBigInt64({offset_expr}, true)"),
        PrimitiveType::U64 | PrimitiveType::USize => format!("v.getBigUint64({offset_expr}, true)"),
        PrimitiveType::F32 => format!("v.getFloat32({offset_expr}, true)"),
        PrimitiveType::F64 => format!("v.getFloat64({offset_expr}, true)"),
    }
}

pub fn composite_slot_decode_expr(layout: &CompositeLayout) -> String {
    let stride = layout.total_size;
    let fields: Vec<String> = layout
        .fields
        .iter()
        .map(|f| {
            let name = camel_case(f.name.as_str());
            let read = dataview_get(f.primitive, &format!("o + {}", f.offset));
            format!("{name}: {read}")
        })
        .collect();
    let obj = fields.join(", ");
    format!("_module.takeSlotStructArray({stride}, (v, o) => ({{ {obj} }}))")
}

pub fn composite_buf_decode_expr(layout: &CompositeLayout) -> String {
    let stride = layout.total_size;
    let fields: Vec<String> = layout
        .fields
        .iter()
        .map(|f| {
            let name = camel_case(f.name.as_str());
            let read = dataview_get(f.primitive, &format!("o + {}", f.offset));
            format!("{name}: {read}")
        })
        .collect();
    let obj = fields.join(", ");
    format!("_module.takeBufStructArray(outPtr, {stride}, (v, o) => ({{ {obj} }}))")
}

pub fn composite_value_decode_expr(layout: &CompositeLayout, ptr_name: &str) -> String {
    let fields: Vec<String> = layout
        .fields
        .iter()
        .map(|field| {
            let name = camel_case(field.name.as_str());
            let read = dataview_get(field.primitive, &field.offset.to_string());
            format!("{name}: {read}")
        })
        .collect();
    let obj = fields.join(", ");
    format!(
        "(() => {{ const bytes = _module.readFromMemory({ptr_name}, {}); const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength); return {{ {obj} }}; }})()",
        layout.total_size
    )
}

pub fn primitive_vec_read_expr(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "reader.readArray(() => reader.readBool())".into(),
        PrimitiveType::I8 => "reader.readI8Array()".into(),
        PrimitiveType::U8 => "reader.readBytes()".into(),
        PrimitiveType::I16 => "reader.readI16Array()".into(),
        PrimitiveType::U16 => "reader.readU16Array()".into(),
        PrimitiveType::I32 => "reader.readI32Array()".into(),
        PrimitiveType::U32 => "reader.readU32Array()".into(),
        PrimitiveType::I64 => "reader.readI64Array()".into(),
        PrimitiveType::U64 => "reader.readU64Array()".into(),
        PrimitiveType::ISize => "reader.readI64Array()".into(),
        PrimitiveType::USize => "reader.readU64Array()".into(),
        PrimitiveType::F32 => "reader.readF32Array()".into(),
        PrimitiveType::F64 => "reader.readF64Array()".into(),
    }
}

const TS_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "type",
];

pub fn escape_ts_keyword(name: &str) -> String {
    if TS_KEYWORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

pub fn ts_type(type_expr: &TypeExpr) -> String {
    match type_expr {
        TypeExpr::Void => "void".to_string(),
        TypeExpr::Primitive(p) => ts_primitive(*p),
        TypeExpr::String => "string".to_string(),
        TypeExpr::Builtin(id) => ts_builtin(id),
        TypeExpr::Option(inner) => format!("{} | null", ts_type(inner)),
        TypeExpr::Vec(inner) => {
            if matches!(inner.as_ref(), TypeExpr::Primitive(PrimitiveType::U8)) {
                "Uint8Array".to_string()
            } else {
                format!("{}[]", ts_type(inner))
            }
        }
        TypeExpr::Result { ok, .. } => ts_type(ok),
        TypeExpr::Record(id) => to_pascal_case(id.as_str()),
        TypeExpr::Enum(id) => to_pascal_case(id.as_str()),
        TypeExpr::Custom(id) => to_pascal_case(id.as_str()),
        TypeExpr::Handle(id) => to_pascal_case(id.as_str()),
        TypeExpr::Callback(id) => to_pascal_case(id.as_str()),
    }
}

pub fn ts_primitive(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "boolean",
        PrimitiveType::I8 | PrimitiveType::U8 => "number",
        PrimitiveType::I16 | PrimitiveType::U16 => "number",
        PrimitiveType::I32 | PrimitiveType::U32 => "number",
        PrimitiveType::I64 | PrimitiveType::U64 => "bigint",
        PrimitiveType::ISize | PrimitiveType::USize => "number",
        PrimitiveType::F32 | PrimitiveType::F64 => "number",
    }
    .to_string()
}

pub fn ts_builtin(id: &BuiltinId) -> String {
    match id.as_str() {
        "Duration" => "Duration",
        "SystemTime" => "Date",
        "Uuid" => "string",
        "Url" => "string",
        other => other,
    }
    .to_string()
}

fn render_value(expr: &ValueExpr, root_value: &str) -> String {
    match expr {
        ValueExpr::Instance => root_value.to_string(),
        ValueExpr::Var(name) if name == "value" => root_value.to_string(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => camel_case(name),
        ValueExpr::Field(parent, field) => {
            format!(
                "{}.{}",
                render_value(parent, root_value),
                camel_case(field.as_str())
            )
        }
    }
}

pub fn emit_reader_read(seq: &ReadSeq) -> String {
    seq.ops.first().map(emit_reader_read_op).unwrap_or_default()
}

pub fn emit_raw_primitive_array_read(prim: PrimitiveType) -> String {
    let method = match prim {
        PrimitiveType::I8 => "takePackedI8Array",
        PrimitiveType::U8 => "takePackedU8Array",
        PrimitiveType::I16 => "takePackedI16Array",
        PrimitiveType::U16 => "takePackedU16Array",
        PrimitiveType::I32 => "takePackedI32Array",
        PrimitiveType::U32 => "takePackedU32Array",
        PrimitiveType::I64 => "takePackedI64Array",
        PrimitiveType::U64 => "takePackedU64Array",
        PrimitiveType::ISize => "takePackedI64Array",
        PrimitiveType::USize => "takePackedU64Array",
        PrimitiveType::F32 => "takePackedF32Array",
        PrimitiveType::F64 => "takePackedF64Array",
        PrimitiveType::Bool => return "reader.readArray(() => reader.readBool())".into(),
    };
    format!("_module.{method}(packed)")
}

fn emit_reader_read_op(op: &ReadOp) -> String {
    match op {
        ReadOp::Primitive { primitive, .. } => match primitive {
            PrimitiveType::Bool => "reader.readBool()".into(),
            PrimitiveType::I8 => "reader.readI8()".into(),
            PrimitiveType::U8 => "reader.readU8()".into(),
            PrimitiveType::I16 => "reader.readI16()".into(),
            PrimitiveType::U16 => "reader.readU16()".into(),
            PrimitiveType::I32 => "reader.readI32()".into(),
            PrimitiveType::U32 => "reader.readU32()".into(),
            PrimitiveType::I64 => "reader.readI64()".into(),
            PrimitiveType::U64 => "reader.readU64()".into(),
            PrimitiveType::ISize => "reader.readISize()".into(),
            PrimitiveType::USize => "reader.readUSize()".into(),
            PrimitiveType::F32 => "reader.readF32()".into(),
            PrimitiveType::F64 => "reader.readF64()".into(),
        },
        ReadOp::String { .. } => "reader.readString()".into(),
        ReadOp::Builtin { id, .. } => match id.as_str() {
            "Duration" => "reader.readDuration()".into(),
            "SystemTime" => "reader.readTimestamp()".into(),
            "Uuid" => "reader.readUuid()".into(),
            "Url" => "reader.readUrl()".into(),
            other => format!("reader.read{}()", to_pascal_case(other)),
        },
        ReadOp::Option { some, .. } => {
            let inner = emit_reader_read(some);
            format!("reader.readOptional(() => {inner})")
        }
        ReadOp::Vec {
            element_type,
            element,
            ..
        } => match element_type {
            TypeExpr::Primitive(prim) => primitive_vec_read_expr(*prim),
            _ => {
                let inner = emit_reader_read(element);
                format!("reader.readArray(() => {inner})")
            }
        },
        ReadOp::Record { id, .. } => {
            format!("{}Codec.decode(reader)", to_pascal_case(id.as_str()))
        }
        ReadOp::Enum { id, .. } => {
            format!("{}Codec.decode(reader)", to_pascal_case(id.as_str()))
        }
        ReadOp::Result { ok, err, .. } => {
            let ok_read = emit_reader_read(ok);
            let err_read = emit_reader_read(err);
            let wrapped_err = wrap_error_in_exception(err, &err_read);
            format!("reader.readResult(() => {ok_read}, () => {wrapped_err})")
        }
        ReadOp::Custom { underlying, .. } => emit_reader_read(underlying),
    }
}

pub fn emit_result_void_ok_reader_read(err: &ReadSeq) -> String {
    let err_read = emit_reader_read(err);
    let wrapped_err = wrap_error_in_exception(err, &err_read);
    format!("reader.readResult(() => undefined, () => {wrapped_err})")
}

fn wrap_error_in_exception(err_seq: &ReadSeq, err_read: &str) -> String {
    err_seq
        .ops
        .first()
        .map(|op| match op {
            ReadOp::Enum { id, .. } => {
                let exception_name = format!("{}Exception", to_pascal_case(id.as_str()));
                format!("new {exception_name}({err_read})")
            }
            ReadOp::Record { id, .. } => {
                let exception_name = format!("{}Exception", to_pascal_case(id.as_str()));
                format!("new {exception_name}({err_read})")
            }
            ReadOp::String { .. } => {
                format!("new Error({err_read})")
            }
            _ => format!("new Error(String({err_read}))"),
        })
        .unwrap_or_else(|| format!("new Error(String({err_read}))"))
}

pub fn emit_writer_write(seq: &WriteSeq, writer: &str, value: &str) -> String {
    seq.ops
        .iter()
        .map(|op| emit_writer_write_op(op, writer, value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn emit_writer_write_op(op: &WriteOp, w: &str, root_value: &str) -> String {
    match op {
        WriteOp::Primitive { primitive, value } => {
            let val = render_value(value, root_value);
            match primitive {
                PrimitiveType::Bool => format!("{w}.writeBool({val})"),
                PrimitiveType::I8 => format!("{w}.writeI8({val})"),
                PrimitiveType::U8 => format!("{w}.writeU8({val})"),
                PrimitiveType::I16 => format!("{w}.writeI16({val})"),
                PrimitiveType::U16 => format!("{w}.writeU16({val})"),
                PrimitiveType::I32 => format!("{w}.writeI32({val})"),
                PrimitiveType::U32 => format!("{w}.writeU32({val})"),
                PrimitiveType::I64 => format!("{w}.writeI64({val})"),
                PrimitiveType::U64 => format!("{w}.writeU64({val})"),
                PrimitiveType::ISize => format!("{w}.writeISize({val})"),
                PrimitiveType::USize => format!("{w}.writeUSize({val})"),
                PrimitiveType::F32 => format!("{w}.writeF32({val})"),
                PrimitiveType::F64 => format!("{w}.writeF64({val})"),
            }
        }
        WriteOp::String { value } => {
            format!("{w}.writeString({})", render_value(value, root_value))
        }
        WriteOp::Builtin { id, value } => {
            let val = render_value(value, root_value);
            match id.as_str() {
                "Duration" => format!("{w}.writeDuration({val})"),
                "SystemTime" => format!("{w}.writeTimestamp({val})"),
                "Uuid" => format!("{w}.writeUuid({val})"),
                "Url" => format!("{w}.writeString({val})"),
                _ => format!("encode{}({w}, {val})", to_pascal_case(id.as_str())),
            }
        }
        WriteOp::Option { value, some } => {
            let inner = emit_writer_write(some, w, root_value);
            format!(
                "{w}.writeOptional({}, (v) => {{ {inner} }})",
                render_value(value, root_value),
            )
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            ..
        } => {
            let val = render_value(value, root_value);
            if matches!(element_type, TypeExpr::Primitive(PrimitiveType::U8)) {
                return format!("{w}.writeBytes({val})");
            }
            let inner = emit_writer_write(element, w, "item");
            format!("{w}.writeArray({val}, (item) => {{ {inner} }})")
        }
        WriteOp::Record { id, value, .. } => {
            format!(
                "{}Codec.encode({w}, {})",
                to_pascal_case(id.as_str()),
                render_value(value, root_value)
            )
        }
        WriteOp::Enum { id, value, .. } => {
            format!(
                "{}Codec.encode({w}, {})",
                to_pascal_case(id.as_str()),
                render_value(value, root_value)
            )
        }
        WriteOp::Result { value, ok, err } => {
            let val = render_value(value, root_value);
            let ok_write = emit_writer_write(ok, w, root_value);
            let err_write = emit_writer_write(err, w, root_value);
            format!(
                "{w}.writeResult({val}, (okVal) => {{ {ok_write} }}, (errVal) => {{ {err_write} }})"
            )
        }
        WriteOp::Custom { underlying, .. } => emit_writer_write(underlying, w, root_value),
    }
}

pub fn emit_size_expr(size: &SizeExpr, root_value: &str) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::Runtime => "0".to_string(),
        SizeExpr::StringLen(value) => {
            format!("wireStringSize({})", render_value(value, root_value))
        }
        SizeExpr::BytesLen(value) => {
            format!("(4 + {}.byteLength)", render_value(value, root_value))
        }
        SizeExpr::ValueSize(expr) => render_value(expr, root_value),
        SizeExpr::WireSize { value, owner } => {
            let val = render_value(value, root_value);
            match owner {
                Some(WireSizeOwner::Record(id)) => {
                    format!("{}Codec.size({})", to_pascal_case(id.as_str()), val)
                }
                Some(WireSizeOwner::Enum(id)) => {
                    format!("{}Codec.size({})", to_pascal_case(id.as_str()), val)
                }
                None => format!("wireSize({})", val),
            }
        }
        SizeExpr::BuiltinSize { id, value } => {
            let val = render_value(value, root_value);
            match id.as_str() {
                "Url" => format!("wireStringSize({val})"),
                "Duration" | "SystemTime" => "12".to_string(),
                "Uuid" => "16".to_string(),
                _ => format!("wireSize({val})"),
            }
        }
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(|p| emit_size_expr(p, root_value))
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({rendered})")
        }
        SizeExpr::OptionSize { value, inner } => {
            let option_value = render_value(value, root_value);
            let payload_size = emit_size_expr(
                &remap_size_root(inner, ValueExpr::Var("value".into())),
                &option_value,
            );
            format!("({option_value} !== null ? 1 + {payload_size} : 1)")
        }
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => {
            let val = render_value(value, root_value);
            match layout {
                VecLayout::Blittable { element_size } => {
                    format!("(4 + {val}.length * {element_size})")
                }
                VecLayout::Encoded => {
                    let inner_size = emit_size_expr(inner, "item");
                    format!("(4 + {val}.reduce((acc, item) => acc + {inner_size}, 0))")
                }
            }
        }
        SizeExpr::ResultSize { value, ok, err } => {
            let val = render_value(value, root_value);
            let ok_payload = format!(
                "((typeof {val} === \"object\" && {val} !== null && (\"tag\" in ({val} as any)) && ({val} as any).tag === \"ok\") ? ({val} as any).value : {val})"
            );
            let err_payload = format!("({val} instanceof Error ? {val} : ({val} as any).error)");
            let ok_size = emit_size_expr(
                &remap_size_root(ok, ValueExpr::Var("value".into())),
                &ok_payload,
            );
            let err_size = emit_size_expr(
                &remap_size_root(err, ValueExpr::Var("value".into())),
                &err_payload,
            );
            format!(
                "(1 + (((typeof {val} === \"object\" && {val} !== null && (\"tag\" in ({val} as any)) && ({val} as any).tag === \"err\") || {val} instanceof Error) ? {err_size} : {ok_size}))"
            )
        }
    }
}

fn remap_size_root(size: &SizeExpr, new_root: ValueExpr) -> SizeExpr {
    match size {
        SizeExpr::Fixed(value) => SizeExpr::Fixed(*value),
        SizeExpr::Runtime => SizeExpr::Runtime,
        SizeExpr::StringLen(value) => SizeExpr::StringLen(value.remap_root(new_root)),
        SizeExpr::BytesLen(value) => SizeExpr::BytesLen(value.remap_root(new_root)),
        SizeExpr::ValueSize(value) => SizeExpr::ValueSize(value.remap_root(new_root)),
        SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
            value: value.remap_root(new_root),
            owner: owner.clone(),
        },
        SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
            id: id.clone(),
            value: value.remap_root(new_root),
        },
        SizeExpr::Sum(parts) => SizeExpr::Sum(
            parts
                .iter()
                .map(|part| remap_size_root(part, new_root.clone()))
                .collect(),
        ),
        SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
            value: value.remap_root(new_root.clone()),
            inner: Box::new(remap_size_root(inner, new_root)),
        },
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => SizeExpr::VecSize {
            value: value.remap_root(new_root.clone()),
            inner: Box::new(remap_size_root(inner, new_root)),
            layout: layout.clone(),
        },
        SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
            value: value.remap_root(new_root.clone()),
            ok: Box::new(remap_size_root(ok, new_root.clone())),
            err: Box::new(remap_size_root(err, new_root)),
        },
    }
}

fn to_pascal_case(name: &str) -> String {
    boltffi_ffi_rules::naming::to_upper_camel_case(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ops::ValueExpr;

    #[test]
    fn option_size_uses_optional_payload_binding() {
        let rendered = emit_size_expr(
            &SizeExpr::OptionSize {
                value: ValueExpr::Var("value".into()),
                inner: Box::new(SizeExpr::Sum(vec![
                    SizeExpr::Fixed(4),
                    SizeExpr::StringLen(ValueExpr::Var("v".into())),
                ])),
            },
            "email",
        );

        assert_eq!(
            rendered,
            "(email !== null ? 1 + (4 + wireStringSize(email)) : 1)"
        );
    }

    #[test]
    fn result_size_uses_result_branch_bindings() {
        let rendered = emit_size_expr(
            &SizeExpr::ResultSize {
                value: ValueExpr::Var("value".into()),
                ok: Box::new(SizeExpr::Fixed(4)),
                err: Box::new(SizeExpr::Sum(vec![
                    SizeExpr::Fixed(4),
                    SizeExpr::StringLen(ValueExpr::Var("errVal".into())),
                ])),
            },
            "result",
        );

        assert_eq!(
            rendered,
            "(1 + (((typeof result === \"object\" && result !== null && (\"tag\" in (result as any)) && (result as any).tag === \"err\") || result instanceof Error) ? (4 + wireStringSize((result instanceof Error ? result : (result as any).error))) : 4))"
        );
    }
}
