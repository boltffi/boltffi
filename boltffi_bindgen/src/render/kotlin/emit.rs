use boltffi_ffi_rules::transport::EnumTagStrategy;

use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ids::BuiltinId;
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::render::kotlin::NamingConvention;

pub fn render_value(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::Instance => String::new(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => NamingConvention::property_name(name),
        ValueExpr::Field(parent, field) => {
            let parent_str = render_value(parent);
            let field_str = NamingConvention::property_name(field.as_str());
            if parent_str.is_empty() {
                field_str
            } else {
                format!("{}.{}", parent_str, field_str)
            }
        }
    }
}

fn render_type_name(name: &str) -> String {
    NamingConvention::class_name(name)
}

fn kotlin_type_for_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Primitive(p) => match p {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 => "Byte".to_string(),
            PrimitiveType::U8 => "UByte".to_string(),
            PrimitiveType::I16 => "Short".to_string(),
            PrimitiveType::U16 => "UShort".to_string(),
            PrimitiveType::I32 => "Int".to_string(),
            PrimitiveType::U32 => "UInt".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "Long".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "ULong".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        },
        TypeExpr::String => "String".to_string(),
        TypeExpr::Bytes => "ByteArray".to_string(),
        TypeExpr::Vec(inner) => match inner.as_ref() {
            TypeExpr::Primitive(primitive) => match primitive {
                PrimitiveType::I32 | PrimitiveType::U32 => "IntArray".to_string(),
                PrimitiveType::I16 | PrimitiveType::U16 => "ShortArray".to_string(),
                PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize => "LongArray".to_string(),
                PrimitiveType::F32 => "FloatArray".to_string(),
                PrimitiveType::F64 => "DoubleArray".to_string(),
                PrimitiveType::U8 | PrimitiveType::I8 => "ByteArray".to_string(),
                PrimitiveType::Bool => "BooleanArray".to_string(),
            },
            _ => format!("List<{}>", kotlin_type_for_type_expr(inner)),
        },
        TypeExpr::Option(inner) => format!("{}?", kotlin_type_for_type_expr(inner)),
        TypeExpr::Result { ok, err } => format!(
            "BoltFFIResult<{}, {}>",
            kotlin_type_for_type_expr(ok),
            kotlin_type_for_type_expr(err)
        ),
        TypeExpr::Record(id) => render_type_name(id.as_str()),
        TypeExpr::Enum(id) => render_type_name(id.as_str()),
        TypeExpr::Custom(id) => render_type_name(id.as_str()),
        TypeExpr::Builtin(id) => match id.as_str() {
            "Duration" => "Duration".to_string(),
            "SystemTime" => "Instant".to_string(),
            "Uuid" => "UUID".to_string(),
            "Url" => "URI".to_string(),
            _ => "String".to_string(),
        },
        TypeExpr::Handle(class_id) => render_type_name(class_id.as_str()),
        TypeExpr::Callback(callback_id) => render_type_name(callback_id.as_str()),
        TypeExpr::Void => "Unit".to_string(),
    }
}

fn kotlin_type_for_write_seq(seq: &WriteSeq) -> String {
    match seq.ops.first() {
        Some(WriteOp::Primitive { primitive, .. }) => {
            kotlin_type_for_type_expr(&TypeExpr::Primitive(*primitive))
        }
        Some(WriteOp::String { .. }) => "String".to_string(),
        Some(WriteOp::Bytes { .. }) => "ByteArray".to_string(),
        Some(WriteOp::Builtin { id, .. }) => {
            kotlin_type_for_type_expr(&TypeExpr::Builtin(id.clone()))
        }
        Some(WriteOp::Record { id, .. }) => render_type_name(id.as_str()),
        Some(WriteOp::Enum { id, .. }) => render_type_name(id.as_str()),
        Some(WriteOp::Custom { id, .. }) => render_type_name(id.as_str()),
        Some(WriteOp::Vec { element_type, .. }) => {
            kotlin_type_for_type_expr(&TypeExpr::Vec(Box::new(element_type.clone())))
        }
        Some(WriteOp::Option { some, .. }) => format!("{}?", kotlin_type_for_write_seq(some)),
        Some(WriteOp::Result { ok, err, .. }) => format!(
            "BoltFFIResult<{}, {}>",
            kotlin_type_for_write_seq(ok),
            kotlin_type_for_write_seq(err)
        ),
        _ => "Any".to_string(),
    }
}

