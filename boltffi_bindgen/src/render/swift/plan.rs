use crate::ir::abi::SpanLengthUnit;
use crate::ir::ops::{
    OffsetExpr, ReadOp, ReadSeq, ValueExpr, WriteOp, WriteSeq, remap_root_in_seq,
};
use crate::render::swift::emit;
use boltffi_ffi_rules::callable::ExecutionKind;
use boltffi_ffi_rules::transport::{ScalarReturnStrategy, ValueReturnStrategy};

#[derive(Debug, Clone)]
pub struct CompositeFieldMapping {
    pub swift_name: String,
    pub c_name: String,
}

#[derive(Debug, Clone)]
pub struct DirectBufferCompositeMapping {
    pub swift_record_type: String,
    pub fields: Vec<CompositeFieldMapping>,
}

#[derive(Debug, Clone)]
pub enum SwiftCallMode {
    Sync {
        symbol: String,
    },
    Async {
        start: String,
        poll: String,
        complete: String,
        cancel: String,
        free: String,
        result: Box<SwiftAsyncResult>,
    },
}

impl SwiftCallMode {
    pub fn is_async(&self) -> bool {
        matches!(self, Self::Async { .. })
    }

    pub fn async_result(&self) -> Option<&SwiftAsyncResult> {
        match self {
            Self::Async { result, .. } => Some(result),
            Self::Sync { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SwiftAsyncResult {
    Void,
    Direct {
        swift_type: String,
        conversion: SwiftAsyncConversion,
    },
    DirectBuffer {
        swift_type: String,
        element_swift_type: String,
        composite_mapping: Option<DirectBufferCompositeMapping>,
        enum_mapping: Option<String>,
    },
    Encoded {
        swift_type: String,
        ok_type: Option<String>,
        decode: ReadSeq,
        throws: bool,
        err_decode: ReadSeq,
        err_is_string: bool,
    },
}

impl SwiftAsyncResult {
    fn ok_buffer_expr(ok_type: Option<&str>, ok: &ReadSeq) -> String {
        if matches!(ok_type, Some("Void")) {
            "()".to_string()
        } else {
            emit::emit_read_value_at(ok, "pos")
        }
    }

    fn ok_reader_expr(ok_type: Option<&str>, ok: &ReadSeq) -> String {
        if matches!(ok_type, Some("Void")) {
            "()".to_string()
        } else {
            emit::emit_reader_read(ok)
        }
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_direct_buffer(&self) -> bool {
        matches!(self, Self::DirectBuffer { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::Encoded { .. })
    }

    pub fn direct_buffer_element_type(&self) -> Option<&str> {
        match self {
            Self::DirectBuffer {
                element_swift_type, ..
            } => Some(element_swift_type),
            _ => None,
        }
    }

    pub fn direct_buffer_composite_mapping(&self) -> Option<&DirectBufferCompositeMapping> {
        match self {
            Self::DirectBuffer {
                composite_mapping, ..
            } => composite_mapping.as_ref(),
            _ => None,
        }
    }

    pub fn direct_buffer_enum_mapping(&self) -> Option<&str> {
        match self {
            Self::DirectBuffer {
                enum_mapping: Some(e),
                ..
            } => Some(e.as_str()),
            _ => None,
        }
    }

    pub fn direct_buffer_is_data(&self) -> bool {
        match self {
            Self::DirectBuffer {
                swift_type,
                element_swift_type,
                composite_mapping,
                enum_mapping,
            } => {
                swift_type == "Data"
                    && element_swift_type == "UInt8"
                    && composite_mapping.is_none()
                    && enum_mapping.is_none()
            }
            _ => false,
        }
    }

    pub fn swift_type(&self) -> Option<&str> {
        match self {
            Self::Void => None,
            Self::Direct { swift_type, .. } | Self::DirectBuffer { swift_type, .. } => {
                Some(swift_type)
            }
            Self::Encoded {
                throws: true,
                ok_type: Some(ok),
                ..
            } => Some(ok),
            Self::Encoded { swift_type, .. } => Some(swift_type),
        }
    }

    pub fn future_type(&self) -> &str {
        match self {
            Self::Void => "Void",
            Self::Direct { swift_type, .. } | Self::DirectBuffer { swift_type, .. } => swift_type,
            Self::Encoded {
                throws: true,
                ok_type: Some(ok),
                ..
            } => ok,
            Self::Encoded { swift_type, .. } => swift_type,
        }
    }

    pub fn throws(&self) -> bool {
        matches!(self, Self::Encoded { throws: true, .. })
    }

    pub fn decode_expr(&self) -> Option<String> {
        match self {
            Self::Encoded {
                throws: true,
                decode,
                err_decode,
                err_is_string,
                ok_type,
                ..
            } => match decode.ops.first() {
                Some(ReadOp::Result { ok, .. }) => {
                    let ok_expr = Self::ok_buffer_expr(ok_type.as_deref(), ok);
                    Some(emit::emit_result_ok_throw_with_ok_expr(
                        &ok_expr,
                        err_decode,
                        *err_is_string,
                    ))
                }
                _ => Some(emit::emit_read_value_at(decode, "0")),
            },
            Self::Encoded { decode, .. } => Some(emit::emit_read_value_at(decode, "0")),
            _ => None,
        }
    }

    pub fn reader_decode_expr(&self) -> Option<String> {
        match self {
            Self::Encoded {
                throws: true,
                decode,
                err_is_string,
                ok_type,
                ..
            } => match decode.ops.first() {
                Some(ReadOp::Result { ok, err, .. }) => {
                    let ok_read = Self::ok_reader_expr(ok_type.as_deref(), ok);
                    let err_read = emit::emit_reader_read(err);
                    let err_body = if *err_is_string {
                        format!("FfiError(message: {})", err_read)
                    } else {
                        err_read
                    };
                    Some(format!(
                        "try {{ let tag = reader.readU8(); if tag == 0 {{ return {} }} else {{ throw {} }} }}()",
                        ok_read, err_body
                    ))
                }
                _ => Some(emit::emit_reader_read(decode)),
            },
            Self::Encoded { decode, .. } => Some(emit::emit_reader_read(decode)),
            _ => None,
        }
    }

    pub fn direct_return_expr(&self, raw_var: &str) -> Option<String> {
        match self {
            Self::Direct { conversion, .. } => Some(match conversion {
                SwiftAsyncConversion::None => raw_var.to_string(),
                SwiftAsyncConversion::Composite(mapping) => format!(
                    "{}({})",
                    mapping.swift_record_type,
                    mapping
                        .fields
                        .iter()
                        .map(|field| format!("{}: {}.{}", field.swift_name, raw_var, field.c_name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                SwiftAsyncConversion::Handle {
                    class_name,
                    nullable,
                } => {
                    if *nullable {
                        format!("{}.map {{ {}(handle: $0) }}", raw_var, class_name)
                    } else {
                        format!("{}(handle: {}!)", class_name, raw_var)
                    }
                }
                SwiftAsyncConversion::Callback { protocol, nullable } => {
                    if *nullable {
                        format!(
                            "{}.handle == 0 ? nil : {}Bridge.wrap({})",
                            raw_var, protocol, raw_var
                        )
                    } else {
                        format!("{}Bridge.wrap({})", protocol, raw_var)
                    }
                }
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SwiftAsyncConversion {
    None,
    Composite(DirectBufferCompositeMapping),
    Handle { class_name: String, nullable: bool },
    Callback { protocol: String, nullable: bool },
}

#[derive(Debug, Clone)]
pub struct SwiftModule {
    pub imports: Vec<String>,
    pub custom_types: Vec<SwiftCustomType>,
    pub records: Vec<SwiftRecord>,
    pub enums: Vec<SwiftEnum>,
    pub classes: Vec<SwiftClass>,
    pub callbacks: Vec<SwiftCallback>,
    pub functions: Vec<SwiftFunction>,
}

#[derive(Debug, Clone)]
pub struct SwiftCustomType {
    pub alias_name: String,
    pub target_type: String,
    pub native_mapping: Option<SwiftNativeMapping>,
}

#[derive(Debug, Clone)]
pub struct SwiftNativeMapping {
    pub native_type: String,
    pub decode_expr: String,
    pub encode_expr: String,
}

impl SwiftModule {
    pub fn has_async(&self) -> bool {
        self.functions.iter().any(|f| f.mode.is_async())
            || self
                .classes
                .iter()
                .any(|c| c.methods.iter().any(|m| m.mode.is_async()))
    }

    pub fn has_streams(&self) -> bool {
        self.classes.iter().any(|c| !c.streams.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct SwiftRecord {
    pub class_name: String,
    pub fields: Vec<SwiftField>,
    pub is_blittable: bool,
    pub is_error: bool,
    pub blittable_size: Option<usize>,
    pub constructors: Vec<SwiftConstructor>,
    pub methods: Vec<SwiftMethod>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftField {
    pub swift_name: String,
    pub swift_type: String,
    pub default_expr: Option<String>,
    pub decode: ReadSeq,
    pub encode: WriteSeq,
    pub doc: Option<String>,
    pub c_offset: Option<usize>,
    pub native_conversion: Option<SwiftNativeConversion>,
}

#[derive(Debug, Clone)]
pub struct SwiftNativeConversion {
    pub decode_wrapper: String,
    pub encode_wrapper: String,
}

impl SwiftField {
    pub fn wire_decode_inline(&self) -> String {
        emit::emit_read_inline(&self.decode, "pos")
    }

    pub fn wire_size_expr(&self) -> String {
        emit::emit_size_expr(&self.encode.size)
    }

    pub fn has_fixed_size(&self) -> bool {
        let size_expr = self.wire_size_expr();
        size_expr.chars().all(|c| c.is_ascii_digit())
    }

    pub fn wire_encode(&self) -> String {
        emit::emit_write_data(&self.encode)
    }

    pub fn wire_encode_bytes(&self) -> String {
        emit::emit_write_bytes(&self.encode)
    }

    pub fn decode_at_offset(&self, base: &str) -> String {
        emit::emit_read_with_offset(&self.decode, "offset", base)
    }

    pub fn encode_at_offset(&self) -> String {
        emit::emit_write_data(&self.encode)
    }

    pub fn wire_reader_decode(&self) -> String {
        let base_decode = emit::emit_reader_read(&self.decode);
        match &self.native_conversion {
            Some(conv) => conv.decode_wrapper.replace("$0", &base_decode),
            None => base_decode,
        }
    }

    pub fn wire_writer_encode(&self) -> String {
        match &self.native_conversion {
            Some(conv) => {
                let converted_value = conv
                    .encode_wrapper
                    .replace("$0", &format!("self.{}", self.swift_name));
                let base_encode = emit::emit_writer_write(&self.encode);
                base_encode.replace(&format!("self.{}", self.swift_name), &converted_value)
            }
            None => emit::emit_writer_write(&self.encode),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwiftEnum {
    pub name: String,
    pub variants: Vec<SwiftVariant>,
    pub style: SwiftEnumStyle,
    pub c_style_tag_type: Option<crate::ir::types::PrimitiveType>,
    pub is_error: bool,
    pub constructors: Vec<SwiftConstructor>,
    pub methods: Vec<SwiftMethod>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum SwiftEnumStyle {
    CStyle,
    Data,
}

impl SwiftEnum {
    pub fn is_c_style(&self) -> bool {
        matches!(self.style, SwiftEnumStyle::CStyle)
    }
}

#[derive(Debug, Clone)]
pub struct SwiftVariant {
    pub swift_name: String,
    pub discriminant: i128,
    pub payload: SwiftVariantPayload,
    pub doc: Option<String>,
}

impl SwiftVariant {
    pub fn is_unit(&self) -> bool {
        matches!(self.payload, SwiftVariantPayload::Unit)
    }

    pub fn is_single_tuple(&self) -> bool {
        match &self.payload {
            SwiftVariantPayload::Tuple(fields) => fields.len() == 1,
            SwiftVariantPayload::Struct(fields) => {
                fields.len() == 1 && fields[0].swift_name.chars().all(|c| c.is_ascii_digit())
            }
            _ => false,
        }
    }

    pub fn fields(&self) -> &[SwiftField] {
        match &self.payload {
            SwiftVariantPayload::Unit => &[],
            SwiftVariantPayload::Tuple(fields) | SwiftVariantPayload::Struct(fields) => fields,
        }
    }

    fn single_tuple_field(&self) -> Option<&SwiftField> {
        match &self.payload {
            SwiftVariantPayload::Tuple(fields) if fields.len() == 1 => Some(&fields[0]),
            SwiftVariantPayload::Struct(fields)
                if fields.len() == 1
                    && fields[0].swift_name.chars().all(|c| c.is_ascii_digit()) =>
            {
                Some(&fields[0])
            }
            _ => None,
        }
    }

    pub fn tuple_value_decode(&self) -> String {
        self.single_tuple_field()
            .map(|f| f.wire_decode_inline())
            .unwrap_or_default()
    }

    pub fn tuple_value_size(&self) -> String {
        self.single_tuple_field()
            .map(|f| f.wire_size_expr())
            .unwrap_or_default()
    }

    pub fn tuple_value_encode(&self) -> String {
        self.single_tuple_field()
            .map(|f| f.wire_encode())
            .unwrap_or_default()
    }

    pub fn tuple_value_encode_bytes(&self) -> String {
        self.single_tuple_field()
            .map(|f| f.wire_encode_bytes())
            .unwrap_or_default()
    }

    pub fn tuple_value_reader_decode(&self) -> String {
        self.single_tuple_field()
            .map(|f| f.wire_reader_decode())
            .unwrap_or_default()
    }

    pub fn tuple_value_writer_encode(&self) -> String {
        self.single_tuple_field()
            .map(|f| f.wire_writer_encode())
            .unwrap_or_default()
    }

    pub fn all_fields_fixed_size(&self) -> bool {
        self.fields().iter().all(|f| f.has_fixed_size())
    }

    pub fn tuple_value_fixed_size(&self) -> bool {
        self.single_tuple_field()
            .map(|f| f.has_fixed_size())
            .unwrap_or(true)
    }
}

#[derive(Debug, Clone)]
pub enum SwiftVariantPayload {
    Unit,
    Tuple(Vec<SwiftField>),
    Struct(Vec<SwiftField>),
}

#[derive(Debug, Clone)]
pub struct SwiftClass {
    pub name: String,
    pub ffi_free: String,
    pub constructors: Vec<SwiftConstructor>,
    pub methods: Vec<SwiftMethod>,
    pub streams: Vec<SwiftStream>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftStream {
    pub name: String,
    pub mode: SwiftStreamMode,
    pub item_type: String,
    pub item_delivery: SwiftStreamItemDelivery,
    pub subscribe: String,
    pub poll: String,
    pub pop_batch: String,
    pub wait: String,
    pub unsubscribe: String,
    pub free: String,
    pub free_buf: String,
    pub atomic_cas: String,
}

#[derive(Debug, Clone)]
pub enum SwiftStreamItemDelivery {
    WireEncoded {
        item_decode: ReadSeq,
    },
    Direct {
        c_element_type: String,
        item_expr_template: String,
    },
}

impl SwiftStreamItemDelivery {
    pub fn reader_decode_expr(&self) -> Option<String> {
        match self {
            Self::WireEncoded { item_decode } => Some(emit::emit_reader_read(item_decode)),
            Self::Direct { .. } => None,
        }
    }

    pub fn direct_element_type(&self) -> Option<&str> {
        match self {
            Self::WireEncoded { .. } => None,
            Self::Direct { c_element_type, .. } => Some(c_element_type),
        }
    }

    pub fn direct_item_expr(&self, raw_item_name: &str) -> Option<String> {
        match self {
            Self::WireEncoded { .. } => None,
            Self::Direct {
                item_expr_template, ..
            } => Some(item_expr_template.replace("$0", raw_item_name)),
        }
    }
}

fn uses_offset_in_read_seq(seq: &ReadSeq) -> bool {
    seq.ops.iter().any(uses_offset_in_read_op)
}

fn uses_offset_in_read_op(op: &ReadOp) -> bool {
    match op {
        ReadOp::Primitive { offset, .. } => offset_uses(offset),
        ReadOp::String { offset } => offset_uses(offset),
        ReadOp::Option { tag_offset, some } => {
            offset_uses(tag_offset) || uses_offset_in_read_seq(some)
        }
        ReadOp::Vec {
            len_offset,
            element,
            ..
        } => offset_uses(len_offset) || uses_offset_in_read_seq(element),
        ReadOp::Record { offset, .. } => offset_uses(offset),
        ReadOp::Enum { offset, .. } => offset_uses(offset),
        ReadOp::Result {
            tag_offset,
            ok,
            err,
        } => offset_uses(tag_offset) || uses_offset_in_read_seq(ok) || uses_offset_in_read_seq(err),
        ReadOp::Builtin { offset, .. } => offset_uses(offset),
        ReadOp::Custom { underlying, .. } => uses_offset_in_read_seq(underlying),
    }
}

fn offset_uses(offset: &OffsetExpr) -> bool {
    matches!(offset, OffsetExpr::Base | OffsetExpr::BasePlus(_))
}

#[derive(Debug, Clone)]
pub enum SwiftStreamMode {
    Async,
    Batch {
        class_name: String,
        method_name_pascal: String,
    },
    Callback {
        class_name: String,
        method_name_pascal: String,
    },
}

#[derive(Debug, Clone)]
pub enum SwiftConstructor {
    Designated {
        ffi_symbol: String,
        closure_return_type: String,
        params: Vec<SwiftParam>,
        is_fallible: bool,
        is_optional: bool,
        throw_decode_expr: Option<String>,
        doc: Option<String>,
    },
    Factory {
        name: String,
        ffi_symbol: String,
        is_fallible: bool,
        is_optional: bool,
        throw_decode_expr: Option<String>,
        doc: Option<String>,
    },
    Convenience {
        name: String,
        ffi_symbol: String,
        closure_return_type: String,
        params: Vec<SwiftParam>,
        is_fallible: bool,
        is_optional: bool,
        throw_decode_expr: Option<String>,
        doc: Option<String>,
    },
}

impl SwiftConstructor {
    pub fn is_designated(&self) -> bool {
        matches!(self, Self::Designated { .. })
    }

    pub fn is_factory(&self) -> bool {
        matches!(self, Self::Factory { .. })
    }

    pub fn is_convenience(&self) -> bool {
        matches!(self, Self::Convenience { .. })
    }

    pub fn ffi_symbol(&self) -> &str {
        match self {
            Self::Designated { ffi_symbol, .. }
            | Self::Factory { ffi_symbol, .. }
            | Self::Convenience { ffi_symbol, .. } => ffi_symbol,
        }
    }

    fn closure_return_type(&self) -> &str {
        match self {
            Self::Designated {
                closure_return_type,
                ..
            }
            | Self::Convenience {
                closure_return_type,
                ..
            } => closure_return_type,
            Self::Factory { .. } => "Void",
        }
    }

    pub fn params(&self) -> &[SwiftParam] {
        match self {
            Self::Designated { params, .. } | Self::Convenience { params, .. } => params,
            Self::Factory { .. } => &[],
        }
    }

    pub fn is_fallible(&self) -> bool {
        match self {
            Self::Designated { is_fallible, .. }
            | Self::Factory { is_fallible, .. }
            | Self::Convenience { is_fallible, .. } => *is_fallible,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            Self::Designated { is_optional, .. }
            | Self::Factory { is_optional, .. }
            | Self::Convenience { is_optional, .. } => *is_optional,
        }
    }

    pub fn throw_decode_expr(&self) -> Option<&str> {
        match self {
            Self::Designated {
                throw_decode_expr, ..
            }
            | Self::Factory {
                throw_decode_expr, ..
            }
            | Self::Convenience {
                throw_decode_expr, ..
            } => throw_decode_expr.as_deref(),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Designated { .. } => None,
            Self::Factory { name, .. } | Self::Convenience { name, .. } => Some(name),
        }
    }

    pub fn has_wrappers(&self) -> bool {
        self.params().iter().any(|p| p.needs_wrapper())
    }

    pub fn needs_closure_wrap(&self) -> bool {
        self.params().iter().any(|p| p.needs_closure_wrap())
    }

    pub fn closure_wrappers(&self) -> Vec<String> {
        closure_wrappers(self.params())
    }

    pub fn annotated_closure_wrappers(&self) -> Vec<String> {
        let mut wrappers = self.closure_wrappers();
        if let Some(first) = wrappers.first_mut()
            && let Some(in_pos) = first.rfind(" in")
        {
            first.replace_range(in_pos.., &format!(" -> {} in", self.closure_return_type()));
        }
        wrappers
    }

    pub fn call_expr(&self) -> String {
        ffi_call_expr(self.ffi_symbol(), &[], self.params())
    }

    pub fn doc(&self) -> &Option<String> {
        match self {
            Self::Designated { doc, .. }
            | Self::Factory { doc, .. }
            | Self::Convenience { doc, .. } => doc,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValueSelfParam {
    pub ffi_args: Vec<String>,
    pub wrapper_code: Option<String>,
    pub is_mutating: bool,
}

#[derive(Debug, Clone)]
pub struct SwiftMethod {
    pub name: String,
    pub mode: SwiftCallMode,
    pub params: Vec<SwiftParam>,
    pub returns: SwiftReturn,
    pub is_static: bool,
    pub value_self: Option<ValueSelfParam>,
    pub mutating_void: bool,
    pub doc: Option<String>,
}

impl SwiftMethod {
    pub fn needs_handle(&self) -> bool {
        !self.is_static && self.value_self.is_none()
    }

    pub fn is_mutating(&self) -> bool {
        self.value_self.as_ref().is_some_and(|rs| rs.is_mutating)
    }

    pub fn is_async(&self) -> bool {
        self.mode.is_async()
    }

    pub fn needs_closure_wrap(&self) -> bool {
        self.params.iter().any(|p| p.needs_closure_wrap())
    }

    pub fn closure_wrappers(&self) -> Vec<String> {
        closure_wrappers(&self.params)
    }

    fn prefix_args(&self) -> Vec<&str> {
        if self.needs_handle() {
            vec!["handle"]
        } else if let Some(rs) = &self.value_self {
            rs.ffi_args.iter().map(String::as_str).collect()
        } else {
            vec![]
        }
    }

    pub fn start_call_expr(&self) -> String {
        match &self.mode {
            SwiftCallMode::Async { start, .. } => {
                ffi_call_expr(start, &self.prefix_args(), &self.params)
            }
            SwiftCallMode::Sync { .. } => String::new(),
        }
    }

    pub fn sync_call_expr(&self) -> String {
        match &self.mode {
            SwiftCallMode::Sync { symbol } => {
                ffi_call_expr(symbol, &self.prefix_args(), &self.params)
            }
            SwiftCallMode::Async { .. } => String::new(),
        }
    }

    pub fn closure_depth(&self) -> usize {
        self.params
            .iter()
            .filter(|p| p.needs_closure_wrap())
            .count()
    }

    pub fn method_body_indent(&self) -> String {
        "    ".repeat(self.closure_depth() + 2)
    }

    pub fn sync_closure_opens(&self) -> Vec<String> {
        let has_return = !self.returns.is_void();
        let return_prefix = if has_return && self.returns.is_throws() {
            "return try "
        } else if has_return {
            "return "
        } else {
            ""
        };
        self.params
            .iter()
            .filter_map(|p| p.closure_wrap_open())
            .enumerate()
            .map(|(i, open)| {
                let indent = "    ".repeat(i + 2);
                format!("{}{}{}", indent, return_prefix, open)
            })
            .collect()
    }

    pub fn sync_closure_closes(&self) -> Vec<String> {
        let depth = self.closure_depth();
        (0..depth)
            .rev()
            .map(|i| format!("{}}}", "    ".repeat(i + 2)))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct SwiftCallback {
    pub protocol_name: String,
    pub wrapper_class: String,
    pub vtable_var: String,
    pub vtable_type: String,
    pub bridge_name: String,
    pub register_fn: String,
    pub create_fn: String,
    pub supports_foreign_wrap: bool,
    pub methods: Vec<SwiftCallbackMethod>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftCallbackMethod {
    pub swift_name: String,
    pub ffi_name: String,
    pub params: Vec<SwiftCallbackParam>,
    pub returns: SwiftReturn,
    pub execution_kind: ExecutionKind,
    pub has_out_param: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftCallbackParam {
    pub label: String,
    pub swift_type: String,
    pub call_arg: String,
    pub ffi_args: Vec<String>,
    pub proxy_ffi_args: Vec<String>,
    pub proxy_wrapper_code: Option<String>,
    pub proxy_closure_wrap_open: Option<String>,
    pub proxy_closure_wrap_close: Option<String>,
    pub decode_prelude: Option<String>,
}

impl SwiftCallbackMethod {
    pub fn is_async(&self) -> bool {
        self.execution_kind == ExecutionKind::Async
    }

    pub fn has_return(&self) -> bool {
        !self.returns.is_void()
    }

    pub fn throws(&self) -> bool {
        self.returns.is_throws()
    }

    pub fn return_type(&self) -> Option<String> {
        self.returns.swift_type()
    }

    pub fn wire_encoded_return(&self) -> bool {
        self.returns.is_wire_encoded()
    }

    pub fn async_callback_c_type(&self) -> String {
        if self.wire_encoded_return() {
            "(@convention(c) (UInt64, UnsafePointer<UInt8>?, UInt, FfiStatus) -> Void)?".to_string()
        } else if let Some(ret) = self.return_type() {
            format!("(@convention(c) (UInt64, {}, FfiStatus) -> Void)?", ret)
        } else {
            "(@convention(c) (UInt64, FfiStatus) -> Void)?".to_string()
        }
    }

    pub fn wire_return_encode(&self) -> Option<String> {
        self.encoded_return_encode().map(|encode| {
            let effective_encode = if self.throws() {
                throws_success_encode(encode)
            } else {
                encode.clone()
            };
            let writer_body = emit::emit_writer_write(&effective_encode);
            let discriminant = if self.throws() {
                "writer.writeU8(0); "
            } else {
                ""
            };
            format!(
                "let encoded = ({{ var writer = WireWriter(); {}{}; return writer.finalize() }})()",
                discriminant, writer_body
            )
        })
    }

    pub fn err_type(&self) -> Option<&str> {
        match &self.returns {
            SwiftReturn::Throws { err_type, .. } => Some(err_type),
            _ => None,
        }
    }

    pub fn wire_err_encode(&self) -> Option<String> {
        match &self.returns {
            SwiftReturn::Throws {
                err_encode: Some(encode),
                ..
            } => {
                let writer_body = emit::emit_writer_write(encode);
                Some(format!(
                    "let encoded = ({{ var writer = WireWriter(); writer.writeU8(1); {}; return writer.finalize() }})()",
                    writer_body
                ))
            }
            _ => None,
        }
    }

    fn encoded_return_encode(&self) -> Option<&WriteSeq> {
        match &self.returns {
            SwiftReturn::FromWireBuffer { encode, .. } => Some(encode),
            SwiftReturn::Throws { ok, .. } => match ok.as_ref() {
                SwiftReturn::FromWireBuffer { encode, .. } => Some(encode),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn direct_out_expr(&self) -> &str {
        if self.returns.is_c_style_enum() {
            "result.cValue"
        } else {
            "result"
        }
    }

    pub fn proxy_out_ffi_type(&self) -> Option<String> {
        match &self.returns {
            SwiftReturn::Direct { swift_type } => Some(swift_type.clone()),
            SwiftReturn::CStyleEnumFromRawValue { swift_type } => {
                Some(format!("{}.RawValue", swift_type))
            }
            SwiftReturn::FromComposite { c_type, .. } => Some(c_type.clone()),
            SwiftReturn::Handle {
                class_name,
                nullable,
            } => {
                if *nullable {
                    Some(format!("{}?.Handle", class_name))
                } else {
                    Some(format!("{}.Handle", class_name))
                }
            }
            SwiftReturn::Callback {
                protocol_name,
                nullable,
            } => {
                if *nullable {
                    Some(format!("(any {})?", protocol_name))
                } else {
                    Some(format!("any {}", protocol_name))
                }
            }
            SwiftReturn::Void
            | SwiftReturn::FromWireBuffer { .. }
            | SwiftReturn::FromDirectBuffer { .. }
            | SwiftReturn::Throws { .. } => None,
        }
    }

    pub fn proxy_wrapper_codes(&self) -> Vec<&str> {
        self.params
            .iter()
            .filter_map(|param| param.proxy_wrapper_code.as_deref())
            .collect()
    }

    pub fn proxy_closure_opens(&self) -> Vec<&str> {
        self.params
            .iter()
            .filter_map(|param| param.proxy_closure_wrap_open.as_deref())
            .collect()
    }

    pub fn proxy_outer_closure_open(&self) -> Option<&str> {
        self.params
            .iter()
            .find_map(|param| param.proxy_closure_wrap_open.as_deref())
    }

    pub fn proxy_inner_closure_opens(&self) -> Vec<&str> {
        let mut opened_closures = self
            .params
            .iter()
            .filter_map(|param| param.proxy_closure_wrap_open.as_deref());
        let _outer_closure = opened_closures.next();
        opened_closures.collect()
    }

    pub fn proxy_closure_closes(&self) -> Vec<&str> {
        self.params
            .iter()
            .rev()
            .filter_map(|param| param.proxy_closure_wrap_close.as_deref())
            .collect()
    }
}

fn throws_success_encode(encode: &WriteSeq) -> WriteSeq {
    match encode.ops.first() {
        Some(WriteOp::Result { ok, .. }) => {
            remap_root_in_seq(ok.as_ref(), ValueExpr::Var("result".to_string()))
        }
        _ => encode.clone(),
    }
}

#[derive(Debug, Clone)]
pub struct SwiftFunction {
    pub name: String,
    pub mode: SwiftCallMode,
    pub params: Vec<SwiftParam>,
    pub returns: SwiftReturn,
    pub doc: Option<String>,
}

pub fn ffi_call_expr(symbol: &str, prefix_args: &[&str], params: &[SwiftParam]) -> String {
    let args = prefix_args
        .iter()
        .map(|s| s.to_string())
        .chain(params.iter().map(|p| p.ffi_arg()));
    format!("{}({})", symbol, args.collect::<Vec<_>>().join(", "))
}

pub fn closure_wrappers(params: &[SwiftParam]) -> Vec<String> {
    params
        .iter()
        .filter_map(|p| p.closure_wrap_open())
        .collect()
}

impl SwiftFunction {
    pub fn is_async(&self) -> bool {
        self.mode.is_async()
    }

    pub fn needs_closure_wrap(&self) -> bool {
        self.params.iter().any(|p| p.needs_closure_wrap())
    }

    pub fn closure_wrappers(&self) -> Vec<String> {
        closure_wrappers(&self.params)
    }

    pub fn start_call_expr(&self) -> String {
        match &self.mode {
            SwiftCallMode::Async { start, .. } => ffi_call_expr(start, &[], &self.params),
            SwiftCallMode::Sync { .. } => String::new(),
        }
    }

    pub fn sync_call_expr(&self) -> String {
        match &self.mode {
            SwiftCallMode::Sync { symbol } => ffi_call_expr(symbol, &[], &self.params),
            SwiftCallMode::Async { .. } => String::new(),
        }
    }

    pub fn closure_depth(&self) -> usize {
        self.params
            .iter()
            .filter(|p| p.needs_closure_wrap())
            .count()
    }

    pub fn body_indent(&self) -> String {
        "    ".repeat(self.closure_depth() + 1)
    }

    pub fn sync_closure_opens(&self) -> Vec<String> {
        let has_return = !self.returns.is_void();
        let return_prefix = if has_return && self.returns.is_throws() {
            "return try "
        } else if has_return {
            "return "
        } else {
            ""
        };
        self.params
            .iter()
            .filter_map(|p| p.closure_wrap_open())
            .enumerate()
            .map(|(i, open)| {
                let indent = "    ".repeat(i + 1);
                format!("{}{}{}", indent, return_prefix, open)
            })
            .collect()
    }

    pub fn sync_closure_closes(&self) -> Vec<String> {
        let depth = self.closure_depth();
        (0..depth)
            .rev()
            .map(|i| format!("{}}}", "    ".repeat(i + 1)))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct SwiftParam {
    pub label: Option<String>,
    pub name: String,
    pub swift_type: String,
    pub conversion: SwiftConversion,
}

impl SwiftParam {
    pub fn signature(&self) -> String {
        let inout_prefix = if matches!(self.conversion, SwiftConversion::MutableBuffer { .. }) {
            "inout "
        } else {
            ""
        };
        match &self.label {
            Some(label) if label != &self.name => {
                format!(
                    "{} {}: {}{}",
                    label, self.name, inout_prefix, self.swift_type
                )
            }
            _ => format!("{}: {}{}", self.name, inout_prefix, self.swift_type),
        }
    }

    pub fn ffi_arg(&self) -> String {
        match &self.conversion {
            SwiftConversion::Direct => self.name.clone(),
            SwiftConversion::CStyleEnumRawValue => format!("{}.rawValue", self.name),
            SwiftConversion::ToComposite { c_type, fields } => {
                let field_inits: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}.{}", f.c_name, self.name, f.swift_name))
                    .collect();
                format!("{}({})", c_type, field_inits.join(", "))
            }
            SwiftConversion::ToCompositeBuffer {
                c_type,
                length_unit,
                ..
            } => match length_unit {
                SpanLengthUnit::Elements => {
                    format!("{}Ptr.baseAddress, UInt({}Raw.count)", self.name, self.name)
                }
                SpanLengthUnit::Bytes => {
                    format!(
                        "{}Ptr.baseAddress, UInt({}Raw.count * MemoryLayout<{}>.stride)",
                        self.name, self.name, c_type
                    )
                }
            },
            SwiftConversion::ToString => format!(
                "{}Buf.baseAddress!, UInt({}Buf.count)",
                self.name, self.name
            ),
            SwiftConversion::ToData => {
                format!("{}Ptr.baseAddress, UInt({}Ptr.count)", self.name, self.name)
            }
            SwiftConversion::ToWireBuffer { .. } => {
                format!("{}Buf.baseAddress, UInt({}Buf.count)", self.name, self.name)
            }
            SwiftConversion::PrimitiveBuffer { .. } => {
                format!("{}Ptr.baseAddress, UInt({}Ptr.count)", self.name, self.name)
            }
            SwiftConversion::MutableBuffer { .. } => {
                format!("{}Ptr.baseAddress, UInt({}Ptr.count)", self.name, self.name)
            }
            SwiftConversion::WrapCallback { protocol, nullable } => {
                if *nullable {
                    format!(
                        "{}.map {{ {}Bridge.create($0) }} ?? BoltFFICallbackHandle(handle: 0, vtable: nil)",
                        self.name, protocol
                    )
                } else {
                    format!("{}Bridge.create({})", protocol, self.name)
                }
            }
            SwiftConversion::InlineClosure { closure } => {
                format!("{}, {}", closure.trampoline_var, closure.ptr_var)
            }
            SwiftConversion::PassHandle { nullable, .. } => {
                if *nullable {
                    format!("{}?.handle", self.name)
                } else {
                    format!("{}.handle", self.name)
                }
            }
        }
    }

    pub fn needs_wrapper(&self) -> bool {
        !matches!(
            self.conversion,
            SwiftConversion::Direct
                | SwiftConversion::CStyleEnumRawValue
                | SwiftConversion::ToComposite { .. }
        )
    }

    pub fn wrapper_code(&self) -> Option<String> {
        match &self.conversion {
            SwiftConversion::ToString => Some(format!("var {n} = {n}", n = self.name)),
            SwiftConversion::ToCompositeBuffer { c_type, fields, .. } => {
                let field_inits = fields
                    .iter()
                    .map(|field| format!("{}: item.{}", field.c_name, field.swift_name))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!(
                    "let {name}Raw = {name}.map {{ item in {c_type}({field_inits}) }}",
                    name = self.name
                ))
            }
            SwiftConversion::InlineClosure { closure } => Some(closure.render()),
            SwiftConversion::ToWireBuffer { encode } => {
                let writer_body = emit::emit_writer_write(encode);
                Some(format!(
                    "let {name}Bytes = boltffiEncode {{ writer in {body} }}",
                    name = self.name,
                    body = writer_body
                ))
            }
            _ => None,
        }
    }

    pub fn needs_closure_wrap(&self) -> bool {
        matches!(
            &self.conversion,
            SwiftConversion::ToString
                | SwiftConversion::ToCompositeBuffer { .. }
                | SwiftConversion::ToWireBuffer { .. }
                | SwiftConversion::ToData
                | SwiftConversion::PrimitiveBuffer { .. }
                | SwiftConversion::MutableBuffer { .. }
        )
    }

    pub fn closure_wrap_open(&self) -> Option<String> {
        match &self.conversion {
            SwiftConversion::ToString => Some(format!("{n}.withUTF8 {{ {n}Buf in", n = self.name)),
            SwiftConversion::ToCompositeBuffer { .. } => Some(format!(
                "{}Raw.withUnsafeBufferPointer {{ {}Ptr in",
                self.name, self.name
            )),
            SwiftConversion::ToWireBuffer { .. } => Some(format!(
                "{}Bytes.withUnsafeBufferPointer {{ {}Buf in",
                self.name, self.name
            )),
            SwiftConversion::ToData => Some(format!(
                "{}.withUnsafeBytes {{ {}Ptr in",
                self.name, self.name
            )),
            SwiftConversion::PrimitiveBuffer { .. } => Some(format!(
                "{}.withUnsafeBufferPointer {{ {}Ptr in",
                self.name, self.name
            )),
            SwiftConversion::MutableBuffer { .. } => Some(format!(
                "{}.withUnsafeMutableBufferPointer {{ {}Ptr in",
                self.name, self.name
            )),
            _ => None,
        }
    }

    pub fn closure_wrap_close(&self) -> Option<&'static str> {
        match &self.conversion {
            SwiftConversion::ToString
            | SwiftConversion::ToCompositeBuffer { .. }
            | SwiftConversion::ToWireBuffer { .. }
            | SwiftConversion::ToData => Some("}"),
            SwiftConversion::PrimitiveBuffer { .. } | SwiftConversion::MutableBuffer { .. } => {
                Some("}")
            }
            _ => None,
        }
    }
}

impl SwiftClosureTrampoline {
    pub fn render(&self) -> String {
        let c_params = std::iter::once("UnsafeMutableRawPointer?".to_string())
            .chain(
                self.trampoline_params
                    .iter()
                    .map(|param| param.c_type.clone()),
            )
            .collect::<Vec<_>>()
            .join(", ");

        let binding_names = std::iter::once("ud".to_string())
            .chain(
                self.trampoline_params
                    .iter()
                    .map(|param| param.name.clone()),
            )
            .collect::<Vec<_>>()
            .join(", ");

        let decode_args = self
            .trampoline_params
            .iter()
            .map(|param| param.decode_expr.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let closure_call = if decode_args.is_empty() {
            format!(
                "Unmanaged<{box_class}>.fromOpaque(ud!).takeUnretainedValue().fn_()",
                box_class = self.box_class,
            )
        } else {
            format!(
                "Unmanaged<{box_class}>.fromOpaque(ud!).takeUnretainedValue().fn_({decode_args})",
                box_class = self.box_class,
                decode_args = decode_args,
            )
        };

        format!(
            r#"typealias {type_alias} = {swift_type}
        class {box_class} {{ let fn_: {type_alias}; init(_ fn_: @escaping {type_alias}) {{ self.fn_ = fn_ }} }}
        let {box_var} = {box_class}({param_name})
        let {ptr_var} = Unmanaged.passRetained({box_var}).toOpaque()
        defer {{ Unmanaged<{box_class}>.fromOpaque({ptr_var}).release() }}
        let {trampoline_var}: @convention(c) ({c_params}) -> {c_return_type} = {{ {binding_names} in
            {trampoline_body}
        }}"#,
            type_alias = self.type_alias,
            swift_type = self.swift_type,
            box_class = self.box_class,
            box_var = self.box_var,
            ptr_var = self.ptr_var,
            trampoline_var = self.trampoline_var,
            param_name = self.param_name,
            c_params = c_params,
            c_return_type = self.c_return_type,
            binding_names = binding_names,
            trampoline_body = self.render_body(&closure_call),
        )
    }

    fn render_body(&self, closure_call: &str) -> String {
        match self.value_return_strategy {
            ValueReturnStrategy::Void => closure_call.to_string(),
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue) => {
                format!("return {}", closure_call)
            }
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag) => {
                format!("return {}.rawValue", closure_call)
            }
            ValueReturnStrategy::CompositeValue => {
                let composite_expr = self
                    .returns
                    .composite_pack_expr("result")
                    .expect("composite closure returns should pack to a C struct");
                format!(
                    "let result = {}\n            return {}",
                    closure_call, composite_expr
                )
            }
            ValueReturnStrategy::Buffer(_)
            | ValueReturnStrategy::ObjectHandle
            | ValueReturnStrategy::CallbackHandle => {
                let encoded_expr = self
                    .returns
                    .encoded_result_expr()
                    .expect("encoded closure returns should have encode ops");
                format!(
                    "let result = {closure_call}\n            {encoded_expr}\n            if encoded.count > 0 {{\n                guard let allocated = malloc(encoded.count)?.assumingMemoryBound(to: UInt8.self) else {{\n                    return FfiBuf_u8(ptr: nil, len: 0, cap: 0, align: 1)\n                }}\n                _ = encoded.withUnsafeBytes {{ bytes in\n                    memcpy(allocated, bytes.baseAddress!, bytes.count)\n                }}\n                return FfiBuf_u8(ptr: allocated, len: encoded.count, cap: encoded.count, align: 1)\n            }}\n            return FfiBuf_u8(ptr: nil, len: 0, cap: 0, align: 1)",
                    closure_call = closure_call,
                    encoded_expr = encoded_expr,
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SwiftConversion {
    Direct,
    CStyleEnumRawValue,
    ToString,
    ToData,
    ToWireBuffer {
        encode: WriteSeq,
    },
    ToComposite {
        c_type: String,
        fields: Vec<CompositeFieldMapping>,
    },
    ToCompositeBuffer {
        c_type: String,
        fields: Vec<CompositeFieldMapping>,
        length_unit: SpanLengthUnit,
    },
    PrimitiveBuffer {
        element_type: String,
    },
    MutableBuffer {
        element_type: String,
    },
    WrapCallback {
        protocol: String,
        nullable: bool,
    },
    InlineClosure {
        closure: Box<SwiftClosureTrampoline>,
    },
    PassHandle {
        class_name: String,
        nullable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SwiftClosureTrampoline {
    pub type_alias: String,
    pub swift_type: String,
    pub box_class: String,
    pub box_var: String,
    pub ptr_var: String,
    pub trampoline_var: String,
    pub param_name: String,
    pub trampoline_params: Vec<SwiftClosureTrampolineParam>,
    pub c_return_type: String,
    pub value_return_strategy: ValueReturnStrategy,
    pub returns: SwiftReturn,
}

#[derive(Debug, Clone)]
pub struct SwiftClosureTrampolineParam {
    pub name: String,
    pub c_type: String,
    pub decode_expr: String,
}

#[derive(Debug, Clone)]
pub enum SwiftReturn {
    Void,
    Direct {
        swift_type: String,
    },
    CStyleEnumFromRawValue {
        swift_type: String,
    },
    FromComposite {
        swift_type: String,
        c_type: String,
        fields: Vec<CompositeFieldMapping>,
    },
    FromWireBuffer {
        swift_type: String,
        decode: ReadSeq,
        encode: WriteSeq,
    },
    FromDirectBuffer {
        swift_type: String,
        element_swift_type: String,
        composite_mapping: Option<DirectBufferCompositeMapping>,
        enum_mapping: Option<String>,
    },
    Handle {
        class_name: String,
        nullable: bool,
    },
    Callback {
        protocol_name: String,
        nullable: bool,
    },
    Throws {
        ok: Box<SwiftReturn>,
        err_type: String,
        result_decode: ReadSeq,
        err_decode: ReadSeq,
        err_is_string: bool,
        err_encode: Option<WriteSeq>,
    },
}

impl SwiftReturn {
    fn result_ok_reader_expr(&self, ok: &ReadSeq) -> String {
        match self {
            SwiftReturn::Void => "()".to_string(),
            SwiftReturn::CStyleEnumFromRawValue { swift_type } => {
                format!("{}(rawValue: {})!", swift_type, emit::emit_reader_read(ok))
            }
            _ => emit::emit_reader_read(ok),
        }
    }

    pub fn swift_type(&self) -> Option<String> {
        match self {
            SwiftReturn::Void => None,
            SwiftReturn::Direct { swift_type } => Some(swift_type.clone()),
            SwiftReturn::CStyleEnumFromRawValue { swift_type } => Some(swift_type.clone()),
            SwiftReturn::FromComposite { swift_type, .. } => Some(swift_type.clone()),
            SwiftReturn::FromWireBuffer { swift_type, .. } => Some(swift_type.clone()),
            SwiftReturn::FromDirectBuffer { swift_type, .. } => Some(swift_type.clone()),
            SwiftReturn::Handle {
                class_name,
                nullable,
            } => {
                if *nullable {
                    Some(format!("{}?", class_name))
                } else {
                    Some(class_name.clone())
                }
            }
            SwiftReturn::Callback {
                protocol_name,
                nullable,
            } => {
                if *nullable {
                    Some(format!("(any {})?", protocol_name))
                } else {
                    Some(format!("any {}", protocol_name))
                }
            }
            SwiftReturn::Throws { ok, .. } => ok.swift_type(),
        }
    }

    pub fn is_throws(&self) -> bool {
        matches!(self, SwiftReturn::Throws { .. })
    }

    pub fn is_void(&self) -> bool {
        matches!(self, SwiftReturn::Void)
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(
            self,
            SwiftReturn::FromWireBuffer { .. } | SwiftReturn::Throws { .. }
        )
    }

    pub fn is_direct_buffer(&self) -> bool {
        matches!(self, SwiftReturn::FromDirectBuffer { .. })
    }

    pub fn direct_buffer_element_type(&self) -> Option<&str> {
        match self {
            SwiftReturn::FromDirectBuffer {
                element_swift_type, ..
            } => Some(element_swift_type.as_str()),
            _ => None,
        }
    }

    pub fn direct_buffer_composite_mapping(&self) -> Option<&DirectBufferCompositeMapping> {
        match self {
            SwiftReturn::FromDirectBuffer {
                composite_mapping, ..
            } => composite_mapping.as_ref(),
            _ => None,
        }
    }

    pub fn direct_buffer_enum_mapping(&self) -> Option<&str> {
        match self {
            SwiftReturn::FromDirectBuffer {
                enum_mapping: Some(e),
                ..
            } => Some(e.as_str()),
            _ => None,
        }
    }

    pub fn direct_buffer_is_data(&self) -> bool {
        match self {
            SwiftReturn::FromDirectBuffer {
                swift_type,
                element_swift_type,
                composite_mapping,
                enum_mapping,
            } => {
                swift_type == "Data"
                    && element_swift_type == "UInt8"
                    && composite_mapping.is_none()
                    && enum_mapping.is_none()
            }
            _ => false,
        }
    }

    pub fn is_handle(&self) -> bool {
        matches!(self, SwiftReturn::Handle { .. })
    }

    pub fn is_callback(&self) -> bool {
        matches!(self, SwiftReturn::Callback { .. })
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, SwiftReturn::CStyleEnumFromRawValue { .. })
    }

    pub fn c_style_enum_type(&self) -> Option<&str> {
        match self {
            SwiftReturn::CStyleEnumFromRawValue { swift_type } => Some(swift_type.as_str()),
            _ => None,
        }
    }

    pub fn is_composite(&self) -> bool {
        match self {
            SwiftReturn::FromComposite { .. } => true,
            SwiftReturn::Throws { ok, .. } => ok.is_composite(),
            _ => false,
        }
    }

    pub fn set_composite_swift_type(&mut self, name: String) {
        if let SwiftReturn::FromComposite { swift_type, .. } = self {
            *swift_type = name;
        }
    }

    pub fn composite_convert_expr(&self, raw_var: &str) -> Option<String> {
        match self {
            SwiftReturn::FromComposite {
                swift_type, fields, ..
            } => {
                let field_assignments: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}.{}", f.swift_name, raw_var, f.c_name))
                    .collect();
                Some(format!("{}({})", swift_type, field_assignments.join(", ")))
            }
            _ => None,
        }
    }

    pub fn handle_info(&self) -> Option<(&str, bool)> {
        match self {
            SwiftReturn::Handle {
                class_name,
                nullable,
            } => Some((class_name.as_str(), *nullable)),
            _ => None,
        }
    }

    pub fn callback_info(&self) -> Option<(&str, bool)> {
        match self {
            SwiftReturn::Callback {
                protocol_name,
                nullable,
            } => Some((protocol_name.as_str(), *nullable)),
            _ => None,
        }
    }

    pub fn composite_pack_expr(&self, value_var: &str) -> Option<String> {
        match self {
            SwiftReturn::FromComposite { c_type, fields, .. } => {
                let field_values = fields
                    .iter()
                    .map(|field| format!("{}: {}.{}", field.c_name, value_var, field.swift_name))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!("{}({})", c_type, field_values))
            }
            SwiftReturn::Throws { ok, .. } => ok.composite_pack_expr(value_var),
            _ => None,
        }
    }

    pub fn encoded_result_expr(&self) -> Option<String> {
        match self {
            SwiftReturn::FromWireBuffer { encode, .. } => {
                let rebased_encode =
                    remap_root_in_seq(encode, ValueExpr::Var("result".to_string()));
                let writer_body = emit::emit_writer_write(&rebased_encode);
                Some(format!(
                    "let encoded = ({{ var writer = WireWriter(); {}; return writer.finalize() }})()",
                    writer_body
                ))
            }
            SwiftReturn::Throws { ok, .. } => ok.encoded_result_expr(),
            _ => None,
        }
    }

    pub fn decode_expr(&self) -> Option<String> {
        match self {
            SwiftReturn::FromWireBuffer { decode, .. } => {
                Some(emit::emit_read_value_at(decode, "0"))
            }
            SwiftReturn::Throws {
                ok,
                err_decode,
                err_is_string,
                ..
            } => match ok.as_ref() {
                SwiftReturn::FromWireBuffer { decode, .. } => match decode.ops.first() {
                    Some(ReadOp::Result { ok, .. }) => {
                        Some(emit::emit_result_ok_throw(ok, err_decode, *err_is_string))
                    }
                    _ => ok.decode_expr(),
                },
                _ => ok.decode_expr(),
            },
            _ => None,
        }
    }

    pub fn reader_decode_expr(&self) -> Option<String> {
        match self {
            SwiftReturn::FromWireBuffer { decode, .. } => Some(emit::emit_reader_read(decode)),
            SwiftReturn::Throws {
                ok,
                result_decode,
                err_is_string,
                ..
            } => Self::decode_result_from_seq(ok, result_decode, *err_is_string),
            _ => None,
        }
    }

    fn decode_result_from_seq(
        ok_return: &SwiftReturn,
        decode: &ReadSeq,
        err_is_string: bool,
    ) -> Option<String> {
        let ops = match ok_return {
            SwiftReturn::FromWireBuffer { decode, .. } => decode,
            _ => decode,
        };
        match ops.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => {
                let ok_read = ok_return.result_ok_reader_expr(ok);
                let err_read = emit::emit_reader_read(err);
                let err_body = if err_is_string {
                    format!("FfiError(message: {})", err_read)
                } else {
                    err_read
                };
                Some(format!(
                    "try {{ let tag = reader.readU8(); if tag == 0 {{ return {} }} else {{ throw {} }} }}()",
                    ok_read, err_body
                ))
            }
            _ => None,
        }
    }
}
