use askama::Template as _;

use crate::{
    ir::{
        BuiltinId, EnumLayout, PrimitiveType, ReadOp, ReadSeq, ReturnDef, SizeExpr, TypeExpr,
        ValueExpr, VecLayout, WriteOp, WriteSeq,
    },
    render::dart::{
        DartLibrary, NamingConvention,
        templates::{
            BuildHookTemplate, CallbackTemplate, ClassTemplate, CustomTypesTemplate,
            EnhancedEnumTemplate, NativeFunctionsTemplate, PreludeTemplate, PubspecTemplate,
            RecordTemplate, SealedClassEnumTemplate,
        },
    },
};

pub struct DartPackage {
    pub pubspec: String,
    pub lib: String,
    pub build: String,
}

pub struct DartEmitter {}

impl DartEmitter {
    pub fn emit(library: &DartLibrary, artifact_name: &str) -> DartPackage {
        let mut output = String::new();

        output.push_str(PreludeTemplate {}.render().unwrap().as_str());

        output.push_str("\n\n");
        output.push_str(
            CustomTypesTemplate {
                custom_types: &library.custom_types,
            }
            .render()
            .unwrap()
            .as_str(),
        );

        for r in &library.records {
            output.push_str("\n\n");
            output.push_str(RecordTemplate { record: r }.render().unwrap().as_str());
        }

        for e in &library.enums {
            let source = match &e.kind {
                super::DartEnumKind::Enhanced => {
                    EnhancedEnumTemplate { dart_enum: e }.render().unwrap()
                }
                super::DartEnumKind::SealedClass => {
                    SealedClassEnumTemplate { dart_enum: e }.render().unwrap()
                }
            };
            output.push_str("\n\n");
            output.push_str(source.as_str());
        }

        for cb in &library.callbacks {
            output.push_str("\n\n");
            output.push_str(CallbackTemplate { cb }.render().unwrap().as_str());
        }

        for class in &library.classes {
            output.push_str("\n\n");
            output.push_str(ClassTemplate { class }.render().unwrap().as_str());
        }

        output.push_str("\n\n");
        output.push_str(
            NativeFunctionsTemplate {
                cfuncs: &library.native.functions,
            }
            .render()
            .unwrap()
            .as_str(),
        );

        DartPackage {
            pubspec: PubspecTemplate {
                artifact_name,
                description: None,
                version: None,
                repository: None,
            }
            .render()
            .unwrap(),
            lib: output,
            build: BuildHookTemplate { artifact_name }.render().unwrap(),
        }
    }
}

pub fn primitive_dart_type(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "bool".to_string(),
        PrimitiveType::I8
        | PrimitiveType::U8
        | PrimitiveType::I16
        | PrimitiveType::U16
        | PrimitiveType::I32
        | PrimitiveType::U32
        | PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize => "int".to_string(),
        PrimitiveType::F32 | PrimitiveType::F64 => "double".to_string(),
    }
}

pub fn primitive_native_type(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "$$ffi.Bool",
        PrimitiveType::I8 => "$$ffi.Int8",
        PrimitiveType::I16 => "$$ffi.Int16",
        PrimitiveType::I32 => "$$ffi.Int32",
        PrimitiveType::I64 => "$$ffi.Int64",
        PrimitiveType::U8 => "$$ffi.Uint8",
        PrimitiveType::U16 => "$$ffi.Uint16",
        PrimitiveType::U32 => "$$ffi.Uint32",
        PrimitiveType::U64 => "$$ffi.Uint64",
        PrimitiveType::ISize => "$$ffi.IntPtr",
        PrimitiveType::USize => "$$ffi.UintPtr",
        PrimitiveType::F32 => "$$ffi.Float",
        PrimitiveType::F64 => "$$ffi.Double",
    }
}

fn render_type_name(name: &str) -> String {
    NamingConvention::class_name(name)
}

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

