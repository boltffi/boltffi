use crate::ir::{
    contract::PackageInfo,
    definitions::StreamMode,
    ids::{AsyncIteratorId, CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId, StreamId, VariantName},
    ops::{ReadOp, ReadSeq, WriteOp, WriteSeq},
    plan::{AbiType, CallbackStyle, Mutability},
    types::TypeExpr,
};
use boltffi_ffi_rules::naming::{CreateFn, GlobalSymbol, Name, RegisterFn, VtableField, VtableType};

/// The resolved FFI boundary for the whole crate.
///
/// Each function and method is an [`AbiCall`] with a concrete parameter strategy
/// (wire-encoded buffer vs direct primitive), read/write op sequences for its
/// return type, and for async methods, the polling and completion setup. Backends
/// must read this and transform ops into syntax.
#[derive(Debug, Clone)]
pub struct AbiContract {
    pub package: PackageInfo,
    pub calls: Vec<AbiCall>,
    pub callbacks: Vec<AbiCallbackInvocation>,
    pub streams: Vec<AbiStream>,
    pub async_iterators: Vec<AbiAsyncIterator>,
    pub records: Vec<AbiRecord>,
    pub enums: Vec<AbiEnum>,
    pub free_buf: Name<GlobalSymbol>,
    pub atomic_cas: Name<GlobalSymbol>,
}