pub fn emit_size_expr(size: &SizeExpr) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::Runtime => "0".to_string(),
        SizeExpr::StringLen(value) => format!("Utf8Codec.maxBytes({})", render_value(value)),
        SizeExpr::BytesLen(value) => format!("{}.size", render_value(value)),
        SizeExpr::ValueSize(value) => render_value(value),
        SizeExpr::WireSize { value, .. } => format!("{}.wireEncodedSize()", render_value(value)),
        SizeExpr::BuiltinSize { id, value } => emit_builtin_size(id, &render_value(value)),
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(emit_size_expr)
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({})", rendered)
        }
        SizeExpr::OptionSize { value, inner } => {
            let inner_expr = emit_size_expr(inner);
            format!(
                "({}?.let {{ v -> 1 + {} }} ?: 1)",
                render_value(value),
                inner_expr
            )
        }
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => emit_vec_size(&render_value(value), inner, layout),
        SizeExpr::ResultSize { value, ok, err } => {
            let v = render_value(value);
            let ok_expr = emit_size_expr(ok);
            let err_expr = emit_size_expr(err);
            format!(
                "(when (val _r = {}) {{ is BoltFFIResult.Ok<*> -> {{ val okVal = _r.value; 1 + {} }}; is BoltFFIResult.Err<*> -> {{ val errVal = _r.error; 1 + {} }} }})",
                v, ok_expr, err_expr
            )
        }
    }
}

pub fn emit_size_expr_for_write_seq(seq: &WriteSeq) -> String {
    match seq.ops.first() {
        Some(WriteOp::Custom { underlying, .. }) => emit_size_expr(&underlying.size),
        Some(WriteOp::Result { ok, err, .. }) => {
            let ok_type = kotlin_type_for_write_seq(ok);
            let err_type = kotlin_type_for_write_seq(err);
            match &seq.size {
                SizeExpr::ResultSize { value, ok, err } => {
                    let v = render_value(value);
                    let ok_expr = emit_size_expr(ok);
                    let err_expr = emit_size_expr(err);
                    format!(
                        "(when (val _r = {}) {{ is BoltFFIResult.Ok<*> -> {{ val okVal = boltffiUnsafeCast<{}>(_r.value); 1 + {} }}; is BoltFFIResult.Err<*> -> {{ val errVal = boltffiUnsafeCast<{}>(_r.error); 1 + {} }} }})",
                        v, ok_type, ok_expr, err_type, err_expr
                    )
                }
                _ => emit_size_expr(&seq.size),
            }
        }
        _ => emit_size_expr(&seq.size),
    }
}

pub fn emit_reader_read(seq: &ReadSeq) -> String {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => {
            let method = match primitive {
                PrimitiveType::Bool => "readBool",
                PrimitiveType::I8 => "readI8",
                PrimitiveType::U8 => "readU8",
                PrimitiveType::I16 => "readI16",
                PrimitiveType::U16 => "readU16",
                PrimitiveType::I32 => "readI32",
                PrimitiveType::U32 => "readU32",
                PrimitiveType::I64 | PrimitiveType::ISize => "readI64",
                PrimitiveType::U64 | PrimitiveType::USize => "readU64",
                PrimitiveType::F32 => "readF32",
                PrimitiveType::F64 => "readF64",
            };
            format!("reader.{}()", method)
        }
        ReadOp::String { .. } => "reader.readString()".to_string(),
        ReadOp::Bytes { .. } => "reader.readBytes()".to_string(),
        ReadOp::Record { id, .. } => {
            format!("{}.decode(reader)", render_type_name(id.as_str()))
        }
        ReadOp::Enum { id, layout, .. } => match layout {
            EnumLayout::CStyle {
                tag_type,
                tag_strategy,
                ..
            } => match tag_strategy {
                EnumTagStrategy::Discriminant => {
                    format!(
                        "{}.fromValue(reader.{}())",
                        render_type_name(id.as_str()),
                        enum_tag_read_method(*tag_type),
                    )
                }
                EnumTagStrategy::OrdinalIndex => {
                    format!("{}.decode(reader)", render_type_name(id.as_str()))
                }
            },
            EnumLayout::Data { .. } | EnumLayout::Recursive => {
                format!("{}.decode(reader)", render_type_name(id.as_str()))
            }
        },
        ReadOp::Option { some, .. } => {
            let inner = emit_reader_read(some);
            format!("reader.readOptional {{ {} }}", inner)
        }
        ReadOp::Vec {
            element_type,
            element,
            layout,
            ..
        } => emit_reader_vec(element_type, element, layout),
        ReadOp::Result { ok, err, .. } => {
            let ok_expr = emit_reader_read(ok);
            let err_expr = emit_reader_read(err);
            format!("reader.readResult({{ {} }}, {{ {} }})", ok_expr, err_expr)
        }
        ReadOp::Builtin { id, .. } => match id.as_str() {
            "Duration" => "reader.readDuration()".to_string(),
            "SystemTime" => "reader.readInstant()".to_string(),
            "Uuid" => "reader.readUuid()".to_string(),
            "Url" => "reader.readUri()".to_string(),
            _ => "reader.readString()".to_string(),
        },
        ReadOp::Custom { underlying, .. } => emit_reader_read(underlying),
    }
}