pub fn type_expr_dart_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Primitive(p) => primitive_dart_type(*p),
        TypeExpr::String | TypeExpr::Str => "String".to_string(),
        TypeExpr::Vec(inner) => match inner.as_ref() {
            TypeExpr::Primitive(primitive) => match primitive {
                PrimitiveType::I32 => "$$typed_data.Int32List".to_string(),
                PrimitiveType::U32 => "$$typed_data.Uint32List".to_string(),
                PrimitiveType::I16 => "$$typed_data.Int16List".to_string(),
                PrimitiveType::U16 => "$$typed_data.Uint16List".to_string(),
                PrimitiveType::I64 => "$$typed_data.Int64List".to_string(),
                PrimitiveType::U64 => "$$typed_data.Uint64List".to_string(),
                PrimitiveType::ISize => "$$typed_data.Int64List".to_string(),
                PrimitiveType::USize => "$$typed_data.Uint64List".to_string(),
                PrimitiveType::F32 => "$$typed_data.Float32List".to_string(),
                PrimitiveType::F64 => "$$typed_data.Float64List".to_string(),
                PrimitiveType::U8 => "$$typed_data.Uint8List".to_string(),
                PrimitiveType::I8 => "$$typed_data.Int8List".to_string(),
                PrimitiveType::Bool => "$$typed_data.Uint8List".to_string(),
            },
            _ => format!("List<{}>", type_expr_dart_type(inner)),
        },
        TypeExpr::Option(inner) => format!("{}?", type_expr_dart_type(inner)),
        TypeExpr::Result { ok, err } => format!(
            "BoltFFIResult<{}, {}>",
            type_expr_dart_type(ok),
            type_expr_dart_type(err)
        ),
        TypeExpr::Record(id) => render_type_name(id.as_str()),
        TypeExpr::Enum(id) => render_type_name(id.as_str()),
        TypeExpr::Custom(id) => render_type_name(id.as_str()),
        TypeExpr::Builtin(id) => match id.as_str() {
            "Duration" => "Duration".to_string(),
            "SystemTime" => "Datetime".to_string(),
            "Uuid" => "String".to_string(),
            "Url" => "Uri".to_string(),
            _ => "String".to_string(),
        },
        TypeExpr::Handle(class_id) => render_type_name(class_id.as_str()),
        TypeExpr::Callback(callback_id) => render_type_name(callback_id.as_str()),
        TypeExpr::Void => "void".to_string(),
    }
}

pub fn return_def_dart_type(return_def: &ReturnDef) -> String {
    match return_def {
        ReturnDef::Void => "void".to_string(),
        ReturnDef::Value(type_expr) => type_expr_dart_type(type_expr),
        ReturnDef::Result { ok, err } => format!(
            "BoltFFIResult<{}, {}>",
            type_expr_dart_type(ok),
            type_expr_dart_type(err)
        ),
    }
}

pub fn primitive_as_num(primitive: PrimitiveType, value: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("({} ? 1 : 0)", value),
        PrimitiveType::I8
        | PrimitiveType::U8
        | PrimitiveType::I16
        | PrimitiveType::U16
        | PrimitiveType::I32
        | PrimitiveType::U32
        | PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize
        | PrimitiveType::F32
        | PrimitiveType::F64 => value.to_string(),
    }
}

pub fn num_as_primitive(primitive: PrimitiveType, value: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("({} == 1)", value),
        PrimitiveType::I8
        | PrimitiveType::U8
        | PrimitiveType::I16
        | PrimitiveType::U16
        | PrimitiveType::I32
        | PrimitiveType::U32
        | PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize
        | PrimitiveType::F32
        | PrimitiveType::F64 => value.to_string(),
    }
}

pub fn primitive_blittable_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::I8 => "setInt8",
        PrimitiveType::Bool | PrimitiveType::U8 => "setUint8",
        PrimitiveType::I16 => "setInt16",
        PrimitiveType::U16 => "setUint16",
        PrimitiveType::I32 => "setInt32",
        PrimitiveType::U32 => "setUint32",
        PrimitiveType::I64 | PrimitiveType::ISize => "setInt64",
        PrimitiveType::U64 | PrimitiveType::USize => "setUint64",
        PrimitiveType::F32 => "setFloat32",
        PrimitiveType::F64 => "setFloat64",
    }
}

pub fn emit_write_blittable_value(
    offset: &str,
    primitive: PrimitiveType,
    value: &str,
    bytes_name: &str,
) -> String {
    format!(
        "{}.{}({}, {}{})",
        bytes_name,
        primitive_blittable_write_method(primitive),
        offset,
        primitive_as_num(primitive, value),
        match primitive {
            PrimitiveType::U8 | PrimitiveType::I8 | PrimitiveType::Bool => "",
            _ => ", $$typed_data.Endian.little",
        }
    )
}

pub fn primitive_blittable_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::I8 => "getInt8",
        PrimitiveType::Bool | PrimitiveType::U8 => "getUint8",
        PrimitiveType::I16 => "getInt16",
        PrimitiveType::U16 => "getUint16",
        PrimitiveType::I32 => "getInt32",
        PrimitiveType::U32 => "getUint32",
        PrimitiveType::I64 | PrimitiveType::ISize => "getInt64",
        PrimitiveType::U64 | PrimitiveType::USize => "getUint64",
        PrimitiveType::F32 => "getFloat32",
        PrimitiveType::F64 => "getFloat64",
    }
}

