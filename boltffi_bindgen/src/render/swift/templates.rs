use super::plan::{SwiftCallMode, SwiftCallback, SwiftClass, SwiftEnum, SwiftField, SwiftFunction, SwiftRecord, SwiftStreamMode, SwiftVariant};
use askama::Template;

pub fn swift_doc_block(doc: &Option<String>, indent: &str) -> String {
    match doc {
        Some(text) => {
            let lines: String = text
                .lines()
                .map(|line| if line.is_empty() { format!("{indent}///\n") } else { format!("{indent}/// {line}\n") })
                .collect();
            lines
        }
        None => String::new(),
    }
}

#[derive(Template)]
#[template(path = "preamble_header.txt", escape = "none")]
pub struct PreambleHeaderTemplate<'a> {
    pub ffi_module_name: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "preamble_footer.txt", escape = "none")]
pub struct PreambleFooterTemplate<'a> {
    pub prefix: &'a str,
    pub has_async: bool,
    pub has_streams: bool,
    pub has_async_iterators: bool,
}

pub fn render_preamble_header(ffi_module_name: Option<&str>) -> String {
    PreambleHeaderTemplate { ffi_module_name }.render().unwrap()
}

pub fn render_preamble_footer(prefix: &str, has_async: bool, has_streams: bool, has_async_iterators: bool) -> String {
    PreambleFooterTemplate {
        prefix,
        has_async,
        has_streams,
        has_async_iterators,
    }
    .render()
    .unwrap()
}

#[derive(Template)]
#[template(path = "record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub class_name: &'a str,
    pub fields: &'a [SwiftField],
    pub is_blittable: bool,
    pub blittable_size: Option<usize>,
    pub doc: &'a Option<String>,
}

impl<'a> RecordTemplate<'a> {
    pub fn from_record(record: &'a SwiftRecord) -> Self {
        Self {
            class_name: &record.class_name,
            fields: &record.fields,
            is_blittable: record.is_blittable,
            blittable_size: record.blittable_size,
            doc: &record.doc,
        }
    }
}

#[derive(Template)]
#[template(path = "enum_c_style.txt", escape = "none")]
pub struct EnumCStyleTemplate<'a> {
    pub class_name: &'a str,
    pub variants: &'a [SwiftVariant],
    pub is_error: bool,
    pub doc: &'a Option<String>,
}

impl<'a> EnumCStyleTemplate<'a> {
    pub fn from_enum(e: &'a SwiftEnum) -> Self {
        Self {
            class_name: &e.name,
            variants: &e.variants,
            is_error: e.is_error,
            doc: &e.doc,
        }
    }
}

#[derive(Template)]
#[template(path = "enum_data.txt", escape = "none")]
pub struct EnumDataTemplate<'a> {
    pub class_name: &'a str,
    pub variants: &'a [SwiftVariant],
    pub is_error: bool,
    pub doc: &'a Option<String>,
}

impl<'a> EnumDataTemplate<'a> {
    pub fn from_enum(e: &'a SwiftEnum) -> Self {
        Self {
            class_name: &e.name,
            variants: &e.variants,
            is_error: e.is_error,
            doc: &e.doc,
        }
    }
}

pub fn render_record(record: &SwiftRecord) -> String {
    RecordTemplate::from_record(record).render().unwrap()
}

pub fn render_enum(e: &SwiftEnum) -> String {
    if e.is_c_style() {
        EnumCStyleTemplate::from_enum(e).render().unwrap()
    } else {
        EnumDataTemplate::from_enum(e).render().unwrap()
    }
}

#[derive(Template)]
#[template(path = "callback_trait.txt", escape = "none")]
pub struct CallbackTemplate<'a> {
    pub callback: &'a SwiftCallback,
}

impl<'a> CallbackTemplate<'a> {
    pub fn new(callback: &'a SwiftCallback) -> Self {
        Self { callback }
    }
}

pub fn render_callback(callback: &SwiftCallback) -> String {
    CallbackTemplate::new(callback).render().unwrap()
}

#[derive(Template)]
#[template(path = "function.txt", escape = "none")]
pub struct FunctionTemplate<'a> {
    pub func: &'a SwiftFunction,
    pub prefix: &'a str,
}

impl<'a> FunctionTemplate<'a> {
    pub fn new(func: &'a SwiftFunction, prefix: &'a str) -> Self {
        Self { func, prefix }
    }
}

pub fn render_function(func: &SwiftFunction, prefix: &str) -> String {
    FunctionTemplate::new(func, prefix).render().unwrap()
}

#[derive(Template)]
#[template(path = "class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub cls: &'a SwiftClass,
    pub prefix: &'a str,
}