fn emit_reader_vec(element_type: &TypeExpr, element: &ReadSeq, layout: &VecLayout) -> String {
    if let TypeExpr::Primitive(primitive) = element_type {
        return format!("reader.{}()", primitive_array_read_method(*primitive));
    }
    match layout {
        VecLayout::Blittable { .. } | VecLayout::Encoded => {
            let inner = emit_reader_read(element);
            format!("reader.readList {{ {} }}", inner)
        }
    }
}

fn primitive_array_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::I32 | PrimitiveType::U32 => "readIntArray",
        PrimitiveType::I16 | PrimitiveType::U16 => "readShortArray",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readLongArray"
        }
        PrimitiveType::F32 => "readFloatArray",
        PrimitiveType::F64 => "readDoubleArray",
        PrimitiveType::U8 | PrimitiveType::I8 => "readBytes",
        PrimitiveType::Bool => "readBooleanArray",
    }
}

fn enum_tag_read_method(tag_type: PrimitiveType) -> &'static str {
    match tag_type {
        PrimitiveType::I8 | PrimitiveType::U8 => "readI8",
        PrimitiveType::I16 | PrimitiveType::U16 => "readI16",
        PrimitiveType::I32 | PrimitiveType::U32 => "readI32",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "readI64"
        }
        PrimitiveType::Bool => "readBool",
        PrimitiveType::F32 => "readF32",
        PrimitiveType::F64 => "readF64",
    }
}

fn enum_tag_write_expr(tag_type: PrimitiveType, value_expr: &str) -> String {
    match tag_type {
        PrimitiveType::I8 | PrimitiveType::U8 => format!("wire.writeI8({value_expr})"),
        PrimitiveType::I16 | PrimitiveType::U16 => format!("wire.writeI16({value_expr})"),
        PrimitiveType::I32 | PrimitiveType::U32 => format!("wire.writeI32({value_expr})"),
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("wire.writeI64({value_expr})")
        }
        PrimitiveType::Bool => format!("wire.writeBool({value_expr})"),
        PrimitiveType::F32 => format!("wire.writeF32({value_expr})"),
        PrimitiveType::F64 => format!("wire.writeF64({value_expr})"),
    }
}