pub fn emit_read_blittable_value(
    offset: &str,
    primitive: PrimitiveType,
    bytes_name: &str,
) -> String {
    num_as_primitive(
        primitive,
        format!(
            "{}.{}({}{})",
            bytes_name,
            primitive_blittable_read_method(primitive),
            offset,
            match primitive {
                PrimitiveType::U8 | PrimitiveType::I8 | PrimitiveType::Bool => "",
                _ => ", $$typed_data.Endian.little",
            }
        )
        .as_str(),
    )
}

pub fn primitive_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "writeBool",
        PrimitiveType::I8 => "writeI8",
        PrimitiveType::U8 => "writeU8",
        PrimitiveType::I16 => "writeI16",
        PrimitiveType::U16 => "writeU16",
        PrimitiveType::I32 => "writeI32",
        PrimitiveType::U32 => "writeU32",
        PrimitiveType::I64 | PrimitiveType::ISize => "writeI64",
        PrimitiveType::U64 | PrimitiveType::USize => "writeU64",
        PrimitiveType::F32 => "writeF32",
        PrimitiveType::F64 => "writeF64",
    }
}

fn emit_write_primitive(primitive: PrimitiveType, writer_name: &str, value: &str) -> String {
    format!(
        "{}.{}({});",
        writer_name,
        primitive_write_method(primitive),
        value
    )
}

fn enum_tag_write_expr(tag_type: PrimitiveType, writer_name: &str, value: &str) -> String {
    let write_method = primitive_write_method(tag_type);

    format!("{}.{}({})", writer_name, write_method, value)
}

fn emit_write_builtin(id: &BuiltinId, writer_name: &str, value: &str) -> String {
    match id.as_str() {
        "Duration" => format!("{}.writeDuration({});", writer_name, value),
        "SystemTime" => format!("{}.writeInstant({});", writer_name, value),
        "Uuid" => format!("{}.writeUuid({});", writer_name, value),
        "Url" => format!("{}.writeUri({});", writer_name, value),
        _ => format!("{}.writeString({});", writer_name, value),
    }
}

fn write_seq_dart_type(seq: &WriteSeq) -> String {
    match seq.ops.first() {
        Some(WriteOp::Primitive { primitive, .. }) => {
            type_expr_dart_type(&TypeExpr::Primitive(*primitive))
        }
        Some(WriteOp::String { .. }) => "String".to_string(),
        Some(WriteOp::Builtin { id, .. }) => type_expr_dart_type(&TypeExpr::Builtin(id.clone())),
        Some(WriteOp::Record { id, .. }) => render_type_name(id.as_str()),
        Some(WriteOp::Enum { id, .. }) => render_type_name(id.as_str()),
        Some(WriteOp::Custom { id, .. }) => render_type_name(id.as_str()),
        Some(WriteOp::Vec { element_type, .. }) => {
            type_expr_dart_type(&TypeExpr::Vec(Box::new(element_type.clone())))
        }
        Some(WriteOp::Option { some, .. }) => format!("{}?", write_seq_dart_type(some)),
        Some(WriteOp::Result { ok, err, .. }) => format!(
            "BoltFFIResult<{}, {}>",
            write_seq_dart_type(ok),
            write_seq_dart_type(err)
        ),
        _ => "dynamic".to_string(),
    }
}

fn emit_writer_vec(
    value: &str,
    element_type: &TypeExpr,
    element: &WriteSeq,
    layout: &VecLayout,
    writer_name: &str,
) -> String {
    match layout {
        VecLayout::Blittable { .. } => match element_type {
            TypeExpr::Primitive(..) => format!("{writer_name}.writeTypedList({value});"),
            _ => {
                let inner_write_expr = emit_writer_write(element, writer_name, "item");
                format!(
                    "{writer_name}.writeList({value}, (item, {writer_name}) {{ {} }});",
                    inner_write_expr
                )
            }
        },
        VecLayout::Encoded => match element_type {
            TypeExpr::Primitive(primitive) => {
                let inner_write_expr = emit_write_primitive(*primitive, writer_name, "item");
                format!(
                    "{writer_name}.writeList({value}, (item, {writer_name}) {{ {} }});",
                    inner_write_expr
                )
            }
            _ => {
                let inner_write_expr = emit_writer_write(element, writer_name, "item");
                format!(
                    "{writer_name}.writeList({value}, (item, {writer_name}) {{ {} }});",
                    inner_write_expr
                )
            }
        },
    }
}

