use crate::{
    ir::ops::{OffsetExpr, ReadOp, ReadSeq, WriteSeq},
    render::swift::emit,
};

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
    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    pub fn is_wire_encoded(&self) -> bool {
        matches!(self, Self::Encoded { .. })
    }

    pub fn swift_type(&self) -> Option<&str> {
        match self {
            Self::Void => None,
            Self::Direct { swift_type, .. } => Some(swift_type),
            Self::Encoded {
                throws: true, ok_type: Some(ok), ..
            } => Some(ok),
            Self::Encoded { swift_type, .. } => Some(swift_type),
        }
    }

    pub fn future_type(&self) -> &str {
        match self {
            Self::Void => "Void",
            Self::Direct { swift_type, .. } => swift_type,
            Self::Encoded {
                throws: true, ok_type: Some(ok), ..
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
                ..
            } => match decode.ops.first() {
                Some(ReadOp::Result { ok, .. }) => Some(emit::emit_result_ok_throw(ok, err_decode, *err_is_string)),
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
                ..
            } => match decode.ops.first() {
                Some(ReadOp::Result { ok, err, .. }) => {
                    let ok_read = emit::emit_reader_read(ok);
                    let err_read = emit::emit_reader_read(err);
                    let err_body = if *err_is_string { format!("FfiError(message: {})", err_read) } else { err_read };
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
}

#[derive(Debug, Clone)]
pub enum SwiftAsyncConversion {
    None,
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
        self.functions.iter().any(|f| f.mode.is_async()) || self.classes.iter().any(|c| c.methods.iter().any(|m| m.mode.is_async())) || self.has_async_iterators()
    }

    pub fn has_streams(&self) -> bool {
        self.classes.iter().any(|c| !c.streams.is_empty())
    }

    pub fn has_async_iterators(&self) -> bool {
        self.classes.iter().any(|c| !c.async_iterators.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct SwiftRecord {
    pub class_name: String,
    pub fields: Vec<SwiftField>,
    pub is_blittable: bool,
    pub blittable_size: Option<usize>,
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
                let converted_value = conv.encode_wrapper.replace("$0", &format!("self.{}", self.swift_name));
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
    pub is_error: bool,
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
    pub discriminant: i64,
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
            SwiftVariantPayload::Struct(fields) => fields.len() == 1 && fields[0].swift_name.chars().all(|c| c.is_ascii_digit()),
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
            SwiftVariantPayload::Struct(fields) if fields.len() == 1 && fields[0].swift_name.chars().all(|c| c.is_ascii_digit()) => Some(&fields[0]),
            _ => None,
        }
    }

    pub fn tuple_value_decode(&self) -> String {
        self.single_tuple_field().map(|f| f.wire_decode_inline()).unwrap_or_default()
    }

    pub fn tuple_value_size(&self) -> String {
        self.single_tuple_field().map(|f| f.wire_size_expr()).unwrap_or_default()
    }

    pub fn tuple_value_encode(&self) -> String {
        self.single_tuple_field().map(|f| f.wire_encode()).unwrap_or_default()
    }

    pub fn tuple_value_encode_bytes(&self) -> String {
        self.single_tuple_field().map(|f| f.wire_encode_bytes()).unwrap_or_default()
    }

    pub fn tuple_value_reader_decode(&self) -> String {
        self.single_tuple_field().map(|f| f.wire_reader_decode()).unwrap_or_default()
    }

    pub fn tuple_value_writer_encode(&self) -> String {
        self.single_tuple_field().map(|f| f.wire_writer_encode()).unwrap_or_default()
    }

    pub fn all_fields_fixed_size(&self) -> bool {
        self.fields().iter().all(|f| f.has_fixed_size())
    }

    pub fn tuple_value_fixed_size(&self) -> bool {
        self.single_tuple_field().map(|f| f.has_fixed_size()).unwrap_or(true)
    }
}

#[derive(Debug, Clone)]
pub enum SwiftVariantPayload {
    Unit,
    Tuple(Vec<SwiftField>),
    Struct(Vec<SwiftField>),
}

#[derive(Debug, Clone)]
pub struct SwiftAsyncIterator {
    pub name: String,
    pub item_type: String,
    /// Wire decode for `Option<T>` — same reader machinery as async results
    pub item_decode: ReadSeq,
    /// `{class}_{method}(handle) -> IteratorHandle`
    pub entry: String,
    /// `{class}_{method}_next(iter) -> RustFutureHandle`
    pub next: String,
    pub next_poll: String,
    pub next_complete: String,
    pub next_cancel: String,
    pub next_free: String,
    /// `{class}_{method}_free(iter)`
    pub free: String,
}

impl SwiftAsyncIterator {
    /// The Swift reader decode expression for `Option<T>` from the wire buffer.
    pub fn option_reader_decode_expr(&self) -> String {
        use crate::render::swift::emit;
        emit::emit_reader_read(&self.item_decode)
    }
}

#[derive(Debug, Clone)]
pub struct SwiftClass {
    pub name: String,
    pub ffi_free: String,
    pub constructors: Vec<SwiftConstructor>,
    pub methods: Vec<SwiftMethod>,
    pub streams: Vec<SwiftStream>,
    pub async_iterators: Vec<SwiftAsyncIterator>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftStream {
    pub name: String,
    pub mode: SwiftStreamMode,
    pub item_type: String,
    pub item_decode: ReadSeq,
    pub subscribe: String,
    pub poll: String,
    pub pop_batch: String,
    pub wait: String,
    pub unsubscribe: String,
    pub free: String,
    pub free_buf: String,
    pub atomic_cas: String,
}

impl SwiftStream {
    pub fn item_decode_expr(&self) -> String {
        emit::emit_read_value_at(&self.item_decode, "offset")
    }

    pub fn item_reader_decode_expr(&self) -> String {
        emit::emit_reader_read(&self.item_decode)
    }

    pub fn uses_offset(&self) -> bool {
        uses_offset_in_read_seq(&self.item_decode)
    }
}

fn uses_offset_in_read_seq(seq: &ReadSeq) -> bool {
    seq.ops.iter().any(uses_offset_in_read_op)
}

fn uses_offset_in_read_op(op: &ReadOp) -> bool {
    match op {
        ReadOp::Primitive { offset, .. } => offset_uses(offset),
        ReadOp::String { offset } => offset_uses(offset),
        ReadOp::Bytes { offset } => offset_uses(offset),
        ReadOp::Option { tag_offset, some } => offset_uses(tag_offset) || uses_offset_in_read_seq(some),
        ReadOp::Vec { len_offset, element, .. } => offset_uses(len_offset) || uses_offset_in_read_seq(element),
        ReadOp::Record { offset, .. } => offset_uses(offset),
        ReadOp::Enum { offset, .. } => offset_uses(offset),
        ReadOp::Result { tag_offset, ok, err } => offset_uses(tag_offset) || uses_offset_in_read_seq(ok) || uses_offset_in_read_seq(err),
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
    Batch { class_name: String, method_name_pascal: String },
    Callback { class_name: String, method_name_pascal: String },
}

#[derive(Debug, Clone)]
pub enum SwiftConstructor {
    Designated {
        ffi_symbol: String,
        params: Vec<SwiftParam>,
        is_fallible: bool,
        doc: Option<String>,
    },
    Factory {
        name: String,
        ffi_symbol: String,
        is_fallible: bool,
        doc: Option<String>,
    },
    Convenience {
        name: String,
        ffi_symbol: String,
        params: Vec<SwiftParam>,
        is_fallible: bool,
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
            Self::Designated { ffi_symbol, .. } | Self::Factory { ffi_symbol, .. } | Self::Convenience { ffi_symbol, .. } => ffi_symbol,
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
            Self::Designated { is_fallible, .. } | Self::Factory { is_fallible, .. } | Self::Convenience { is_fallible, .. } => *is_fallible,
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
            first.replace_range(in_pos.., " -> OpaquePointer? in");
        }
        wrappers
    }

    pub fn call_expr(&self) -> String {
        ffi_call_expr(self.ffi_symbol(), &[], self.params())
    }

    pub fn doc(&self) -> &Option<String> {
        match self {
            Self::Designated { doc, .. } | Self::Factory { doc, .. } | Self::Convenience { doc, .. } => doc,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwiftMethod {
    pub name: String,
    pub mode: SwiftCallMode,
    pub params: Vec<SwiftParam>,
    pub returns: SwiftReturn,
    pub is_static: bool,
    pub doc: Option<String>,
}

impl SwiftMethod {
    pub fn needs_handle(&self) -> bool {
        !self.is_static
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
        if self.needs_handle() { vec!["handle"] } else { vec![] }
    }

    pub fn start_call_expr(&self) -> String {
        match &self.mode {
            SwiftCallMode::Async { start, .. } => ffi_call_expr(start, &self.prefix_args(), &self.params),
            SwiftCallMode::Sync { .. } => String::new(),
        }
    }

    pub fn sync_call_expr(&self) -> String {
        match &self.mode {
            SwiftCallMode::Sync { symbol } => ffi_call_expr(symbol, &self.prefix_args(), &self.params),
            SwiftCallMode::Async { .. } => String::new(),
        }
    }

    pub fn closure_depth(&self) -> usize {
        self.params.iter().filter(|p| p.needs_closure_wrap()).count()
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
            "_ = "
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
        (0..depth).rev().map(|i| format!("{}}}", "    ".repeat(i + 2))).collect()
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
    pub methods: Vec<SwiftCallbackMethod>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftCallbackMethod {
    pub swift_name: String,
    pub ffi_name: String,
    pub params: Vec<SwiftCallbackParam>,
    pub returns: SwiftReturn,
    pub is_async: bool,
    pub has_out_param: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SwiftCallbackParam {
    pub label: String,
    pub swift_type: String,
    pub call_arg: String,
    pub ffi_args: Vec<String>,
    pub decode_prelude: Option<String>,
}

impl SwiftCallbackMethod {
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
            let writer_body = emit::emit_writer_write(encode);
            let discriminant = if self.throws() { "writer.writeU8(0); " } else { "" };
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
            SwiftReturn::Throws { err_encode: Some(encode), .. } => {
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
    let args = prefix_args.iter().map(|s| s.to_string()).chain(params.iter().map(|p| p.ffi_arg()));
    format!("{}({})", symbol, args.collect::<Vec<_>>().join(", "))
}

pub fn closure_wrappers(params: &[SwiftParam]) -> Vec<String> {
    params.iter().filter_map(|p| p.closure_wrap_open()).collect()
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
        self.params.iter().filter(|p| p.needs_closure_wrap()).count()
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
            "_ = "
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
        (0..depth).rev().map(|i| format!("{}}}", "    ".repeat(i + 1))).collect()
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
                format!("{} {}: {}{}", label, self.name, inout_prefix, self.swift_type)
            }
            _ => format!("{}: {}{}", self.name, inout_prefix, self.swift_type),
        }
    }

    pub fn ffi_arg(&self) -> String {
        match &self.conversion {
            SwiftConversion::Direct => self.name.clone(),
            SwiftConversion::ToString => format!("{}Buf.baseAddress!, UInt({}Buf.count)", self.name, self.name),
            SwiftConversion::ToData => {
                format!("{}Ptr.baseAddress, UInt({}Ptr.count)", self.name, self.name)
            }
            SwiftConversion::ToWireBuffer { .. } => {
                format!("{}Bytes, UInt({}Bytes.count)", self.name, self.name)
            }
            SwiftConversion::PrimitiveBuffer { .. } => {
                format!("{}Ptr.baseAddress, UInt({}Ptr.count)", self.name, self.name)
            }
            SwiftConversion::MutableBuffer { .. } => {
                format!("{}Ptr.baseAddress, UInt({}Ptr.count)", self.name, self.name)
            }
            SwiftConversion::WrapCallback { protocol, nullable } => {
                if *nullable {
                    format!("{}.map {{ {}Bridge.create($0) }} ?? BoltFFICallbackHandle(handle: 0, vtable: nil)", self.name, protocol)
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
        !matches!(self.conversion, SwiftConversion::Direct)
    }

    pub fn wrapper_code(&self) -> Option<String> {
        match &self.conversion {
            SwiftConversion::ToString => Some(format!("var {n} = {n}", n = self.name)),
            SwiftConversion::InlineClosure { closure } => Some(closure.render()),
            SwiftConversion::ToWireBuffer { encode } => {
                let writer_body = emit::emit_writer_write(encode);
                Some(format!("let {name}Bytes = boltffiEncode {{ writer in {body} }}", name = self.name, body = writer_body))
            }
            _ => None,
        }
    }

    pub fn needs_closure_wrap(&self) -> bool {
        matches!(
            &self.conversion,
            SwiftConversion::ToString | SwiftConversion::ToData | SwiftConversion::PrimitiveBuffer { .. } | SwiftConversion::MutableBuffer { .. }
        )
    }

    pub fn closure_wrap_open(&self) -> Option<String> {
        match &self.conversion {
            SwiftConversion::ToString => Some(format!("{n}.withUTF8 {{ {n}Buf in", n = self.name)),
            SwiftConversion::ToData => Some(format!("{}.withUnsafeBytes {{ {}Ptr in", self.name, self.name)),
            SwiftConversion::PrimitiveBuffer { .. } => Some(format!("{}.withUnsafeBufferPointer {{ {}Ptr in", self.name, self.name)),
            SwiftConversion::MutableBuffer { .. } => Some(format!("{}.withUnsafeMutableBufferPointer {{ {}Ptr in", self.name, self.name)),
            _ => None,
        }
    }

    pub fn closure_wrap_close(&self) -> Option<&'static str> {
        match &self.conversion {
            SwiftConversion::ToString | SwiftConversion::ToData => Some("}"),
            SwiftConversion::PrimitiveBuffer { .. } | SwiftConversion::MutableBuffer { .. } => Some("}"),
            _ => None,
        }
    }
}

impl SwiftClosureTrampoline {
    pub fn render(&self) -> String {
        let c_params: String = self.trampoline_params.iter().map(|p| p.c_type.as_str()).collect::<Vec<_>>().join(", ");

        let param_names: String = self.trampoline_params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");

        let decode_args: String = self.trampoline_params.iter().map(|p| p.decode_expr.as_str()).collect::<Vec<_>>().join(", ");

        format!(
            r#"typealias {type_alias} = {swift_type}
        class {box_class} {{ let fn_: {type_alias}; init(_ fn_: @escaping {type_alias}) {{ self.fn_ = fn_ }} }}
        let {box_var} = {box_class}({param_name})
        let {ptr_var} = Unmanaged.passRetained({box_var}).toOpaque()
        defer {{ Unmanaged<{box_class}>.fromOpaque({ptr_var}).release() }}
        let {trampoline_var}: @convention(c) (UnsafeMutableRawPointer?, {c_params}) -> Void = {{ ud, {param_names} in
            Unmanaged<{box_class}>.fromOpaque(ud!).takeUnretainedValue().fn_({decode_args})
        }}"#,
            type_alias = self.type_alias,
            swift_type = self.swift_type,
            box_class = self.box_class,
            box_var = self.box_var,
            ptr_var = self.ptr_var,
            trampoline_var = self.trampoline_var,
            param_name = self.param_name,
            c_params = c_params,
            param_names = param_names,
            decode_args = decode_args,
        )
    }
}

#[derive(Debug, Clone)]
pub enum SwiftConversion {
    Direct,
    ToString,
    ToData,
    ToWireBuffer { encode: WriteSeq },
    PrimitiveBuffer { element_type: String },
    MutableBuffer { element_type: String },
    WrapCallback { protocol: String, nullable: bool },
    InlineClosure { closure: SwiftClosureTrampoline },
    PassHandle { class_name: String, nullable: bool },
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
    FromWireBuffer {
        swift_type: String,
        decode: ReadSeq,
        encode: WriteSeq,
    },
    Handle {
        class_name: String,
        nullable: bool,
    },
    Throws {
        ok: Box<SwiftReturn>,
        err_type: String,
        err_decode: ReadSeq,
        err_is_string: bool,
        err_encode: Option<WriteSeq>,
    },
}

impl SwiftReturn {
    pub fn swift_type(&self) -> Option<String> {
        match self {
            SwiftReturn::Void => None,
            SwiftReturn::Direct { swift_type } => Some(swift_type.clone()),
            SwiftReturn::FromWireBuffer { swift_type, .. } => Some(swift_type.clone()),
            SwiftReturn::Handle { class_name, nullable } => {
                if *nullable {
                    Some(format!("{}?", class_name))
                } else {
                    Some(class_name.clone())
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
        match self {
            SwiftReturn::FromWireBuffer { .. } => true,
            SwiftReturn::Throws { ok, .. } => ok.is_wire_encoded(),
            _ => false,
        }
    }

    pub fn is_handle(&self) -> bool {
        matches!(self, SwiftReturn::Handle { .. })
    }

    pub fn handle_info(&self) -> Option<(&str, bool)> {
        match self {
            SwiftReturn::Handle { class_name, nullable } => Some((class_name.as_str(), *nullable)),
            _ => None,
        }
    }

    pub fn decode_expr(&self) -> Option<String> {
        match self {
            SwiftReturn::FromWireBuffer { decode, .. } => Some(emit::emit_read_value_at(decode, "0")),
            SwiftReturn::Throws {
                ok, err_decode, err_is_string, ..
            } => match ok.as_ref() {
                SwiftReturn::FromWireBuffer { decode, .. } => match decode.ops.first() {
                    Some(ReadOp::Result { ok, .. }) => Some(emit::emit_result_ok_throw(ok, err_decode, *err_is_string)),
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
            SwiftReturn::Throws { ok, err_is_string, .. } => match ok.as_ref() {
                SwiftReturn::FromWireBuffer { decode, .. } => match decode.ops.first() {
                    Some(ReadOp::Result { ok, err, .. }) => {
                        let ok_read = emit::emit_reader_read(ok);
                        let err_read = emit::emit_reader_read(err);
                        let err_body = if *err_is_string { format!("FfiError(message: {})", err_read) } else { err_read };
                        Some(format!(
                            "try {{ let tag = reader.readU8(); if tag == 0 {{ return {} }} else {{ throw {} }} }}()",
                            ok_read, err_body
                        ))
                    }
                    _ => ok.reader_decode_expr(),
                },
                _ => ok.reader_decode_expr(),
            },
            _ => None,
        }
    }
}