impl<'a> ClassTemplate<'a> {
    pub fn new(cls: &'a SwiftClass, prefix: &'a str) -> Self {
        Self { cls, prefix }
    }
}

pub fn render_class(cls: &SwiftClass, prefix: &str) -> String {
    ClassTemplate::new(cls, prefix).render().unwrap()
}

use super::plan::SwiftModule;

pub struct SwiftEmitter {
    prefix: String,
    ffi_module_name: Option<String>,
}

impl SwiftEmitter {
    pub fn new() -> Self {
        Self {
            prefix: String::new(),
            ffi_module_name: None,
        }
    }

    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            ffi_module_name: None,
        }
    }

    pub fn with_ffi_module(mut self, ffi_module: impl Into<String>) -> Self {
        self.ffi_module_name = Some(ffi_module.into());
        self
    }

    pub fn emit(&self, module: &SwiftModule) -> String {
        let mut output = String::new();

        output.push_str(&render_preamble_header(self.ffi_module_name.as_deref()));
        output.push_str("\n\n");

        module.custom_types.iter().for_each(|ct| {
            if ct.native_mapping.is_none() {
                output.push_str(&format!("public typealias {} = {}\n", ct.alias_name, ct.target_type));
            }
        });
        if module.custom_types.iter().any(|ct| ct.native_mapping.is_none()) {
            output.push('\n');
        }

        module.records.iter().for_each(|record| {
            output.push_str(&render_record(record));
            output.push_str("\n\n");
        });

        module.enums.iter().for_each(|enumeration| {
            output.push_str(&render_enum(enumeration));
            output.push_str("\n\n");
        });

        module.callbacks.iter().for_each(|callback| {
            output.push_str(&render_callback(callback));
            output.push_str("\n\n");
        });

        module.functions.iter().for_each(|func| {
            output.push_str(&render_function(func, &self.prefix));
            output.push_str("\n\n");
        });

        module.classes.iter().for_each(|class_def| {
            output.push_str(&render_class(class_def, &self.prefix));
            output.push_str("\n\n");
        });

        output.push_str(&render_preamble_footer(
            &self.prefix,
            module.has_async(),
            module.has_streams(),
            module.has_async_iterators(),
        ));
        output.push('\n');

        output
    }
}