#[derive(Debug, Clone)]
pub struct AbiRecord {
    pub id: RecordId,
    pub decode_ops: ReadSeq,
    pub encode_ops: WriteSeq,
    pub is_blittable: bool,
    pub size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct AbiEnum {
    pub id: EnumId,
    pub decode_ops: ReadSeq,
    pub encode_ops: WriteSeq,
    pub is_c_style: bool,
    pub variants: Vec<AbiEnumVariant>,
}

#[derive(Debug, Clone)]
pub struct AbiEnumVariant {
    pub name: VariantName,
    pub discriminant: i64,
    pub payload: AbiEnumPayload,
}

#[derive(Debug, Clone)]
pub enum AbiEnumPayload {
    Unit,
    Tuple(Vec<AbiEnumField>),
    Struct(Vec<AbiEnumField>),
}

#[derive(Debug, Clone)]
pub struct AbiEnumField {
    pub name: FieldName,
    pub type_expr: TypeExpr,
    pub decode: ReadSeq,
    pub encode: WriteSeq,
}

#[derive(Debug, Clone)]
pub struct AbiAsyncIterator {
    pub class_id: ClassId,
    pub iterator_id: AsyncIteratorId,
    pub item: StreamItemTransport,
    /// Creates the iterator handle: `{class}_{method}(handle) -> IteratorHandle`
    pub entry: Name<GlobalSymbol>,
    /// Starts one `next()` future: `{class}_{method}_next(iter) -> RustFutureHandle`
    pub next: Name<GlobalSymbol>,
    /// Polls the future
    pub next_poll: Name<GlobalSymbol>,
    /// Completes the future, returns wire-encoded `Option<T>`
    pub next_complete: Name<GlobalSymbol>,
    /// Cancels the future
    pub next_cancel: Name<GlobalSymbol>,
    /// Frees the future handle
    pub next_free: Name<GlobalSymbol>,
    /// Drops the iterator handle
    pub free: Name<GlobalSymbol>,
}

#[derive(Debug, Clone)]
pub enum StreamItemTransport {
    WireEncoded { decode_ops: ReadSeq },
}

#[derive(Debug, Clone)]
pub struct AbiStream {
    pub class_id: ClassId,
    pub stream_id: StreamId,
    pub mode: StreamMode,
    pub item: StreamItemTransport,
    pub subscribe: Name<GlobalSymbol>,
    pub poll: Name<GlobalSymbol>,
    pub pop_batch: Name<GlobalSymbol>,
    pub wait: Name<GlobalSymbol>,
    pub unsubscribe: Name<GlobalSymbol>,
    pub free: Name<GlobalSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallId {
    Function(FunctionId),
    Method { class_id: ClassId, method_id: MethodId },
    Constructor { class_id: ClassId, index: usize },
}

#[derive(Debug, Clone)]
pub struct AbiCall {
    pub id: CallId,
    pub symbol: Name<GlobalSymbol>,
    pub mode: CallMode,
    pub params: Vec<AbiParam>,
    pub output_shape: OutputShape,
    pub error: ErrorTransport,
}

#[derive(Debug, Clone)]
pub enum CallMode {
    Sync,
    Async(Box<AsyncCall>),
}

#[derive(Debug, Clone)]
pub struct AsyncCall {
    pub poll: Name<GlobalSymbol>,
    pub complete: Name<GlobalSymbol>,
    pub cancel: Name<GlobalSymbol>,
    pub free: Name<GlobalSymbol>,
    pub result_shape: OutputShape,
    pub error: ErrorTransport,
}

#[derive(Debug, Clone)]
pub enum ValueShape {
    Scalar(AbiType),
    OptionScalar { abi: AbiType, read: ReadSeq, write: WriteSeq },
    ResultScalar { ok: AbiType, err: AbiType, read: ReadSeq, write: WriteSeq },
    PrimitiveVec { element_abi: AbiType, read: ReadSeq, write: WriteSeq },
    BlittableRecord { id: RecordId, size: u32, read: ReadSeq, write: WriteSeq },
    WireEncoded { read: ReadSeq, write: WriteSeq },
}

#[derive(Debug, Clone)]
pub enum InputShape {
    Value(ValueShape),
    Utf8Slice {
        len_param: ParamName,
    },
    PrimitiveSlice {
        len_param: ParamName,
        mutability: Mutability,
        element_abi: AbiType,
    },
    WirePacket {
        len_param: ParamName,
        value: ValueShape,
    },
    OutputBuffer {
        len_param: ParamName,
        value: ValueShape,
    },
    Handle {
        class_id: ClassId,
        nullable: bool,
    },
    Callback {
        callback_id: CallbackId,
        nullable: bool,
        style: CallbackStyle,
    },
    HiddenSyntheticLen {
        for_param: ParamName,
    },
    HiddenOutLen {
        for_param: ParamName,
    },
    HiddenOutDirect,
    HiddenStatusOut,
}

#[derive(Debug, Clone)]
pub enum OutputShape {
    Unit,
    Value(ValueShape),
    Handle { class_id: ClassId, nullable: bool },
    Callback { callback_id: CallbackId, nullable: bool },
}

#[derive(Debug, Clone)]
pub struct AbiParam {
    pub name: ParamName,
    pub ffi_type: AbiType,
    pub input_shape: InputShape,
}

#[derive(Debug, Clone)]
pub enum ErrorTransport {
    None,
    StatusCode,
    Encoded { decode_ops: ReadSeq, encode_ops: Option<WriteSeq> },
}

#[derive(Debug, Clone, Copy)]
pub enum ParamBinding<'a> {
    Input(InputBinding<'a>),
    Hidden(HiddenInputBinding<'a>),
    UnsupportedValue,
}

#[derive(Debug, Clone, Copy)]
pub enum InputBinding<'a> {
    Scalar,
    Utf8Slice {
        len_param: &'a ParamName,
    },
    PrimitiveSlice {
        len_param: &'a ParamName,
        mutability: Mutability,
        element_abi: AbiType,
    },
    WirePacket {
        len_param: &'a ParamName,
        decode_ops: &'a ReadSeq,
        encode_ops: &'a WriteSeq,
    },
    OutputBuffer {
        len_param: &'a ParamName,
        decode_ops: &'a ReadSeq,
    },
    Handle {
        class_id: &'a ClassId,
        nullable: bool,
    },
    CallbackHandle {
        callback_id: &'a CallbackId,
        nullable: bool,
        style: CallbackStyle,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum HiddenInputBinding<'a> {
    SyntheticLen { for_param: &'a ParamName },
    OutLen { for_param: &'a ParamName },
    OutDirect,
    StatusOut,
}

#[derive(Debug, Clone, Copy)]
pub enum FastOutputBinding<'a> {
    Scalar {
        abi_type: AbiType,
    },
    OptionScalar {
        abi_type: AbiType,
        decode_ops: &'a ReadSeq,
        encode_ops: &'a WriteSeq,
    },
    ResultScalar {
        ok_abi: AbiType,
        err_abi: AbiType,
        decode_ops: &'a ReadSeq,
        encode_ops: &'a WriteSeq,
    },
    PrimitiveVec {
        element_abi: AbiType,
        decode_ops: &'a ReadSeq,
        encode_ops: &'a WriteSeq,
    },
    BlittableRecord {
        record_id: &'a RecordId,
        size: u32,
        decode_ops: &'a ReadSeq,
        encode_ops: &'a WriteSeq,
    },
}

impl FastOutputBinding<'_> {
    pub fn decode_ops(&self) -> Option<&ReadSeq> {
        match self {
            Self::Scalar { .. } => None,
            Self::OptionScalar { decode_ops, .. } | Self::ResultScalar { decode_ops, .. } | Self::PrimitiveVec { decode_ops, .. } | Self::BlittableRecord { decode_ops, .. } => {
                Some(decode_ops)
            }
        }
    }

    pub fn encode_ops(&self) -> Option<&WriteSeq> {
        match self {
            Self::Scalar { .. } => None,
            Self::OptionScalar { encode_ops, .. } | Self::ResultScalar { encode_ops, .. } | Self::PrimitiveVec { encode_ops, .. } | Self::BlittableRecord { encode_ops, .. } => {
                Some(encode_ops)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireOutputKind {
    Utf8String,
    Encoded,
}

#[derive(Debug, Clone, Copy)]
pub struct WireOutputBinding<'a> {
    pub decode_ops: &'a ReadSeq,
    pub encode_ops: &'a WriteSeq,
    pub wire_shape: WireOutputKind,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputBinding<'a> {
    Unit,
    Fast(FastOutputBinding<'a>),
    Wire(WireOutputBinding<'a>),
    Handle { class_id: &'a ClassId, nullable: bool },
    CallbackHandle { callback_id: &'a CallbackId, nullable: bool },
}

impl OutputBinding<'_> {
    pub fn decode_ops(&self) -> Option<&ReadSeq> {
        match self {
            Self::Fast(fast) => fast.decode_ops(),
            Self::Wire(wire) => Some(wire.decode_ops),
            Self::Unit | Self::Handle { .. } | Self::CallbackHandle { .. } => None,
        }
    }

    pub fn encode_ops(&self) -> Option<&WriteSeq> {
        match self {
            Self::Fast(fast) => fast.encode_ops(),
            Self::Wire(wire) => Some(wire.encode_ops),
            Self::Unit | Self::Handle { .. } | Self::CallbackHandle { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AbiCallbackInvocation {
    pub callback_id: CallbackId,
    pub vtable_type: Name<VtableType>,
    pub register_fn: Name<RegisterFn>,
    pub create_fn: Name<CreateFn>,
    pub methods: Vec<AbiCallbackMethod>,
}

#[derive(Debug, Clone)]
pub struct AbiCallbackMethod {
    pub id: MethodId,
    pub vtable_field: Name<VtableField>,
    pub is_async: bool,
    pub params: Vec<AbiParam>,
    pub output_shape: OutputShape,
    pub error: ErrorTransport,
}

impl AbiCall {
    pub fn output_binding(&self) -> OutputBinding<'_> {
        self.output_shape.output_binding()
    }
}

impl AsyncCall {
    pub fn result_binding(&self) -> OutputBinding<'_> {
        self.result_shape.output_binding()
    }
}

impl AbiParam {
    pub fn param_binding(&self) -> ParamBinding<'_> {
        match &self.input_shape {
            InputShape::Value(ValueShape::Scalar(_)) => ParamBinding::Input(InputBinding::Scalar),
            InputShape::Utf8Slice { len_param } => ParamBinding::Input(InputBinding::Utf8Slice { len_param }),
            InputShape::PrimitiveSlice {
                len_param,
                mutability,
                element_abi,
            } => ParamBinding::Input(InputBinding::PrimitiveSlice {
                len_param,
                mutability: *mutability,
                element_abi: *element_abi,
            }),
            InputShape::WirePacket { len_param, value } => ParamBinding::Input(InputBinding::WirePacket {
                len_param,
                decode_ops: value
                    .read_ops()
                    .unwrap_or_else(|| panic!("wire packet input shape missing decode ops for param {}", self.name.as_str())),
                encode_ops: value
                    .write_ops()
                    .unwrap_or_else(|| panic!("wire packet input shape missing encode ops for param {}", self.name.as_str())),
            }),
            InputShape::OutputBuffer { len_param, value } => ParamBinding::Input(InputBinding::OutputBuffer {
                len_param,
                decode_ops: value
                    .read_ops()
                    .unwrap_or_else(|| panic!("output buffer input shape missing decode ops for param {}", self.name.as_str())),
            }),
            InputShape::Handle { class_id, nullable } => ParamBinding::Input(InputBinding::Handle { class_id, nullable: *nullable }),
            InputShape::Callback { callback_id, nullable, style } => ParamBinding::Input(InputBinding::CallbackHandle {
                callback_id,
                nullable: *nullable,
                style: *style,
            }),
            InputShape::HiddenSyntheticLen { for_param } => ParamBinding::Hidden(HiddenInputBinding::SyntheticLen { for_param }),
            InputShape::HiddenOutLen { for_param } => ParamBinding::Hidden(HiddenInputBinding::OutLen { for_param }),
            InputShape::HiddenOutDirect => ParamBinding::Hidden(HiddenInputBinding::OutDirect),
            InputShape::HiddenStatusOut => ParamBinding::Hidden(HiddenInputBinding::StatusOut),
            InputShape::Value(_) => ParamBinding::UnsupportedValue,
        }
    }

    pub fn input_binding(&self) -> Option<InputBinding<'_>> {
        match self.param_binding() {
            ParamBinding::Input(binding) => Some(binding),
            ParamBinding::Hidden(_) | ParamBinding::UnsupportedValue => None,
        }
    }
}

impl OutputShape {
    pub fn output_binding(&self) -> OutputBinding<'_> {
        match self {
            OutputShape::Unit => OutputBinding::Unit,
            OutputShape::Value(ValueShape::Scalar(abi_type)) => OutputBinding::Fast(FastOutputBinding::Scalar { abi_type: *abi_type }),
            OutputShape::Value(ValueShape::OptionScalar { abi, read, write }) => OutputBinding::Fast(FastOutputBinding::OptionScalar {
                abi_type: *abi,
                decode_ops: read,
                encode_ops: write,
            }),
            OutputShape::Value(ValueShape::ResultScalar { ok, err, read, write }) => OutputBinding::Fast(FastOutputBinding::ResultScalar {
                ok_abi: *ok,
                err_abi: *err,
                decode_ops: read,
                encode_ops: write,
            }),
            OutputShape::Value(ValueShape::PrimitiveVec { element_abi, read, write }) => OutputBinding::Fast(FastOutputBinding::PrimitiveVec {
                element_abi: *element_abi,
                decode_ops: read,
                encode_ops: write,
            }),
            OutputShape::Value(ValueShape::BlittableRecord { id, size, read, write }) => OutputBinding::Fast(FastOutputBinding::BlittableRecord {
                record_id: id,
                size: *size,
                decode_ops: read,
                encode_ops: write,
            }),
            OutputShape::Handle { class_id, nullable } => OutputBinding::Handle { class_id, nullable: *nullable },
            OutputShape::Callback { callback_id, nullable } => OutputBinding::CallbackHandle { callback_id, nullable: *nullable },
            OutputShape::Value(ValueShape::WireEncoded { read, write }) => {
                let wire_shape = match (read.ops.first(), write.ops.first()) {
                    (Some(ReadOp::String { .. }), Some(WriteOp::String { .. })) => WireOutputKind::Utf8String,
                    _ => WireOutputKind::Encoded,
                };
                OutputBinding::Wire(WireOutputBinding {
                    decode_ops: read,
                    encode_ops: write,
                    wire_shape,
                })
            }
        }
    }
}

impl ValueShape {
    pub fn read_ops(&self) -> Option<&ReadSeq> {
        match self {
            Self::Scalar(_) => None,
            Self::OptionScalar { read, .. }
            | Self::ResultScalar { read, .. }
            | Self::PrimitiveVec { read, .. }
            | Self::BlittableRecord { read, .. }
            | Self::WireEncoded { read, .. } => Some(read),
        }
    }

    pub fn write_ops(&self) -> Option<&WriteSeq> {
        match self {
            Self::Scalar(_) => None,
            Self::OptionScalar { write, .. }
            | Self::ResultScalar { write, .. }
            | Self::PrimitiveVec { write, .. }
            | Self::BlittableRecord { write, .. }
            | Self::WireEncoded { write, .. } => Some(write),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ops::{SizeExpr, WireShape};
    use boltffi_ffi_rules::naming;

    fn minimal_contract(call: AbiCall) -> AbiContract {
        AbiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            calls: vec![call],
            callbacks: Vec::new(),
            streams: Vec::new(),
            async_iterators: Vec::new(),
            records: Vec::new(),
            enums: Vec::new(),
            free_buf: naming::free_buf_u8(),
            atomic_cas: naming::atomic_u8_cas(),
        }
    }

    fn minimal_call(param: AbiParam, output_shape: OutputShape) -> AbiCall {
        AbiCall {
            id: CallId::Function(FunctionId::new("f")),
            symbol: naming::function_ffi_name("f"),
            mode: CallMode::Sync,
            params: vec![param],
            output_shape,
            error: ErrorTransport::None,
        }
    }

    fn assert_value_shape_consistency(value_shape: &ValueShape) {
        match value_shape {
            ValueShape::Scalar(_) => {}
            ValueShape::OptionScalar { .. }
            | ValueShape::ResultScalar { .. }
            | ValueShape::PrimitiveVec { .. }
            | ValueShape::BlittableRecord { .. }
            | ValueShape::WireEncoded { .. } => {
                assert!(value_shape.read_ops().is_some());
                assert!(value_shape.write_ops().is_some());
            }
        }
    }

    fn assert_param_shape_consistency(param: &AbiParam) {
        if let InputShape::Value(ValueShape::Scalar(abi_type)) = &param.input_shape {
            assert_eq!(*abi_type, param.ffi_type);
        }
        if matches!(
            param.input_shape,
            InputShape::Utf8Slice { .. }
                | InputShape::PrimitiveSlice { .. }
                | InputShape::WirePacket { .. }
                | InputShape::OutputBuffer { .. }
                | InputShape::Handle { .. }
                | InputShape::Callback { .. }
        ) {
            assert_eq!(param.ffi_type, AbiType::Pointer);
        }
        if let InputShape::Value(value_shape) = &param.input_shape {
            assert_value_shape_consistency(value_shape);
        }
    }

    fn assert_contract_shape_consistency(contract: &AbiContract) {
        contract.calls.iter().for_each(|call| {
            call.params.iter().for_each(assert_param_shape_consistency);
            if let OutputShape::Value(value_shape) = &call.output_shape {
                assert_value_shape_consistency(value_shape);
            }
            if let CallMode::Async(async_call) = &call.mode
                && let OutputShape::Value(value_shape) = &async_call.result_shape
            {
                assert_value_shape_consistency(value_shape);
            }
        });
        contract.callbacks.iter().for_each(|callback| {
            callback.methods.iter().for_each(|method| {
                method.params.iter().for_each(assert_param_shape_consistency);
                if let OutputShape::Value(value_shape) = &method.output_shape {
                    assert_value_shape_consistency(value_shape);
                }
            });
        });
    }

    #[test]
    fn shape_consistency_accepts_matching_contract() {
        let param = AbiParam {
            name: ParamName::new("v"),
            ffi_type: AbiType::I32,
            input_shape: InputShape::Value(ValueShape::Scalar(AbiType::I32)),
        };
        let call = minimal_call(param, OutputShape::Value(ValueShape::Scalar(AbiType::I32)));
        let contract = minimal_contract(call);
        assert_contract_shape_consistency(&contract);
    }

    #[test]
    #[should_panic]
    fn shape_consistency_rejects_scalar_input_shape_type_mismatch() {
        let param = AbiParam {
            name: ParamName::new("v"),
            ffi_type: AbiType::I64,
            input_shape: InputShape::Value(ValueShape::Scalar(AbiType::I32)),
        };
        let call = minimal_call(param, OutputShape::Unit);
        assert_contract_shape_consistency(&minimal_contract(call));
    }

    #[test]
    fn encoded_shapes_keep_wire_metadata() {
        let read = ReadSeq {
            size: SizeExpr::Fixed(0),
            ops: Vec::new(),
            shape: WireShape::Value,
        };
        let write = WriteSeq {
            size: SizeExpr::Fixed(0),
            ops: Vec::new(),
            shape: WireShape::Value,
        };
        let encoded = ValueShape::WireEncoded {
            read: read.clone(),
            write: write.clone(),
        };
        assert!(encoded.read_ops().is_some());
        assert!(encoded.write_ops().is_some());
    }
}