pub fn emit_write_expr(seq: &WriteSeq) -> String {
    let op = seq.ops.first().expect("write ops");
    match op {
        WriteOp::Primitive { primitive, value } => {
            emit_write_primitive(*primitive, &render_value(value))
        }
        WriteOp::String { value } => format!("wire.writeString({})", render_value(value)),
        WriteOp::Bytes { value } => format!("wire.writeBytes({})", render_value(value)),
        WriteOp::Option { value, some } => {
            let inner = emit_write_expr(some);
            format!(
                "{}?.let {{ v -> wire.writeU8(1u); {} }} ?: wire.writeU8(0u)",
                render_value(value),
                inner
            )
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => emit_write_vec(&render_value(value), element_type, element, layout),
        WriteOp::Record { value, .. } => {
            format!("{}.wireEncodeTo(wire)", render_value(value))
        }
        WriteOp::Enum { value, layout, .. } => match layout {
            EnumLayout::CStyle {
                tag_type,
                tag_strategy,
                ..
            } => match tag_strategy {
                EnumTagStrategy::Discriminant => {
                    enum_tag_write_expr(*tag_type, &format!("{}.value", render_value(value)))
                }
                EnumTagStrategy::OrdinalIndex => {
                    format!("{}.wireEncodeTo(wire)", render_value(value))
                }
            },
            EnumLayout::Data { .. } | EnumLayout::Recursive => {
                format!("{}.wireEncodeTo(wire)", render_value(value))
            }
        },
        WriteOp::Result { value, ok, err } => {
            let v = render_value(value);
            let ok_expr = emit_write_expr(ok);
            let err_expr = emit_write_expr(err);
            let ok_type = kotlin_type_for_write_seq(ok);
            let err_type = kotlin_type_for_write_seq(err);
            format!(
                "when ({}) {{ is BoltFFIResult.Ok<*> -> {{ wire.writeU8(0u); val okVal = boltffiUnsafeCast<{}>({}.value); {} }} is BoltFFIResult.Err<*> -> {{ wire.writeU8(1u); val errVal = boltffiUnsafeCast<{}>({}.error); {} }} }}",
                v, ok_type, v, ok_expr, err_type, v, err_expr
            )
        }
        WriteOp::Builtin { id, value } => emit_write_builtin(id, &render_value(value)),
        WriteOp::Custom { underlying, .. } => emit_write_expr(underlying),
    }
}

fn emit_vec_size(value: &str, inner: &SizeExpr, layout: &VecLayout) -> String {
    match (layout, inner) {
        (VecLayout::Blittable { .. }, _) | (_, SizeExpr::Fixed(_)) => {
            format!("(4 + {}.size * {})", value, emit_size_expr(inner))
        }
        (VecLayout::Encoded, _) => {
            format!(
                "(4 + {}.sumOf {{ item -> ({}).toInt() }})",
                value,
                emit_size_expr(inner)
            )
        }
    }
}

fn emit_builtin_size(id: &BuiltinId, value: &str) -> String {
    if id.as_str() == "Url" {
        format!("Utf8Codec.maxBytes({}.toString())", value)
    } else {
        format!("{}.wireEncodedSize()", value)
    }
}

fn emit_write_primitive(primitive: PrimitiveType, value: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("wire.writeBool({})", value),
        PrimitiveType::I8 => format!("wire.writeI8({})", value),
        PrimitiveType::U8 => format!("wire.writeU8({})", value),
        PrimitiveType::I16 => format!("wire.writeI16({})", value),
        PrimitiveType::U16 => format!("wire.writeU16({})", value),
        PrimitiveType::I32 => format!("wire.writeI32({})", value),
        PrimitiveType::U32 => format!("wire.writeU32({})", value),
        PrimitiveType::I64 | PrimitiveType::ISize => format!("wire.writeI64({})", value),
        PrimitiveType::U64 | PrimitiveType::USize => format!("wire.writeU64({})", value),
        PrimitiveType::F32 => format!("wire.writeF32({})", value),
        PrimitiveType::F64 => format!("wire.writeF64({})", value),
    }
}

fn emit_write_vec(
    value: &str,
    element_type: &TypeExpr,
    element: &WriteSeq,
    layout: &VecLayout,
) -> String {
    match (layout, element_type) {
        (_, TypeExpr::Primitive(_)) => format!("wire.writePrimitiveList({})", value),
        (VecLayout::Blittable { .. }, TypeExpr::Record(id)) => format!(
            "wire.writeU32({}.size.toUInt()); {}Writer.writeAllToWire(wire, {})",
            value,
            id.as_str(),
            value
        ),
        _ => {
            let inner = emit_write_expr(element);
            format!(
                "wire.writeU32({}.size.toUInt()); {}.forEach {{ item -> {} }}",
                value, value, inner
            )
        }
    }
}

fn emit_write_builtin(id: &BuiltinId, value: &str) -> String {
    match id.as_str() {
        "Duration" => format!("wire.writeDuration({})", value),
        "SystemTime" => format!("wire.writeInstant({})", value),
        "Uuid" => format!("wire.writeUuid({})", value),
        "Url" => format!("wire.writeUri({})", value),
        _ => format!("wire.writeString({})", value),
    }
}