pub fn emit_writer_write(seq: &WriteSeq, writer_name: &str, value: &str) -> String {
    match seq.ops.first() {
        Some(WriteOp::Primitive { primitive, .. }) => {
            format!(
                "{writer_name}.{}({});",
                primitive_write_method(*primitive),
                value,
            )
        }
        Some(WriteOp::String { .. }) => format!("{writer_name}.writeString({value});"),
        Some(WriteOp::Builtin { id, .. }) => emit_write_builtin(id, writer_name, value),
        Some(WriteOp::Record { .. }) => format!("{value}._m$wireEncode({writer_name});",),
        Some(WriteOp::Enum { .. }) => format!("{value}._m$wireEncode({writer_name});"),
        Some(WriteOp::Custom { underlying, .. }) => {
            emit_writer_write(underlying, writer_name, value)
        }
        Some(WriteOp::Vec {
            element_type,
            element,
            layout,
            ..
        }) => emit_writer_vec(value, element_type, element, layout, writer_name),
        Some(WriteOp::Option { some, .. }) => {
            let inner_write_expr = emit_writer_write(some, writer_name, "value");

            format!(
                "{writer_name}.writeOptional({value}, (value, {writer_name}) {{ {inner_write_expr} }});"
            )
        }
        Some(WriteOp::Result { ok, err, .. }) => format!(
            r#"
{writer_name}.writeResult(
  {value},
  (value, {writer_name}) {{ {} }},
  (value, {writer_name}) {{ {} }}
);
            "#,
            emit_writer_write(ok, writer_name, "value"),
            emit_writer_write(err, writer_name, "value"),
        ),
        _ => ";".to_string(),
    }
}

pub fn primitive_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
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
    }
}

fn emit_reader_vec(
    element_type: &TypeExpr,
    element: &ReadSeq,
    layout: &VecLayout,
    reader_name: &str,
) -> String {
    match layout {
        VecLayout::Blittable { .. } => match element_type {
            TypeExpr::Primitive(primitive) => {
                let method = match primitive {
                    PrimitiveType::U8 | PrimitiveType::Bool => "readUint8List",
                    PrimitiveType::I8 => "readInt8List",
                    PrimitiveType::I16 => "readInt16List",
                    PrimitiveType::U16 => "readUint16List",
                    PrimitiveType::I32 => "readInt32List",
                    PrimitiveType::U32 => "readUint32List",
                    PrimitiveType::U64 | PrimitiveType::USize => "readUint64List",
                    PrimitiveType::I64 | PrimitiveType::ISize => "readInt64List",
                    PrimitiveType::F32 => "readFloat32List",
                    PrimitiveType::F64 => "readFloat64List",
                };
                format!("{reader_name}.{}()", method)
            }
            _ => {
                let inner_read_expr = emit_reader_read(element, reader_name);
                format!("{reader_name}.readList(({reader_name}) => {inner_read_expr})")
            }
        },
        VecLayout::Encoded => {
            let inner_read_expr = emit_reader_read(element, reader_name);
            format!("{reader_name}.readList(({reader_name}) => {inner_read_expr})")
        }
    }
}

pub fn emit_reader_read(seq: &ReadSeq, reader_name: &str) -> String {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => {
            format!("{reader_name}.{}()", primitive_read_method(*primitive))
        }
        ReadOp::String { .. } => format!("{reader_name}.readString()"),
        ReadOp::Record { id, .. } => {
            format!(
                "{}._m$wireDecode({reader_name})",
                NamingConvention::class_name(id.as_str())
            )
        }
        ReadOp::Enum { id, layout, .. } => match layout {
            EnumLayout::CStyle {
                tag_type,
                is_error: false,
                ..
            } => {
                format!(
                    "{}._m$fromValue({reader_name}.{}())",
                    render_type_name(id.as_str()),
                    primitive_read_method(*tag_type),
                )
            }
            EnumLayout::CStyle { is_error: true, .. }
            | EnumLayout::Data { .. }
            | EnumLayout::Recursive => {
                format!(
                    "{}._m$wireDecode({reader_name})",
                    render_type_name(id.as_str())
                )
            }
        },
        ReadOp::Option { some, .. } => {
            let inner_read_expr = emit_reader_read(some, reader_name);
            format!("{reader_name}.readOptional(({reader_name}) => {inner_read_expr})")
        }
        ReadOp::Vec {
            element_type,
            element,
            layout,
            ..
        } => emit_reader_vec(element_type, element, layout, reader_name),
        ReadOp::Result { ok, err, .. } => {
            let ok_expr = emit_reader_read(ok, reader_name);
            let err_expr = emit_reader_read(err, reader_name);
            format!(
                r#"
{reader_name}.readResult(
  ({reader_name}) => {ok_expr},
  ({reader_name}) => {err_expr}
)
            "#
            )
        }
        ReadOp::Builtin { id, .. } => match id.as_str() {
            "Duration" => "reader.readDuration()".to_string(),
            "SystemTime" => "reader.readInstant()".to_string(),
            "Uuid" => "reader.readUuid()".to_string(),
            "Url" => "reader.readUri()".to_string(),
            _ => "reader.readString()".to_string(),
        },
        ReadOp::Custom { underlying, .. } => emit_reader_read(underlying, reader_name),
    }
}