impl Default for SwiftEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;
    use crate::{
        ir::{
            codec::VecLayout,
            ids::RecordId,
            ops::{OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq},
            types::{PrimitiveType, TypeExpr},
        },
        render::swift::plan::{
            SwiftAsyncIterator, SwiftAsyncResult, SwiftCallback, SwiftCallbackMethod, SwiftCallbackParam, SwiftClass, SwiftConstructor, SwiftConversion, SwiftEnumStyle,
            SwiftFunction, SwiftMethod, SwiftParam, SwiftReturn, SwiftStream, SwiftStreamMode, SwiftVariantPayload,
        },
    };

    fn val(name: &str) -> ValueExpr {
        ValueExpr::Var(name.to_string())
    }

    fn offset(base: &str) -> OffsetExpr {
        OffsetExpr::Var(base.to_string())
    }

    fn offset_plus(base: &str, add: usize) -> OffsetExpr {
        if add == 0 {
            OffsetExpr::Var(base.to_string())
        } else {
            OffsetExpr::VarPlus(base.to_string(), add)
        }
    }

    fn read_primitive(primitive: PrimitiveType, offset_expr: OffsetExpr) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Fixed(primitive.wire_size_bytes()),
            ops: vec![ReadOp::Primitive { primitive, offset: offset_expr }],
            shape: WireShape::Value,
        }
    }

    fn read_empty() -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Fixed(0),
            ops: Vec::new(),
            shape: WireShape::Value,
        }
    }

    fn write_primitive(primitive: PrimitiveType, value: &str) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Fixed(primitive.wire_size_bytes()),
            ops: vec![WriteOp::Primitive { primitive, value: val(value) }],
            shape: WireShape::Value,
        }
    }

    fn read_string(offset_expr: OffsetExpr) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Runtime,
            ops: vec![ReadOp::String { offset: offset_expr }],
            shape: WireShape::Value,
        }
    }

    fn write_string(value: &str) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::StringLen(val(value))]),
            ops: vec![WriteOp::String { value: val(value) }],
            shape: WireShape::Value,
        }
    }

    fn read_bytes(offset_expr: OffsetExpr) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Runtime,
            ops: vec![ReadOp::Bytes { offset: offset_expr }],
            shape: WireShape::Value,
        }
    }

    fn write_bytes(value: &str) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::BytesLen(val(value))]),
            ops: vec![WriteOp::Bytes { value: val(value) }],
            shape: WireShape::Value,
        }
    }

    fn read_option(offset_expr: OffsetExpr, inner: ReadSeq) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Runtime,
            ops: vec![ReadOp::Option {
                tag_offset: offset_expr,
                some: Box::new(inner),
            }],
            shape: WireShape::Optional,
        }
    }

    fn write_option(value: &str, inner: WriteSeq) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::OptionSize {
                value: val(value),
                inner: Box::new(inner.size.clone()),
            },
            ops: vec![WriteOp::Option {
                value: val(value),
                some: Box::new(inner),
            }],
            shape: WireShape::Optional,
        }
    }

    fn read_vec(offset_expr: OffsetExpr, element_type: TypeExpr, element: ReadSeq, layout: VecLayout) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Runtime,
            ops: vec![ReadOp::Vec {
                len_offset: offset_expr,
                element_type,
                element: Box::new(element),
                layout,
            }],
            shape: WireShape::Sequence,
        }
    }

    fn write_vec(value: &str, element: WriteSeq, element_type: &TypeExpr, layout: VecLayout) -> WriteSeq {
        let size = if matches!(element_type, TypeExpr::Primitive(PrimitiveType::U8)) {
            SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::BytesLen(val(value))])
        } else {
            SizeExpr::VecSize {
                value: val(value),
                inner: Box::new(element.size.clone()),
                layout: layout.clone(),
            }
        };
        WriteSeq {
            size,
            ops: vec![WriteOp::Vec {
                value: val(value),
                element: Box::new(element),
                element_type: element_type.clone(),
                layout,
            }],
            shape: WireShape::Sequence,
        }
    }

    fn read_record(id: &str, offset_expr: OffsetExpr) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Runtime,
            ops: vec![ReadOp::Record {
                id: RecordId::new(id),
                offset: offset_expr,
                fields: vec![],
            }],
            shape: WireShape::Value,
        }
    }

    fn write_record(id: &str, value: &str, size: usize) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Fixed(size),
            ops: vec![WriteOp::Record {
                id: RecordId::new(id),
                value: val(value),
                fields: vec![],
            }],
            shape: WireShape::Value,
        }
    }

    fn field(name: &str, swift_type: &str, decode: ReadSeq, encode: WriteSeq, c_offset: Option<usize>, default_expr: Option<&str>) -> SwiftField {
        SwiftField {
            swift_name: name.to_string(),
            swift_type: swift_type.to_string(),
            default_expr: default_expr.map(|s| s.to_string()),
            decode,
            encode,
            doc: None,
            c_offset,
            native_conversion: None,
        }
    }

    #[test]
    fn snapshot_blittable_point() {
        let record = SwiftRecord {
            class_name: "Point".to_string(),
            fields: vec![
                field(
                    "x",
                    "Double",
                    read_primitive(PrimitiveType::F64, offset_plus("offset", 0)),
                    write_primitive(PrimitiveType::F64, "x"),
                    Some(0),
                    None,
                ),
                field(
                    "y",
                    "Double",
                    read_primitive(PrimitiveType::F64, offset_plus("offset", 8)),
                    write_primitive(PrimitiveType::F64, "y"),
                    Some(8),
                    None,
                ),
            ],
            is_blittable: true,
            blittable_size: Some(16),
            doc: None,
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_blittable_with_alignment_padding() {
        let record = SwiftRecord {
            class_name: "Padded".to_string(),
            fields: vec![
                field(
                    "a",
                    "UInt8",
                    read_primitive(PrimitiveType::U8, offset_plus("offset", 0)),
                    write_primitive(PrimitiveType::U8, "a"),
                    Some(0),
                    None,
                ),
                field(
                    "b",
                    "UInt32",
                    read_primitive(PrimitiveType::U32, offset_plus("offset", 4)),
                    write_primitive(PrimitiveType::U32, "b"),
                    Some(4),
                    None,
                ),
                field(
                    "c",
                    "UInt8",
                    read_primitive(PrimitiveType::U8, offset_plus("offset", 8)),
                    write_primitive(PrimitiveType::U8, "c"),
                    Some(8),
                    None,
                ),
            ],
            is_blittable: true,
            blittable_size: Some(12),
            doc: None,
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_encoded_record_with_string() {
        let record = SwiftRecord {
            class_name: "User".to_string(),
            fields: vec![
                field(
                    "id",
                    "Int64",
                    read_primitive(PrimitiveType::I64, offset("pos")),
                    write_primitive(PrimitiveType::I64, "id"),
                    None,
                    None,
                ),
                field("name", "String", read_string(offset("pos")), write_string("name"), None, None),
            ],
            is_blittable: false,
            blittable_size: None,
            doc: None,
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_record_with_default_value() {
        let record = SwiftRecord {
            class_name: "Config".to_string(),
            fields: vec![
                field(
                    "timeout",
                    "Double",
                    read_primitive(PrimitiveType::F64, offset_plus("offset", 0)),
                    write_primitive(PrimitiveType::F64, "timeout"),
                    Some(0),
                    Some("30.0"),
                ),
                field(
                    "retries",
                    "Int32",
                    read_primitive(PrimitiveType::I32, offset_plus("offset", 8)),
                    write_primitive(PrimitiveType::I32, "retries"),
                    Some(8),
                    Some("3"),
                ),
            ],
            is_blittable: true,
            blittable_size: Some(12),
            doc: None,
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_record_with_field_docs() {
        let record = SwiftRecord {
            class_name: "Location".to_string(),
            fields: vec![
                {
                    let mut f = field(
                        "id",
                        "Int64",
                        read_primitive(PrimitiveType::I64, offset_plus("offset", 0)),
                        write_primitive(PrimitiveType::I64, "id"),
                        Some(0),
                        None,
                    );
                    f.doc = Some("Unique identifier for this location.".to_string());
                    f
                },
                {
                    let mut f = field(
                        "lat",
                        "Double",
                        read_primitive(PrimitiveType::F64, offset_plus("offset", 8)),
                        write_primitive(PrimitiveType::F64, "lat"),
                        Some(8),
                        None,
                    );
                    f.doc = Some("Latitude in decimal degrees.".to_string());
                    f
                },
            ],
            is_blittable: true,
            blittable_size: Some(16),
            doc: Some("A physical location with coordinates.".to_string()),
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_enum_with_variant_docs() {
        let e = SwiftEnum {
            name: "Direction".to_string(),
            style: SwiftEnumStyle::CStyle,
            is_error: false,
            variants: vec![
                SwiftVariant {
                    swift_name: "north".to_string(),
                    discriminant: 0,
                    payload: SwiftVariantPayload::Unit,
                    doc: Some("Pointing toward the north pole.".to_string()),
                },
                SwiftVariant {
                    swift_name: "south".to_string(),
                    discriminant: 1,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
            ],
            doc: Some("A cardinal compass direction.".to_string()),
        };
        insta::assert_snapshot!(render_enum(&e));
    }

    #[test]
    fn snapshot_c_style_enum() {
        let e = SwiftEnum {
            name: "Status".to_string(),
            style: SwiftEnumStyle::CStyle,
            is_error: false,
            variants: vec![
                SwiftVariant {
                    swift_name: "active".to_string(),
                    discriminant: 0,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "inactive".to_string(),
                    discriminant: 1,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "pending".to_string(),
                    discriminant: 2,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
            ],
            doc: None,
        };
        insta::assert_snapshot!(render_enum(&e));
    }

    #[test]
    fn snapshot_c_style_error_enum() {
        let e = SwiftEnum {
            name: "ApiError".to_string(),
            style: SwiftEnumStyle::CStyle,
            is_error: true,
            variants: vec![
                SwiftVariant {
                    swift_name: "notFound".to_string(),
                    discriminant: 0,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "unauthorized".to_string(),
                    discriminant: 1,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "serverError".to_string(),
                    discriminant: 2,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
            ],
            doc: None,
        };
        insta::assert_snapshot!(render_enum(&e));
    }

    #[test]
    fn snapshot_data_enum_with_payloads() {
        let e = SwiftEnum {
            name: "Message".to_string(),
            style: SwiftEnumStyle::Data,
            is_error: false,
            variants: vec![
                SwiftVariant {
                    swift_name: "empty".to_string(),
                    discriminant: 0,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "text".to_string(),
                    discriminant: 1,
                    payload: SwiftVariantPayload::Tuple(vec![field("value", "String", read_string(offset("pos")), write_string("value"), None, None)]),
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "number".to_string(),
                    discriminant: 2,
                    payload: SwiftVariantPayload::Tuple(vec![field(
                        "value",
                        "Int64",
                        read_primitive(PrimitiveType::I64, offset("pos")),
                        write_primitive(PrimitiveType::I64, "value"),
                        None,
                        None,
                    )]),
                    doc: None,
                },
            ],
            doc: None,
        };
        insta::assert_snapshot!(render_enum(&e));
    }

    #[test]
    fn snapshot_data_enum_with_struct_payload() {
        let e = SwiftEnum {
            name: "Event".to_string(),
            style: SwiftEnumStyle::Data,
            is_error: false,
            variants: vec![
                SwiftVariant {
                    swift_name: "click".to_string(),
                    discriminant: 0,
                    payload: SwiftVariantPayload::Struct(vec![
                        field(
                            "x",
                            "Int32",
                            read_primitive(PrimitiveType::I32, offset("pos")),
                            write_primitive(PrimitiveType::I32, "x"),
                            None,
                            None,
                        ),
                        field(
                            "y",
                            "Int32",
                            read_primitive(PrimitiveType::I32, offset("pos")),
                            write_primitive(PrimitiveType::I32, "y"),
                            None,
                            None,
                        ),
                    ]),
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "keyPress".to_string(),
                    discriminant: 1,
                    payload: SwiftVariantPayload::Struct(vec![field(
                        "code",
                        "UInt32",
                        read_primitive(PrimitiveType::U32, offset("pos")),
                        write_primitive(PrimitiveType::U32, "code"),
                        None,
                        None,
                    )]),
                    doc: None,
                },
            ],
            doc: None,
        };
        insta::assert_snapshot!(render_enum(&e));
    }

    #[test]
    fn snapshot_sync_function_returning_primitive() {
        let func = SwiftFunction {
            name: "add".to_string(),
            mode: SwiftCallMode::Sync {
                symbol: "boltffi_add".to_string(),
            },
            params: vec![
                SwiftParam {
                    label: None,
                    name: "a".to_string(),
                    swift_type: "Int32".to_string(),
                    conversion: SwiftConversion::Direct,
                },
                SwiftParam {
                    label: None,
                    name: "b".to_string(),
                    swift_type: "Int32".to_string(),
                    conversion: SwiftConversion::Direct,
                },
            ],
            returns: SwiftReturn::Direct { swift_type: "Int32".to_string() },
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }

    #[test]
    fn snapshot_sync_function_with_string_param() {
        let func = SwiftFunction {
            name: "greet".to_string(),
            mode: SwiftCallMode::Sync {
                symbol: "boltffi_greet".to_string(),
            },
            params: vec![SwiftParam {
                label: None,
                name: "name".to_string(),
                swift_type: "String".to_string(),
                conversion: SwiftConversion::ToString,
            }],
            returns: SwiftReturn::FromWireBuffer {
                swift_type: "String".to_string(),
                decode: read_string(offset("pos")),
                encode: write_string("value"),
            },
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }

    #[test]
    fn snapshot_sync_function_with_record_param() {
        let func = SwiftFunction {
            name: "processPoint".to_string(),
            mode: SwiftCallMode::Sync {
                symbol: "boltffi_process_point".to_string(),
            },
            params: vec![SwiftParam {
                label: None,
                name: "point".to_string(),
                swift_type: "Point".to_string(),
                conversion: SwiftConversion::ToWireBuffer {
                    encode: write_record("Point", "point", 16),
                },
            }],
            returns: SwiftReturn::FromWireBuffer {
                swift_type: "Point".to_string(),
                decode: read_record("Point", offset("pos")),
                encode: write_record("Point", "value", 16),
            },
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }

    #[test]
    fn snapshot_async_function_returning_string() {
        let func = SwiftFunction {
            name: "fetchData".to_string(),
            mode: SwiftCallMode::Async {
                start: "boltffi_fetch_data_start".to_string(),
                poll: "boltffi_fetch_data_poll".to_string(),
                complete: "boltffi_fetch_data_complete".to_string(),
                cancel: "boltffi_fetch_data_cancel".to_string(),
                free: "boltffi_fetch_data_free".to_string(),
                result: Box::new(SwiftAsyncResult::Encoded {
                    swift_type: "String".to_string(),
                    ok_type: None,
                    decode: read_string(offset("pos")),
                    throws: false,
                    err_decode: read_empty(),
                    err_is_string: false,
                }),
            },
            params: vec![SwiftParam {
                label: None,
                name: "url".to_string(),
                swift_type: "String".to_string(),
                conversion: SwiftConversion::ToString,
            }],
            returns: SwiftReturn::Void,
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }

    #[test]
    fn snapshot_callback_trait_simple() {
        let callback = SwiftCallback {
            protocol_name: "DataHandler".to_string(),
            wrapper_class: "DataHandlerWrapper".to_string(),
            vtable_var: "dataHandlerVtable".to_string(),
            vtable_type: "DataHandlerVtable".to_string(),
            bridge_name: "DataHandlerBridge".to_string(),
            register_fn: "boltffi_register_data_handler".to_string(),
            create_fn: "boltffi_create_data_handler".to_string(),
            methods: vec![SwiftCallbackMethod {
                swift_name: "onData".to_string(),
                ffi_name: "on_data".to_string(),
                params: vec![SwiftCallbackParam {
                    label: "data".to_string(),
                    swift_type: "Data".to_string(),
                    call_arg: "data".to_string(),
                    ffi_args: vec!["dataPtr".to_string(), "dataLen".to_string()],
                    decode_prelude: Some("let data = Data(bytes: dataPtr!, count: Int(dataLen))".to_string()),
                }],
                returns: SwiftReturn::Void,
                is_async: false,
                has_out_param: false,
                doc: None,
            }],
            doc: None,
        };
        insta::assert_snapshot!(render_callback(&callback));
    }

    #[test]
    fn snapshot_callback_trait_with_return() {
        let callback = SwiftCallback {
            protocol_name: "Validator".to_string(),
            wrapper_class: "ValidatorWrapper".to_string(),
            vtable_var: "validatorVtable".to_string(),
            vtable_type: "ValidatorVtable".to_string(),
            bridge_name: "ValidatorBridge".to_string(),
            register_fn: "boltffi_register_validator".to_string(),
            create_fn: "boltffi_create_validator".to_string(),
            methods: vec![SwiftCallbackMethod {
                swift_name: "validate".to_string(),
                ffi_name: "validate".to_string(),
                params: vec![SwiftCallbackParam {
                    label: "input".to_string(),
                    swift_type: "String".to_string(),
                    call_arg: "input".to_string(),
                    ffi_args: vec!["inputPtr".to_string(), "inputLen".to_string()],
                    decode_prelude: Some("let input = String(decoding: UnsafeBufferPointer(start: inputPtr, count: Int(inputLen)), as: UTF8.self)".to_string()),
                }],
                returns: SwiftReturn::Direct { swift_type: "Bool".to_string() },
                is_async: false,
                has_out_param: true,
                doc: None,
            }],
            doc: None,
        };
        insta::assert_snapshot!(render_callback(&callback));
    }

    #[test]
    fn snapshot_class_with_documented_constructors_and_method() {
        let cls = SwiftClass {
            name: "DataStore".to_string(),
            ffi_free: "boltffi_data_store_free".to_string(),
            constructors: vec![
                SwiftConstructor::Designated {
                    ffi_symbol: "boltffi_data_store_new".to_string(),
                    params: vec![SwiftParam {
                        label: None,
                        name: "capacity".to_string(),
                        swift_type: "Int32".to_string(),
                        conversion: SwiftConversion::Direct,
                    }],
                    is_fallible: false,
                    doc: Some("Creates a new data store with the given capacity.".to_string()),
                },
                SwiftConstructor::Factory {
                    name: "withDefaults".to_string(),
                    ffi_symbol: "boltffi_data_store_with_defaults".to_string(),
                    is_fallible: false,
                    doc: Some("Creates a data store with sensible default settings.".to_string()),
                },
            ],
            methods: vec![SwiftMethod {
                name: "insert".to_string(),
                mode: SwiftCallMode::Sync {
                    symbol: "boltffi_data_store_insert".to_string(),
                },
                params: vec![SwiftParam {
                    label: None,
                    name: "key".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                }],
                returns: SwiftReturn::Void,
                is_static: false,
                doc: Some("Inserts a value into the store by key.".to_string()),
            }],
            streams: vec![],
            async_iterators: vec![],
            doc: Some("A persistent key-value data store.".to_string()),
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_constructor_and_method() {
        let cls = SwiftClass {
            name: "Database".to_string(),
            ffi_free: "boltffi_database_free".to_string(),
            constructors: vec![SwiftConstructor::Designated {
                ffi_symbol: "boltffi_database_open".to_string(),
                params: vec![SwiftParam {
                    label: None,
                    name: "path".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                }],
                is_fallible: false,
                doc: None,
            }],
            methods: vec![SwiftMethod {
                name: "query".to_string(),
                mode: SwiftCallMode::Sync {
                    symbol: "boltffi_database_query".to_string(),
                },
                params: vec![SwiftParam {
                    label: None,
                    name: "sql".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                }],
                returns: SwiftReturn::FromWireBuffer {
                    swift_type: "String".to_string(),
                    decode: read_string(offset("pos")),
                    encode: write_string("value"),
                },
                is_static: false,
                doc: None,
            }],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_stream() {
        let cls = SwiftClass {
            name: "EventSource".to_string(),
            ffi_free: "boltffi_event_source_free".to_string(),
            constructors: vec![],
            methods: vec![],
            streams: vec![SwiftStream {
                name: "events".to_string(),
                mode: SwiftStreamMode::Async,
                item_type: "String".to_string(),
                item_decode: read_string(OffsetExpr::Fixed(0)),
                subscribe: "boltffi_event_source_events_subscribe".to_string(),
                poll: "boltffi_event_source_events_poll".to_string(),
                pop_batch: "boltffi_event_source_events_pop_batch".to_string(),
                wait: "boltffi_event_source_events_wait".to_string(),
                unsubscribe: "boltffi_event_source_events_unsubscribe".to_string(),
                free: "boltffi_event_source_events_free".to_string(),
                free_buf: "boltffi_free_buf_u8".to_string(),
                atomic_cas: "boltffi_atomic_u8_cas".to_string(),
            }],
            async_iterators: vec![],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_async_iterator() {
        let item_decode = read_primitive(PrimitiveType::I32, OffsetExpr::Var("pos".to_string()));
        let option_decode = read_option(OffsetExpr::Var("pos".to_string()), item_decode);
        let cls = SwiftClass {
            name: "NumberIterator".to_string(),
            ffi_free: "boltffi_number_iterator_free".to_string(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![SwiftAsyncIterator {
                name: "countToThree".to_string(),
                item_type: "Int32".to_string(),
                item_decode: option_decode,
                entry: "boltffi_number_iterator_count_to_three".to_string(),
                next: "boltffi_number_iterator_count_to_three_next".to_string(),
                next_poll: "boltffi_number_iterator_count_to_three_next_poll".to_string(),
                next_complete: "boltffi_number_iterator_count_to_three_next_complete".to_string(),
                next_cancel: "boltffi_number_iterator_count_to_three_next_cancel".to_string(),
                next_free: "boltffi_number_iterator_count_to_three_next_free".to_string(),
                free: "boltffi_number_iterator_count_to_three_free".to_string(),
            }],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_async_method() {
        let cls = SwiftClass {
            name: "HttpClient".to_string(),
            ffi_free: "boltffi_http_client_free".to_string(),
            constructors: vec![],
            methods: vec![SwiftMethod {
                name: "fetch".to_string(),
                mode: SwiftCallMode::Async {
                    start: "boltffi_http_client_fetch_start".to_string(),
                    poll: "boltffi_http_client_fetch_poll".to_string(),
                    complete: "boltffi_http_client_fetch_complete".to_string(),
                    cancel: "boltffi_http_client_fetch_cancel".to_string(),
                    free: "boltffi_http_client_fetch_free".to_string(),
                    result: Box::new(SwiftAsyncResult::Encoded {
                        swift_type: "Data".to_string(),
                        ok_type: None,
                        decode: read_bytes(offset("pos")),
                        throws: false,
                        err_decode: read_empty(),
                        err_is_string: false,
                    }),
                },
                params: vec![SwiftParam {
                    label: None,
                    name: "url".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                }],
                returns: SwiftReturn::Void,
                is_static: false,
                doc: None,
            }],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_fallible_constructor() {
        let cls = SwiftClass {
            name: "Connection".to_string(),
            ffi_free: "boltffi_connection_free".to_string(),
            constructors: vec![SwiftConstructor::Designated {
                ffi_symbol: "boltffi_connection_open".to_string(),
                params: vec![SwiftParam {
                    label: None,
                    name: "url".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                }],
                is_fallible: true,
                doc: None,
            }],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_sync_function_with_multiple_string_params() {
        let func = SwiftFunction {
            name: "concat".to_string(),
            mode: SwiftCallMode::Sync {
                symbol: "boltffi_concat".to_string(),
            },
            params: vec![
                SwiftParam {
                    label: None,
                    name: "a".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                },
                SwiftParam {
                    label: None,
                    name: "b".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                },
            ],
            returns: SwiftReturn::FromWireBuffer {
                swift_type: "String".to_string(),
                decode: read_string(offset("pos")),
                encode: write_string("value"),
            },
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_static_method() {
        let cls = SwiftClass {
            name: "Logger".to_string(),
            ffi_free: "boltffi_logger_free".to_string(),
            constructors: vec![],
            methods: vec![SwiftMethod {
                name: "getDefault".to_string(),
                mode: SwiftCallMode::Sync {
                    symbol: "boltffi_logger_get_default".to_string(),
                },
                params: vec![],
                returns: SwiftReturn::Handle {
                    class_name: "Logger".to_string(),
                    nullable: false,
                },
                is_static: true,
                doc: None,
            }],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_callback_with_async_method() {
        let callback = SwiftCallback {
            protocol_name: "AsyncHandler".to_string(),
            wrapper_class: "AsyncHandlerWrapper".to_string(),
            vtable_var: "asyncHandlerVtable".to_string(),
            vtable_type: "AsyncHandlerVtable".to_string(),
            bridge_name: "AsyncHandlerBridge".to_string(),
            register_fn: "boltffi_register_async_handler".to_string(),
            create_fn: "boltffi_create_async_handler".to_string(),
            methods: vec![SwiftCallbackMethod {
                swift_name: "onComplete".to_string(),
                ffi_name: "on_complete".to_string(),
                params: vec![SwiftCallbackParam {
                    label: "result".to_string(),
                    swift_type: "String".to_string(),
                    call_arg: "result".to_string(),
                    ffi_args: vec!["resultPtr".to_string(), "resultLen".to_string()],
                    decode_prelude: Some("let result = String(decoding: UnsafeBufferPointer(start: resultPtr, count: Int(resultLen)), as: UTF8.self)".to_string()),
                }],
                returns: SwiftReturn::Void,
                is_async: true,
                has_out_param: false,
                doc: None,
            }],
            doc: None,
        };
        insta::assert_snapshot!(render_callback(&callback));
    }

    #[test]
    fn snapshot_record_with_optional_field() {
        let record = SwiftRecord {
            class_name: "UserProfile".to_string(),
            fields: vec![
                field("name", "String", read_string(offset("pos")), write_string("name"), None, None),
                field(
                    "bio",
                    "String?",
                    read_option(offset("pos"), read_string(offset("pos"))),
                    write_option("bio", write_string("v")),
                    None,
                    None,
                ),
            ],
            is_blittable: false,
            blittable_size: None,
            doc: None,
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_record_with_array_field() {
        let record = SwiftRecord {
            class_name: "Team".to_string(),
            fields: vec![
                field("name", "String", read_string(offset("pos")), write_string("name"), None, None),
                field(
                    "members",
                    "[String]",
                    read_vec(offset("pos"), TypeExpr::String, read_string(offset("pos")), VecLayout::Encoded),
                    write_vec("members", write_string("item"), &TypeExpr::String, VecLayout::Encoded),
                    None,
                    None,
                ),
            ],
            is_blittable: false,
            blittable_size: None,
            doc: None,
        };
        insta::assert_snapshot!(render_record(&record));
    }

    #[test]
    fn snapshot_sync_function_returning_optional() {
        let func = SwiftFunction {
            name: "findUser".to_string(),
            mode: SwiftCallMode::Sync {
                symbol: "boltffi_find_user".to_string(),
            },
            params: vec![SwiftParam {
                label: None,
                name: "id".to_string(),
                swift_type: "Int64".to_string(),
                conversion: SwiftConversion::Direct,
            }],
            returns: SwiftReturn::FromWireBuffer {
                swift_type: "String?".to_string(),
                decode: read_option(offset("pos"), read_string(offset("pos"))),
                encode: write_option("value", write_string("v")),
            },
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }

    #[test]
    fn snapshot_class_with_nullable_handle_return() {
        let cls = SwiftClass {
            name: "Cache".to_string(),
            ffi_free: "boltffi_cache_free".to_string(),
            constructors: vec![],
            methods: vec![SwiftMethod {
                name: "get".to_string(),
                mode: SwiftCallMode::Sync {
                    symbol: "boltffi_cache_get".to_string(),
                },
                params: vec![SwiftParam {
                    label: None,
                    name: "key".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                }],
                returns: SwiftReturn::Handle {
                    class_name: "CacheEntry".to_string(),
                    nullable: true,
                },
                is_static: false,
                doc: None,
            }],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
        };
        insta::assert_snapshot!(render_class(&cls, "boltffi"));
    }

    #[test]
    fn snapshot_enum_with_associated_optional() {
        let e = SwiftEnum {
            name: "SearchResult".to_string(),
            variants: vec![
                SwiftVariant {
                    swift_name: "found".to_string(),
                    discriminant: 0,
                    payload: SwiftVariantPayload::Tuple(vec![field("0", "String", read_string(offset("pos")), write_string("0"), None, None)]),
                    doc: None,
                },
                SwiftVariant {
                    swift_name: "notFound".to_string(),
                    discriminant: 1,
                    payload: SwiftVariantPayload::Unit,
                    doc: None,
                },
            ],
            style: SwiftEnumStyle::Data,
            is_error: false,
            doc: None,
        };
        insta::assert_snapshot!(render_enum(&e));
    }

    #[test]
    fn snapshot_async_function_with_multiple_params() {
        let func = SwiftFunction {
            name: "uploadFile".to_string(),
            mode: SwiftCallMode::Async {
                start: "boltffi_upload_file_start".to_string(),
                poll: "boltffi_upload_file_poll".to_string(),
                complete: "boltffi_upload_file_complete".to_string(),
                cancel: "boltffi_upload_file_cancel".to_string(),
                free: "boltffi_upload_file_free".to_string(),
                result: Box::new(SwiftAsyncResult::Encoded {
                    swift_type: "String".to_string(),
                    ok_type: None,
                    decode: read_string(offset("pos")),
                    throws: false,
                    err_decode: read_empty(),
                    err_is_string: false,
                }),
            },
            params: vec![
                SwiftParam {
                    label: None,
                    name: "path".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                },
                SwiftParam {
                    label: Some("to".to_string()),
                    name: "destination".to_string(),
                    swift_type: "String".to_string(),
                    conversion: SwiftConversion::ToString,
                },
            ],
            returns: SwiftReturn::Void,
            doc: None,
        };
        insta::assert_snapshot!(render_function(&func, "boltffi"));
    }
}