fn remap_size_expr_value_expr(expr: &SizeExpr, v: ValueExpr) -> SizeExpr {
    match expr {
        SizeExpr::Fixed(value) => SizeExpr::Fixed(*value),
        SizeExpr::Runtime => SizeExpr::Runtime,
        SizeExpr::StringLen(..) => SizeExpr::StringLen(v),
        SizeExpr::BytesLen(..) => SizeExpr::BytesLen(v),
        SizeExpr::ValueSize(..) => SizeExpr::ValueSize(v),
        SizeExpr::WireSize { owner, .. } => SizeExpr::WireSize {
            owner: owner.clone(),
            value: v,
        },
        SizeExpr::BuiltinSize { id, .. } => SizeExpr::BuiltinSize {
            id: id.clone(),
            value: v,
        },
        SizeExpr::Sum(exprs) => SizeExpr::Sum(
            exprs
                .iter()
                .map(|s| remap_size_expr_value_expr(s, v.clone()))
                .collect(),
        ),
        SizeExpr::OptionSize { inner, .. } => SizeExpr::OptionSize {
            value: v,
            inner: inner.clone(),
        },
        SizeExpr::VecSize { inner, layout, .. } => SizeExpr::VecSize {
            value: v,
            inner: inner.clone(),
            layout: layout.clone(),
        },
        SizeExpr::ResultSize { ok, err, .. } => SizeExpr::ResultSize {
            value: v,
            ok: ok.clone(),
            err: err.clone(),
        },
    }
}

fn emit_vec_size(value: &str, inner: &SizeExpr, layout: &VecLayout) -> String {
    match layout {
        VecLayout::Blittable { .. } => {
            format!("(4 + {}.length * {})", value, emit_size_expr(inner))
        }
        VecLayout::Encoded => format!(
            "{value}.fold<int>(0, (sum, item) => sum + {})",
            emit_size_expr(&remap_size_expr_value_expr(
                inner,
                ValueExpr::Named("item".to_string())
            ))
        ),
    }
}

fn emit_builtin_size(id: &BuiltinId, value: &str) -> String {
    if id.as_str() == "Url" {
        format!("{}.toString().length * 3", value)
    } else {
        format!("{}._m$wireEncodedSize()", value)
    }
}

pub fn emit_size_expr(size: &SizeExpr) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::Runtime => "0".to_string(),
        SizeExpr::StringLen(value) => format!("({}.length * 3)", render_value(value)),
        SizeExpr::BytesLen(value) => format!("{}.length", render_value(value)),
        SizeExpr::ValueSize(value) => render_value(value),
        SizeExpr::WireSize { value, .. } => format!("{}._m$wireEncodedSize()", render_value(value)),
        SizeExpr::BuiltinSize { id, value } => emit_builtin_size(id, &render_value(value)),
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(emit_size_expr)
                .reduce(|acc, s| acc + " + " + s.as_str())
                .unwrap_or_else(|| String::from("0"));
            format!("({})", rendered)
        }
        SizeExpr::OptionSize { value, inner } => {
            let inner_expr = emit_size_expr(&remap_size_expr_value_expr(
                inner,
                ValueExpr::Var(format!("{}!", render_value(value))),
            ));
            format!(
                "(switch ({} == null) {{ true => 1, false => 1 + {} }})",
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
            let ok_expr = emit_size_expr(&remap_size_expr_value_expr(
                ok,
                ValueExpr::Var("value".to_string()),
            ));
            let err_expr = emit_size_expr(&remap_size_expr_value_expr(
                err,
                ValueExpr::Var("value".to_string()),
            ));
            format!(
                "1 + (switch ({}) {{ BoltFFIResult$Ok(:final value) => {}, BoltFFIResult$Err(:final value) => {} }})",
                render_value(value),
                ok_expr,
                err_expr
            )
        }
    }
}
