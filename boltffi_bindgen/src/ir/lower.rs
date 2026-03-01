use crate::ir::{
    abi::{
        AbiAsyncIterator, AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField, AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream,
        AsyncCall, CallId, CallMode, ErrorTransport, InputShape, OutputShape, StreamItemTransport, ValueShape,
    },
    codec::{BlittableField, CodecPlan, EncodedField, EnumLayout, RecordLayout, VariantLayout, VariantPayloadLayout, VecLayout},
    contract::FfiContract,
    definitions::{
        AsyncIteratorDef, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, EnumDef, EnumRepr, FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, RecordDef,
        ReturnDef, StreamDef, VariantPayload,
    },
    ids::{AsyncIteratorId, BuiltinId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId},
    ops::{FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq},
    plan::{
        AbiType, AsyncPlan, AsyncResult, CallPlan, CallPlanKind, CallTarget, CallbackStyle, CompletionCallback, DirectPlan, Mutability, ParamPlan, ParamStrategy, ReturnPlan,
        ReturnValuePlan,
    },
    types::{PrimitiveType, TypeExpr},
};
use boltffi_ffi_rules::naming;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

#[derive(Debug, Clone)]
struct AbiCallbackParamPlan {
    name: ParamName,
    strategy: AbiCallbackParamStrategy,
}

#[derive(Debug, Clone)]
enum AbiCallbackParamStrategy {
    Direct(DirectPlan),
    Encoded { codec: CodecPlan },
}

#[derive(Debug, Clone)]
enum ValueShapeClass {
    Scalar(AbiType),
    OptionScalar(AbiType),
    ResultScalar { ok: AbiType, err: AbiType },
    PrimitiveVec(AbiType),
    BlittableRecord { id: RecordId, size: u32 },
    Encoded,
}

/// Walks an [`FfiContract`] and produces an [`AbiContract`].
///
/// Most of the work is codec planning, figuring out which records are blittable,
/// which enums are C-style vs data-carrying, and detecting recursive types.
/// `record_stack` and `enum_stack` track what we are currently lowering so we
/// catch cycles: if lowering `TreeNode` hits `TreeNode` again in its own fields,
/// that is a recursive type, and it gets encoded layout because a fixed size
/// does not exist.
pub struct Lowerer<'c> {
    contract: &'c FfiContract,
    // tracks which records and enums we are currently lowering so we detect cycles.
    // if we hit the same id again mid-walk, the type is recursive and gets
    // encoded layout instead of blittable.
    record_stack: RefCell<HashSet<RecordId>>,
    enum_stack: RefCell<HashSet<EnumId>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn new(contract: &'c FfiContract) -> Self {
        Self {
            contract,
            record_stack: RefCell::new(HashSet::new()),
            enum_stack: RefCell::new(HashSet::new()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ABI Contract Generation
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn to_abi_contract(&self) -> AbiContract {
        let function_calls = self.contract.functions.iter().map(|func| self.abi_call_for_function(func));

        let class_calls = self.contract.catalog.all_classes().flat_map(|class| {
            let ctor_calls = class.constructors.iter().enumerate().map(|(index, ctor)| self.abi_call_for_constructor(class, ctor, index));
            let method_calls = class.methods.iter().map(|method| self.abi_call_for_method(class, method));
            ctor_calls.chain(method_calls)
        });

        let calls = function_calls.chain(class_calls).collect();

        let callbacks = self.contract.catalog.all_callbacks().map(|callback| self.abi_callback_invocation(callback)).collect();

        let records = self.contract.catalog.all_records().map(|record| self.abi_record(record)).collect();

        let enums = self.contract.catalog.all_enums().map(|enumeration| self.abi_enum(enumeration)).collect();

        let streams = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.streams.iter().map(|stream| self.abi_stream(&class.id, stream)))
            .collect();

        let async_iterators = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.async_iterators.iter().map(|iter| self.abi_async_iterator(&class.id, iter)))
            .collect();

        AbiContract {
            package: self.contract.package.clone(),
            calls,
            callbacks,
            streams,
            async_iterators,
            records,
            enums,
            free_buf: naming::free_buf_u8(),
            atomic_cas: naming::atomic_u8_cas(),
        }
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> AbiCall {
        let plan = self.lower_function(func);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (mode, output_shape, error) = self.abi_mode_output_shape_error_for_function(func, &plan.kind);

        AbiCall {
            id: CallId::Function(func.id.clone()),
            symbol,
            mode,
            params,
            output_shape,
            error,
        }
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> AbiCall {
        let plan = self.lower_method(class, method);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (mode, output_shape, error) = self.abi_mode_output_shape_error_for_method(class, method, &plan.kind);

        AbiCall {
            id: CallId::Method {
                class_id: class.id.clone(),
                method_id: method.id.clone(),
            },
            symbol,
            mode,
            params,
            output_shape,
            error,
        }
    }

    fn abi_call_for_constructor(&self, class: &ClassDef, ctor: &ConstructorDef, index: usize) -> AbiCall {
        let plan = self.lower_constructor(class, ctor);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (output_shape, error) = self.sync_output_shape_and_error(match &plan.kind {
            CallPlanKind::Sync { returns } => returns,
            CallPlanKind::Async { .. } => panic!("constructors cannot be async"),
        });

        AbiCall {
            id: CallId::Constructor {
                class_id: class.id.clone(),
                index,
            },
            symbol,
            mode: CallMode::Sync,
            params,
            output_shape,
            error,
        }
    }

    fn abi_callback_invocation(&self, callback: &CallbackTraitDef) -> AbiCallbackInvocation {
        let methods = callback
            .methods
            .iter()
            .map(|method| {
                let params = self.abi_callback_params(callback, method).collect();
                let (output_shape, error) = self.callback_output_shape_and_error(&method.returns);

                AbiCallbackMethod {
                    id: method.id.clone(),
                    vtable_field: naming::vtable_field_name(method.id.as_str()),
                    is_async: method.is_async,
                    params,
                    output_shape,
                    error,
                }
            })
            .collect();

        AbiCallbackInvocation {
            callback_id: callback.id.clone(),
            vtable_type: naming::callback_vtable_name(callback.id.as_str()),
            register_fn: naming::callback_register_fn(callback.id.as_str()),
            create_fn: naming::callback_create_fn(callback.id.as_str()),
            methods,
        }
    }

    fn abi_stream(&self, class_id: &ClassId, stream: &StreamDef) -> AbiStream {
        let class_name = class_id.as_str();
        let stream_name = stream.id.as_str();
        let item_codec = self.build_codec(&stream.item_type);
        let decode_ops = self.expand_decode(&item_codec);

        AbiStream {
            class_id: class_id.clone(),
            stream_id: stream.id.clone(),
            mode: stream.mode,
            item: StreamItemTransport::WireEncoded { decode_ops },
            subscribe: naming::stream_ffi_subscribe(class_name, stream_name),
            poll: naming::stream_ffi_poll(class_name, stream_name),
            pop_batch: naming::stream_ffi_pop_batch(class_name, stream_name),
            wait: naming::stream_ffi_wait(class_name, stream_name),
            unsubscribe: naming::stream_ffi_unsubscribe(class_name, stream_name),
            free: naming::stream_ffi_free(class_name, stream_name),
        }
    }

    fn abi_async_iterator(&self, class_id: &ClassId, iter: &AsyncIteratorDef) -> AbiAsyncIterator {
        let class_name = class_id.as_str();
        let method_name = iter.id.as_str();

        // The `_next_complete` returns wire-encoded `Option<T>`.
        let option_codec = self.build_codec(&crate::ir::types::TypeExpr::Option(Box::new(iter.item_type.clone())));
        let option_decode_ops = self.expand_decode(&option_codec);

        AbiAsyncIterator {
            class_id: class_id.clone(),
            iterator_id: AsyncIteratorId::new(method_name),
            item: StreamItemTransport::WireEncoded { decode_ops: option_decode_ops },
            entry: naming::iterator_ffi_new(class_name, method_name),
            next: naming::iterator_ffi_next(class_name, method_name),
            next_poll: {
                use boltffi_ffi_rules::naming::Name;
                Name::new(format!("{}_poll", naming::iterator_ffi_next(class_name, method_name)))
            },
            next_complete: {
                use boltffi_ffi_rules::naming::Name;
                Name::new(format!("{}_complete", naming::iterator_ffi_next(class_name, method_name)))
            },
            next_cancel: {
                use boltffi_ffi_rules::naming::Name;
                Name::new(format!("{}_cancel", naming::iterator_ffi_next(class_name, method_name)))
            },
            next_free: {
                use boltffi_ffi_rules::naming::Name;
                Name::new(format!("{}_free", naming::iterator_ffi_next(class_name, method_name)))
            },
            free: naming::iterator_ffi_free(class_name, method_name),
        }
    }

    fn abi_record(&self, record: &RecordDef) -> AbiRecord {
        let codec = self.build_codec(&TypeExpr::Record(record.id.clone()));
        let decode_ops = self.expand_decode(&codec);
        let encode_ops = self.expand_encode(&codec, ValueExpr::Instance);
        let (is_blittable, size) = match codec {
            CodecPlan::Record {
                layout: RecordLayout::Blittable { size, .. },
                ..
            } => (true, Some(size)),
            _ => (false, None),
        };

        AbiRecord {
            id: record.id.clone(),
            decode_ops,
            encode_ops,
            is_blittable,
            size,
        }
    }

    fn abi_enum(&self, enumeration: &EnumDef) -> AbiEnum {
        let codec = self.build_codec(&TypeExpr::Enum(enumeration.id.clone()));
        let decode_ops = self.expand_decode(&codec);
        let encode_ops = self.expand_encode(&codec, ValueExpr::Instance);
        let (is_c_style, variants) = match codec {
            CodecPlan::Enum {
                layout: EnumLayout::CStyle { .. },
                ..
            } => (
                true,
                match &enumeration.repr {
                    EnumRepr::CStyle { variants, .. } => variants
                        .iter()
                        .map(|variant| AbiEnumVariant {
                            name: variant.name.clone(),
                            discriminant: variant.discriminant,
                            payload: AbiEnumPayload::Unit,
                        })
                        .collect(),
                    _ => vec![],
                },
            ),
            CodecPlan::Enum {
                layout: EnumLayout::Data { variants, .. },
                ..
            } => (
                false,
                match &enumeration.repr {
                    EnumRepr::Data { variants: data_variants, .. } => {
                        let layout_fields = variants
                            .iter()
                            .map(|variant| {
                                let fields = match &variant.payload {
                                    VariantPayloadLayout::Unit => Vec::new(),
                                    VariantPayloadLayout::Fields(fields) => fields.iter().map(|field| self.abi_enum_field(field)).collect(),
                                };
                                (variant.name.clone(), fields)
                            })
                            .collect::<HashMap<_, _>>();

                        data_variants
                            .iter()
                            .map(|variant| {
                                let fields = layout_fields.get(&variant.name).cloned().unwrap_or_default();
                                let payload = match &variant.payload {
                                    VariantPayload::Unit => AbiEnumPayload::Unit,
                                    VariantPayload::Tuple(_) => AbiEnumPayload::Tuple(fields),
                                    VariantPayload::Struct(_) => AbiEnumPayload::Struct(fields),
                                };
                                AbiEnumVariant {
                                    name: variant.name.clone(),
                                    discriminant: variant.discriminant,
                                    payload,
                                }
                            })
                            .collect()
                    }
                    _ => Vec::new(),
                },
            ),
            _ => (false, vec![]),
        };

        AbiEnum {
            id: enumeration.id.clone(),
            decode_ops,
            encode_ops,
            is_c_style,
            variants,
        }
    }

    fn abi_enum_field(&self, field: &EncodedField) -> AbiEnumField {
        let decode = self.expand_decode(&field.codec);
        let encode = self.expand_encode(&field.codec, ValueExpr::Named(field.name.as_str().to_string()));
        AbiEnumField {
            name: field.name.clone(),
            type_expr: TypeExpr::from(&field.codec),
            decode,
            encode,
        }
    }

    fn abi_mode_output_shape_error_for_function(&self, func: &FunctionDef, kind: &CallPlanKind) -> (CallMode, OutputShape, ErrorTransport) {
        match kind {
            CallPlanKind::Sync { returns } => {
                let (output_shape, error) = self.sync_output_shape_and_error(returns);
                (CallMode::Sync, output_shape, error)
            }
            CallPlanKind::Async { async_plan } => {
                let mode = CallMode::Async(Box::new(self.async_call_for_function(func, async_plan)));
                (mode, OutputShape::Value(ValueShape::Scalar(AbiType::Pointer)), ErrorTransport::None)
            }
        }
    }

    fn abi_mode_output_shape_error_for_method(&self, class: &ClassDef, method: &MethodDef, kind: &CallPlanKind) -> (CallMode, OutputShape, ErrorTransport) {
        match kind {
            CallPlanKind::Sync { returns } => {
                let (output_shape, error) = self.sync_output_shape_and_error(returns);
                (CallMode::Sync, output_shape, error)
            }
            CallPlanKind::Async { async_plan } => {
                let mode = CallMode::Async(Box::new(self.async_call_for_method(class, method, async_plan)));
                (mode, OutputShape::Value(ValueShape::Scalar(AbiType::Pointer)), ErrorTransport::None)
            }
        }
    }

    fn async_call_for_function(&self, func: &FunctionDef, plan: &AsyncPlan) -> AsyncCall {
        AsyncCall {
            poll: naming::function_ffi_poll(func.id.as_str()),
            complete: naming::function_ffi_complete(func.id.as_str()),
            cancel: naming::function_ffi_cancel(func.id.as_str()),
            free: naming::function_ffi_free(func.id.as_str()),
            result_shape: self.output_shape_from_async_result(&plan.result),
            error: ErrorTransport::StatusCode,
        }
    }

    fn async_call_for_method(&self, class: &ClassDef, method: &MethodDef, plan: &AsyncPlan) -> AsyncCall {
        AsyncCall {
            poll: naming::method_ffi_poll(class.id.as_str(), method.id.as_str()),
            complete: naming::method_ffi_complete(class.id.as_str(), method.id.as_str()),
            cancel: naming::method_ffi_cancel(class.id.as_str(), method.id.as_str()),
            free: naming::method_ffi_free(class.id.as_str(), method.id.as_str()),
            result_shape: self.output_shape_from_async_result(&plan.result),
            error: ErrorTransport::StatusCode,
        }
    }

    fn output_shape_from_async_result(&self, result: &AsyncResult) -> OutputShape {
        match result {
            AsyncResult::Void => OutputShape::Unit,
            AsyncResult::Value(value) => self.output_shape_from_return_value(value),
            AsyncResult::Fallible { ok, err_codec } => {
                let ok_codec = self.codec_from_return_value(ok);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("value".into()));
                OutputShape::Value(self.value_shape_from_read_write(&decode_ops, &encode_ops))
            }
        }
    }

    fn sync_output_shape_and_error(&self, returns: &ReturnPlan) -> (OutputShape, ErrorTransport) {
        match returns {
            ReturnPlan::Value(v) => (self.output_shape_from_return_value(v), ErrorTransport::None),
            // fallible constructors cant be wire-encoded becuase the ok side
            // is a handle (opaque pointer) not a value. so we just return a
            // nullable handle here and let null signal the error case
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Handle { class_id, .. },
                err_codec,
            } => (
                OutputShape::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                ErrorTransport::Encoded {
                    decode_ops: self.expand_decode(err_codec),
                    encode_ops: None,
                },
            ),
            ReturnPlan::Fallible { ok, err_codec } => {
                let ok_codec = self.codec_from_return_value(ok);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("value".into()));
                (
                    OutputShape::Value(self.value_shape_from_read_write(&decode_ops, &encode_ops)),
                    ErrorTransport::Encoded {
                        decode_ops: self.expand_decode(err_codec),
                        encode_ops: None,
                    },
                )
            }
        }
    }

    fn output_shape_from_return_value(&self, value: &ReturnValuePlan) -> OutputShape {
        match value {
            ReturnValuePlan::Void => OutputShape::Unit,
            ReturnValuePlan::Direct(d) => OutputShape::Value(ValueShape::Scalar(d.abi_type)),
            ReturnValuePlan::Encoded { codec } => {
                let decode_ops = self.expand_decode(codec);
                let encode_ops = self.expand_encode(codec, ValueExpr::Var("value".into()));
                OutputShape::Value(self.value_shape_from_read_write(&decode_ops, &encode_ops))
            }
            ReturnValuePlan::Handle { class_id, nullable } => OutputShape::Handle {
                class_id: class_id.clone(),
                nullable: *nullable,
            },
            ReturnValuePlan::Callback { callback_id, nullable } => OutputShape::Callback {
                callback_id: callback_id.clone(),
                nullable: *nullable,
            },
        }
    }

    fn value_shape_from_read_write(&self, read: &ReadSeq, write: &WriteSeq) -> ValueShape {
        let read_class = self.classify_value_shape_from_read_seq(read);
        let write_class = self.classify_value_shape_from_write_seq(write);
        let shape_class = self.merge_value_shape_classes(read_class, write_class);
        self.materialize_value_shape(shape_class, read, write)
    }

    fn classify_value_shape_from_read_seq(&self, read: &ReadSeq) -> ValueShapeClass {
        match read.ops.first() {
            Some(op) => self.classify_value_shape_from_read_op(op),
            None => ValueShapeClass::Encoded,
        }
    }

    fn classify_value_shape_from_write_seq(&self, write: &WriteSeq) -> ValueShapeClass {
        match write.ops.first() {
            Some(op) => self.classify_value_shape_from_write_op(op),
            None => ValueShapeClass::Encoded,
        }
    }

    fn merge_value_shape_classes(&self, read_class: ValueShapeClass, write_class: ValueShapeClass) -> ValueShapeClass {
        match (read_class, write_class) {
            (ValueShapeClass::Scalar(read), ValueShapeClass::Scalar(write)) if read == write => ValueShapeClass::Scalar(read),
            (ValueShapeClass::OptionScalar(read), ValueShapeClass::OptionScalar(write)) if read == write => ValueShapeClass::OptionScalar(read),
            (ValueShapeClass::ResultScalar { ok: read_ok, err: read_err }, ValueShapeClass::ResultScalar { ok: write_ok, err: write_err })
                if read_ok == write_ok && read_err == write_err =>
            {
                ValueShapeClass::ResultScalar { ok: read_ok, err: read_err }
            }
            (ValueShapeClass::PrimitiveVec(read), ValueShapeClass::PrimitiveVec(write)) if read == write => ValueShapeClass::PrimitiveVec(read),
            (ValueShapeClass::BlittableRecord { id: read_id, size: read_size }, ValueShapeClass::BlittableRecord { id: write_id, size: write_size })
                if read_id == write_id && read_size == write_size =>
            {
                ValueShapeClass::BlittableRecord { id: read_id, size: read_size }
            }
            (ValueShapeClass::Encoded, ValueShapeClass::Encoded) => ValueShapeClass::Encoded,
            (read, write) => panic!("read/write value shape mismatch: read={:?}, write={:?}", read, write),
        }
    }

    fn classify_value_shape_from_read_op(&self, op: &ReadOp) -> ValueShapeClass {
        match op {
            ReadOp::Primitive { primitive, .. } => ValueShapeClass::Scalar(primitive_to_abi(*primitive)),
            ReadOp::Option { some, .. } => some
                .ops
                .first()
                .and_then(|inner| self.primitive_abi_from_read_op(inner))
                .map(ValueShapeClass::OptionScalar)
                .unwrap_or(ValueShapeClass::Encoded),
            ReadOp::Result { ok, err, .. } => ok
                .ops
                .first()
                .and_then(|ok_op| self.primitive_abi_from_read_op(ok_op))
                .zip(err.ops.first().and_then(|err_op| self.primitive_abi_from_read_op(err_op)))
                .map(|(ok, err)| ValueShapeClass::ResultScalar { ok, err })
                .unwrap_or(ValueShapeClass::Encoded),
            ReadOp::Vec {
                element_type: TypeExpr::Primitive(primitive),
                ..
            } => ValueShapeClass::PrimitiveVec(primitive_to_abi(*primitive)),
            ReadOp::Vec { .. } => ValueShapeClass::Encoded,
            ReadOp::Record { id, .. } => self
                .blittable_record_size_by_id(id)
                .map(|size| ValueShapeClass::BlittableRecord { id: id.clone(), size })
                .unwrap_or(ValueShapeClass::Encoded),
            _ => ValueShapeClass::Encoded,
        }
    }

    fn classify_value_shape_from_write_op(&self, op: &WriteOp) -> ValueShapeClass {
        match op {
            WriteOp::Primitive { primitive, .. } => ValueShapeClass::Scalar(primitive_to_abi(*primitive)),
            WriteOp::Option { some, .. } => some
                .ops
                .first()
                .and_then(|inner| self.primitive_abi_from_write_op(inner))
                .map(ValueShapeClass::OptionScalar)
                .unwrap_or(ValueShapeClass::Encoded),
            WriteOp::Result { ok, err, .. } => ok
                .ops
                .first()
                .and_then(|ok_op| self.primitive_abi_from_write_op(ok_op))
                .zip(err.ops.first().and_then(|err_op| self.primitive_abi_from_write_op(err_op)))
                .map(|(ok, err)| ValueShapeClass::ResultScalar { ok, err })
                .unwrap_or(ValueShapeClass::Encoded),
            WriteOp::Vec {
                element_type: TypeExpr::Primitive(primitive),
                ..
            } => ValueShapeClass::PrimitiveVec(primitive_to_abi(*primitive)),
            WriteOp::Vec { .. } => ValueShapeClass::Encoded,
            WriteOp::Record { id, .. } => self
                .blittable_record_size_by_id(id)
                .map(|size| ValueShapeClass::BlittableRecord { id: id.clone(), size })
                .unwrap_or(ValueShapeClass::Encoded),
            _ => ValueShapeClass::Encoded,
        }
    }

    fn materialize_value_shape(&self, shape_class: ValueShapeClass, read: &ReadSeq, write: &WriteSeq) -> ValueShape {
        match shape_class {
            ValueShapeClass::Scalar(abi) => ValueShape::Scalar(abi),
            ValueShapeClass::OptionScalar(abi) => ValueShape::OptionScalar {
                abi,
                read: read.clone(),
                write: write.clone(),
            },
            ValueShapeClass::ResultScalar { ok, err } => ValueShape::ResultScalar {
                ok,
                err,
                read: read.clone(),
                write: write.clone(),
            },
            ValueShapeClass::PrimitiveVec(element_abi) => ValueShape::PrimitiveVec {
                element_abi,
                read: read.clone(),
                write: write.clone(),
            },
            ValueShapeClass::BlittableRecord { id, size } => ValueShape::BlittableRecord {
                id,
                size,
                read: read.clone(),
                write: write.clone(),
            },
            ValueShapeClass::Encoded => ValueShape::WireEncoded {
                read: read.clone(),
                write: write.clone(),
            },
        }
    }

    fn primitive_abi_from_read_op(&self, op: &ReadOp) -> Option<AbiType> {
        match op {
            ReadOp::Primitive { primitive, .. } => Some(primitive_to_abi(*primitive)),
            _ => None,
        }
    }

    fn primitive_abi_from_write_op(&self, op: &WriteOp) -> Option<AbiType> {
        match op {
            WriteOp::Primitive { primitive, .. } => Some(primitive_to_abi(*primitive)),
            _ => None,
        }
    }

    fn blittable_record_size_by_id(&self, record_id: &RecordId) -> Option<u32> {
        match self.build_codec(&TypeExpr::Record(record_id.clone())) {
            CodecPlan::Record {
                layout: RecordLayout::Blittable { size, .. },
                ..
            } => u32::try_from(size).ok(),
            _ => None,
        }
    }

    fn codec_from_return_value(&self, value: &ReturnValuePlan) -> CodecPlan {
        match value {
            ReturnValuePlan::Void => CodecPlan::Void,
            ReturnValuePlan::Direct(d) => CodecPlan::Primitive(self.primitive_from_abi(d.abi_type)),
            ReturnValuePlan::Encoded { codec } => codec.clone(),
            ReturnValuePlan::Handle { .. } | ReturnValuePlan::Callback { .. } => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    fn primitive_from_abi(&self, abi: AbiType) -> PrimitiveType {
        match abi {
            AbiType::Bool => PrimitiveType::Bool,
            AbiType::I8 => PrimitiveType::I8,
            AbiType::U8 => PrimitiveType::U8,
            AbiType::I16 => PrimitiveType::I16,
            AbiType::U16 => PrimitiveType::U16,
            AbiType::I32 => PrimitiveType::I32,
            AbiType::U32 => PrimitiveType::U32,
            AbiType::I64 => PrimitiveType::I64,
            AbiType::U64 => PrimitiveType::U64,
            AbiType::ISize => PrimitiveType::ISize,
            AbiType::USize => PrimitiveType::USize,
            AbiType::F32 => PrimitiveType::F32,
            AbiType::F64 => PrimitiveType::F64,
            AbiType::Void | AbiType::Pointer => {
                panic!("unsupported ABI primitive for wire encoding")
            }
        }
    }

    fn expand_decode(&self, codec: &CodecPlan) -> ReadSeq {
        self.expand_decode_with_offset(codec, "pos")
    }

    // self is only used in recursive calls, records, enums, vecs all recurse back here,
    // but clippy does not see through the recursion and thinks it is unused.
    #[allow(clippy::only_used_in_recursion)]
    fn expand_decode_with_offset(&self, codec: &CodecPlan, base: &str) -> ReadSeq {
        let offset = OffsetExpr::Base;
        match codec {
            CodecPlan::Void => ReadSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            },
            CodecPlan::Primitive(primitive) => ReadSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![ReadOp::Primitive { primitive: *primitive, offset }],
                shape: WireShape::Value,
            },
            CodecPlan::String => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::String { offset }],
                shape: WireShape::Value,
            },
            CodecPlan::Bytes => ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Bytes { offset }],
                shape: WireShape::Value,
            },
            CodecPlan::Builtin(id) => {
                let size = self.builtin_fixed_size(id).map(SizeExpr::Fixed).unwrap_or(SizeExpr::Runtime);
                ReadSeq {
                    size,
                    ops: vec![ReadOp::Builtin { id: id.clone(), offset }],
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
                                let offset_expr = if field.offset == 0 { OffsetExpr::Base } else { OffsetExpr::BasePlus(field.offset) };
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
                    ops: vec![ReadOp::Record { id: id.clone(), offset, fields }],
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

    fn expand_encode(&self, codec: &CodecPlan, value: ValueExpr) -> WriteSeq {
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
                ops: vec![WriteOp::String { value: value.clone() }],
                shape: WireShape::Value,
            },
            CodecPlan::Bytes => WriteSeq {
                size: SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::BytesLen(value.clone())]),
                ops: vec![WriteOp::Bytes { value: value.clone() }],
                shape: WireShape::Value,
            },
            CodecPlan::Builtin(id) => WriteSeq {
                size: self.builtin_fixed_size(id).map(SizeExpr::Fixed).unwrap_or_else(|| {
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
                            record_id: None,
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
                let size_expr = if matches!(element.as_ref(), CodecPlan::Primitive(PrimitiveType::U8)) {
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
                                seq: self.expand_encode(&CodecPlan::Primitive(field.primitive), field_value),
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
                        record_id: Some(id.clone()),
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
                        record_id: None,
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

    fn builtin_fixed_size(&self, id: &BuiltinId) -> Option<usize> {
        match id.as_str() {
            "Duration" | "SystemTime" => Some(12),
            "Uuid" => Some(16),
            _ => None,
        }
    }

    fn abi_params_from_plan(&self, params: &[ParamPlan]) -> Vec<AbiParam> {
        params.iter().flat_map(|param| self.abi_param_from_plan(param)).collect()
    }

    fn abi_param_from_plan(&self, param: &ParamPlan) -> Vec<AbiParam> {
        let base_name = param.name.as_str();
        let len_name = ParamName::new(format!("{}_len", base_name));

        match &param.strategy {
            ParamStrategy::Direct(d) => vec![AbiParam {
                name: param.name.clone(),
                ffi_type: d.abi_type,
                input_shape: InputShape::Value(ValueShape::Scalar(d.abi_type)),
            }],
            ParamStrategy::Buffer { mutability, element_abi } => vec![
                AbiParam {
                    name: param.name.clone(),
                    ffi_type: AbiType::Pointer,
                    input_shape: InputShape::PrimitiveSlice {
                        len_param: len_name.clone(),
                        mutability: *mutability,
                        element_abi: *element_abi,
                    },
                },
                AbiParam {
                    name: len_name,
                    ffi_type: AbiType::U64,
                    input_shape: InputShape::HiddenSyntheticLen { for_param: param.name.clone() },
                },
            ],
            ParamStrategy::String { .. } => vec![
                AbiParam {
                    name: param.name.clone(),
                    ffi_type: AbiType::Pointer,
                    input_shape: InputShape::Utf8Slice { len_param: len_name.clone() },
                },
                AbiParam {
                    name: len_name,
                    ffi_type: AbiType::U64,
                    input_shape: InputShape::HiddenSyntheticLen { for_param: param.name.clone() },
                },
            ],
            ParamStrategy::Encoded { codec, mutability } => {
                let decode_ops = self.expand_decode(codec);
                let encode_ops = self.expand_encode(codec, ValueExpr::Named(param.name.as_str().to_string()));
                let value_shape = self.value_shape_from_read_write(&decode_ops, &encode_ops);
                let input_shape = match mutability {
                    Mutability::Mutable => InputShape::OutputBuffer {
                        len_param: len_name.clone(),
                        value: value_shape.clone(),
                    },
                    Mutability::Shared => InputShape::WirePacket {
                        len_param: len_name.clone(),
                        value: value_shape,
                    },
                };
                vec![
                    AbiParam {
                        name: param.name.clone(),
                        ffi_type: AbiType::Pointer,
                        input_shape,
                    },
                    AbiParam {
                        name: len_name,
                        ffi_type: AbiType::U64,
                        input_shape: InputShape::HiddenSyntheticLen { for_param: param.name.clone() },
                    },
                ]
            }
            ParamStrategy::Handle { class_id, nullable } => vec![AbiParam {
                name: param.name.clone(),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::Handle {
                    class_id: class_id.clone(),
                    nullable: *nullable,
                },
            }],
            ParamStrategy::Callback { callback_id, nullable, style } => vec![AbiParam {
                name: param.name.clone(),
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::Callback {
                    callback_id: callback_id.clone(),
                    nullable: *nullable,
                    style: *style,
                },
            }],
        }
    }

    fn callback_output_shape_and_error(&self, returns: &ReturnDef) -> (OutputShape, ErrorTransport) {
        match returns {
            ReturnDef::Void => (OutputShape::Unit, ErrorTransport::None),
            ReturnDef::Value(ty) => {
                let plan = self.lower_value_type(ty);
                (self.output_shape_from_return_value(&plan), ErrorTransport::None)
            }
            ReturnDef::Result { ok, err } => {
                let ok_codec = self.build_codec(ok);
                let err_codec = self.build_codec(err);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec.clone()),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("result".into()));
                (
                    OutputShape::Value(self.value_shape_from_read_write(&decode_ops, &encode_ops)),
                    ErrorTransport::Encoded {
                        decode_ops: self.expand_decode(&err_codec),
                        encode_ops: Some(self.expand_encode(&err_codec, ValueExpr::Var("error".into()))),
                    },
                )
            }
        }
    }

    fn abi_callback_params<'a>(&'a self, callback: &'a CallbackTraitDef, method: &'a CallbackMethodDef) -> impl Iterator<Item = AbiParam> + 'a {
        let handle_param = AbiParam {
            name: ParamName::new("handle"),
            ffi_type: AbiType::Pointer,
            input_shape: InputShape::Callback {
                callback_id: callback.id.clone(),
                nullable: false,
                style: CallbackStyle::BoxedDyn,
            },
        };

        let method_params = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(param))
            .flat_map(|param| self.abi_callback_param_from_plan(param));

        let out_params = self.abi_callback_out_params(&method.returns, method.is_async);

        std::iter::once(handle_param).chain(method_params).chain(out_params)
    }

    fn abi_callback_param_from_plan(&self, param: AbiCallbackParamPlan) -> Vec<AbiParam> {
        let base_name = param.name.as_str();
        let len_name = ParamName::new(format!("{}_len", base_name));

        match param.strategy {
            AbiCallbackParamStrategy::Direct(d) => vec![AbiParam {
                name: param.name,
                ffi_type: d.abi_type,
                input_shape: InputShape::Value(ValueShape::Scalar(d.abi_type)),
            }],
            AbiCallbackParamStrategy::Encoded { codec } => {
                let decode_ops = self.expand_decode(&codec);
                let encode_ops = self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                let value_shape = self.value_shape_from_read_write(&decode_ops, &encode_ops);
                vec![
                    AbiParam {
                        name: param.name.clone(),
                        ffi_type: AbiType::Pointer,
                        input_shape: InputShape::WirePacket {
                            len_param: len_name.clone(),
                            value: value_shape,
                        },
                    },
                    AbiParam {
                        name: len_name,
                        ffi_type: AbiType::U64,
                        input_shape: InputShape::HiddenSyntheticLen { for_param: param.name.clone() },
                    },
                ]
            }
        }
    }

    fn abi_callback_out_params(&self, returns: &ReturnDef, is_async: bool) -> Vec<AbiParam> {
        let has_return = !matches!(returns, ReturnDef::Void) && !is_async;
        let out_ptr_name = ParamName::new("out_ptr");
        let out_len_name = ParamName::new("out_len");

        if !has_return {
            return Vec::new();
        }

        let (output_shape, _) = self.callback_output_shape_and_error(returns);

        match output_shape {
            OutputShape::Value(ValueShape::Scalar(abi)) => std::iter::once(AbiParam {
                name: out_ptr_name,
                ffi_type: abi,
                input_shape: InputShape::HiddenOutDirect,
            })
            .collect(),
            OutputShape::Value(value_shape) => {
                vec![
                    AbiParam {
                        name: out_ptr_name.clone(),
                        ffi_type: AbiType::Pointer,
                        input_shape: InputShape::OutputBuffer {
                            len_param: out_len_name.clone(),
                            value: value_shape,
                        },
                    },
                    AbiParam {
                        name: out_len_name,
                        ffi_type: AbiType::U64,
                        input_shape: InputShape::HiddenOutLen { for_param: out_ptr_name },
                    },
                ]
            }
            OutputShape::Handle { .. } | OutputShape::Callback { .. } | OutputShape::Unit => std::iter::once(AbiParam {
                name: out_ptr_name,
                ffi_type: AbiType::Pointer,
                input_shape: InputShape::HiddenOutDirect,
            })
            .collect(),
        }
    }
}

impl<'c> Lowerer<'c> {
    fn lower_function(&self, func: &FunctionDef) -> CallPlan {
        let params = func.params.iter().map(|p| self.lower_param(p)).collect();

        let kind = if func.is_async {
            CallPlanKind::Async {
                async_plan: self.build_async_plan(&func.returns),
            }
        } else {
            CallPlanKind::Sync {
                returns: self.lower_return(&func.returns),
            }
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.function_symbol(&func.id)),
            params,
            kind,
        }
    }

    fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> CallPlan {
        let mut params: Vec<ParamPlan> = method.params.iter().map(|p| self.lower_param(p)).collect();

        if method.receiver != Receiver::Static {
            params.insert(
                0,
                ParamPlan {
                    name: ParamName::new("self"),
                    strategy: ParamStrategy::Handle {
                        class_id: class.id.clone(),
                        nullable: false,
                    },
                },
            );
        }

        let kind = if method.is_async {
            CallPlanKind::Async {
                async_plan: self.build_async_plan(&method.returns),
            }
        } else {
            CallPlanKind::Sync {
                returns: self.lower_return(&method.returns),
            }
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.method_symbol(&class.id, &method.id)),
            params,
            kind,
        }
    }

    fn lower_constructor(&self, class: &ClassDef, ctor: &ConstructorDef) -> CallPlan {
        let params = ctor.params().into_iter().map(|p| self.lower_param(p)).collect();

        let returns = if ctor.is_fallible() {
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Handle {
                    class_id: class.id.clone(),
                    nullable: false,
                },
                err_codec: CodecPlan::String,
            }
        } else {
            ReturnPlan::Value(ReturnValuePlan::Handle {
                class_id: class.id.clone(),
                nullable: false,
            })
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.constructor_symbol(&class.id, ctor.name())),
            params,
            kind: CallPlanKind::Sync { returns },
        }
    }

    fn lower_callback(&self, callback: &CallbackTraitDef) -> Vec<CallPlan> {
        callback
            .methods
            .iter()
            .map(|method| {
                let mut params: Vec<ParamPlan> = method.params.iter().map(|p| self.lower_param(p)).collect();

                params.insert(
                    0,
                    ParamPlan {
                        name: ParamName::new("callback"),
                        strategy: ParamStrategy::Callback {
                            callback_id: callback.id.clone(),
                            style: CallbackStyle::BoxedDyn,
                            nullable: false,
                        },
                    },
                );

                let kind = if method.is_async {
                    CallPlanKind::Async {
                        async_plan: self.build_async_plan(&method.returns),
                    }
                } else {
                    CallPlanKind::Sync {
                        returns: self.lower_return(&method.returns),
                    }
                };

                CallPlan {
                    target: CallTarget::VtableField(naming::vtable_field_name(method.id.as_str())),
                    params,
                    kind,
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Param/Return Lowering
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    fn lower_param(&self, param: &ParamDef) -> ParamPlan {
        ParamPlan {
            name: param.name.clone(),
            strategy: self.param_strategy(&param.type_expr, &param.passing),
        }
    }

    fn param_strategy(&self, type_expr: &TypeExpr, passing: &ParamPassing) -> ParamStrategy {
        if let (ParamPassing::ImplTrait | ParamPassing::BoxedDyn, TypeExpr::Callback(id)) = (passing, type_expr) {
            let style = match passing {
                ParamPassing::ImplTrait => CallbackStyle::ImplTrait,
                ParamPassing::BoxedDyn => CallbackStyle::BoxedDyn,
                _ => unreachable!(),
            };
            return ParamStrategy::Callback {
                callback_id: id.clone(),
                style,
                nullable: false,
            };
        }

        let mutability = match passing {
            ParamPassing::RefMut => Mutability::Mutable,
            _ => Mutability::Shared,
        };

        match type_expr {
            TypeExpr::Void => ParamStrategy::Direct(DirectPlan { abi_type: AbiType::Void }),

            TypeExpr::Primitive(p) => ParamStrategy::Direct(DirectPlan { abi_type: primitive_to_abi(*p) }),

            TypeExpr::String => ParamStrategy::String { mutability },

            TypeExpr::Bytes => ParamStrategy::Buffer {
                element_abi: AbiType::U8,
                mutability,
            },

            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => ParamStrategy::Buffer {
                    element_abi: primitive_to_abi(*p),
                    mutability,
                },
                _ => ParamStrategy::Encoded {
                    codec: self.build_codec(type_expr),
                    mutability,
                },
            },

            TypeExpr::Handle(class_id) => ParamStrategy::Handle {
                class_id: class_id.clone(),
                nullable: false,
            },

            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(class_id) => ParamStrategy::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                TypeExpr::Callback(callback_id) => ParamStrategy::Callback {
                    callback_id: callback_id.clone(),
                    style: CallbackStyle::BoxedDyn,
                    nullable: true,
                },
                _ => ParamStrategy::Encoded {
                    codec: self.build_codec(type_expr),
                    mutability,
                },
            },

            TypeExpr::Callback(callback_id) => ParamStrategy::Callback {
                callback_id: callback_id.clone(),
                style: CallbackStyle::BoxedDyn,
                nullable: false,
            },

            _ => ParamStrategy::Encoded {
                codec: self.build_codec(type_expr),
                mutability,
            },
        }
    }

    fn lower_return(&self, returns: &ReturnDef) -> ReturnPlan {
        match returns {
            ReturnDef::Void => ReturnPlan::Value(ReturnValuePlan::Void),
            ReturnDef::Value(ty) => ReturnPlan::Value(self.lower_value_type(ty)),
            ReturnDef::Result { ok, err } => ReturnPlan::Fallible {
                ok: self.lower_value_type(ok),
                err_codec: self.build_codec(err),
            },
        }
    }

    fn lower_value_type(&self, ty: &TypeExpr) -> ReturnValuePlan {
        match ty {
            TypeExpr::Void => ReturnValuePlan::Void,

            TypeExpr::Primitive(p) => ReturnValuePlan::Direct(DirectPlan { abi_type: primitive_to_abi(*p) }),

            TypeExpr::Handle(class_id) => ReturnValuePlan::Handle {
                class_id: class_id.clone(),
                nullable: false,
            },

            TypeExpr::Callback(callback_id) => ReturnValuePlan::Callback {
                callback_id: callback_id.clone(),
                nullable: false,
            },

            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(class_id) => ReturnValuePlan::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                TypeExpr::Callback(callback_id) => ReturnValuePlan::Callback {
                    callback_id: callback_id.clone(),
                    nullable: true,
                },
                _ => ReturnValuePlan::Encoded { codec: self.build_codec(ty) },
            },

            _ => ReturnValuePlan::Encoded { codec: self.build_codec(ty) },
        }
    }

    fn build_async_plan(&self, returns: &ReturnDef) -> AsyncPlan {
        let result = match returns {
            ReturnDef::Void => AsyncResult::Void,
            ReturnDef::Value(ty) => AsyncResult::Value(self.lower_value_type(ty)),
            ReturnDef::Result { ok, err } => AsyncResult::Fallible {
                ok: self.lower_value_type(ok),
                err_codec: self.build_codec(err),
            },
        };

        AsyncPlan {
            completion_callback: CompletionCallback {
                param_name: ParamName::new("completion"),
                ffi_type: AbiType::Pointer,
            },
            result,
        }
    }
}

impl<'c> Lowerer<'c> {
    fn lower_callback_param(&self, param: &ParamDef) -> AbiCallbackParamPlan {
        let strategy = match &param.type_expr {
            TypeExpr::Primitive(p) => AbiCallbackParamStrategy::Direct(DirectPlan { abi_type: primitive_to_abi(*p) }),
            _ => AbiCallbackParamStrategy::Encoded {
                codec: self.build_codec(&param.type_expr),
            },
        };

        AbiCallbackParamPlan {
            name: param.name.clone(),
            strategy,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Codec Building
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn build_codec(&self, type_expr: &TypeExpr) -> CodecPlan {
        match type_expr {
            TypeExpr::Void => CodecPlan::Void,
            TypeExpr::Primitive(p) => CodecPlan::Primitive(*p),
            TypeExpr::String => CodecPlan::String,
            TypeExpr::Bytes => CodecPlan::Bytes,
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
                let def = self.contract.catalog.resolve_custom(id).expect("custom type should be resolved");
                CodecPlan::Custom {
                    id: id.clone(),
                    underlying: Box::new(self.build_codec(&def.repr)),
                }
            }

            TypeExpr::Handle(_) | TypeExpr::Callback(_) => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    fn record_layout(&self, id: &RecordId) -> RecordLayout {
        if self.record_stack.borrow().contains(id) {
            return RecordLayout::Recursive;
        }

        self.record_stack.borrow_mut().insert(id.clone());

        let def = self.contract.catalog.resolve_record(id).expect("record should be resolved");

        let layout = if self.is_blittable_record(def) {
            self.build_blittable_record_layout(def)
        } else {
            self.build_encoded_record_layout(def)
        };

        self.record_stack.borrow_mut().remove(id);
        layout
    }

    fn is_blittable_record(&self, def: &RecordDef) -> bool {
        def.fields.iter().all(|f| match &f.type_expr {
            TypeExpr::Primitive(p) => !p.is_platform_sized(),
            _ => false,
        })
    }

    fn build_blittable_record_layout(&self, def: &RecordDef) -> RecordLayout {
        let (size, fields) = compute_blittable_layout(def);
        RecordLayout::Blittable { size, fields }
    }

    fn build_encoded_record_layout(&self, def: &RecordDef) -> RecordLayout {
        let fields = def
            .fields
            .iter()
            .map(|f| EncodedField {
                name: f.name.clone(),
                codec: self.build_codec(&f.type_expr),
            })
            .collect();

        RecordLayout::Encoded { fields }
    }

    fn enum_layout(&self, id: &EnumId) -> EnumLayout {
        if self.enum_stack.borrow().contains(id) {
            return EnumLayout::Recursive;
        }

        self.enum_stack.borrow_mut().insert(id.clone());

        let def = self.contract.catalog.resolve_enum(id).expect("enum should be resolved");

        let layout = match &def.repr {
            EnumRepr::CStyle { tag_type, .. } => EnumLayout::CStyle {
                tag_type: *tag_type,
                is_error: def.is_error,
            },

            EnumRepr::Data { tag_type, variants } => EnumLayout::Data {
                tag_type: *tag_type,
                variants: variants
                    .iter()
                    .map(|v| VariantLayout {
                        name: v.name.clone(),
                        discriminant: v.discriminant,
                        payload: self.variant_payload_layout(&v.payload),
                    })
                    .collect(),
            },
        };

        self.enum_stack.borrow_mut().remove(id);
        layout
    }

    fn variant_payload_layout(&self, payload: &VariantPayload) -> VariantPayloadLayout {
        match payload {
            VariantPayload::Unit => VariantPayloadLayout::Unit,
            VariantPayload::Tuple(types) => VariantPayloadLayout::Fields(
                types
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| EncodedField {
                        name: FieldName::new(format!("value_{}", idx)),
                        codec: self.build_codec(ty),
                    })
                    .collect(),
            ),
            VariantPayload::Struct(fields) => VariantPayloadLayout::Fields(
                fields
                    .iter()
                    .map(|f| EncodedField {
                        name: f.name.clone(),
                        codec: self.build_codec(&f.type_expr),
                    })
                    .collect(),
            ),
        }
    }

    fn vec_layout(&self, element: &TypeExpr) -> VecLayout {
        match element {
            TypeExpr::Primitive(p) => match p.size_bytes() {
                Some(size) => VecLayout::Blittable { element_size: size },
                None => VecLayout::Encoded,
            },

            TypeExpr::Record(id) => {
                let def = self.contract.catalog.resolve_record(id);
                match def {
                    Some(def) if self.is_blittable_record(def) => VecLayout::Blittable {
                        element_size: self.blittable_record_size(def),
                    },
                    _ => VecLayout::Encoded,
                }
            }

            _ => VecLayout::Encoded,
        }
    }

    fn blittable_record_size(&self, def: &RecordDef) -> usize {
        let (size, _) = compute_blittable_layout(def);
        size
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Symbol Generation
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    fn function_symbol(&self, id: &FunctionId) -> naming::Name<naming::GlobalSymbol> {
        naming::function_ffi_name(id.as_str())
    }

    fn method_symbol(&self, class_id: &ClassId, method_id: &MethodId) -> naming::Name<naming::GlobalSymbol> {
        naming::method_ffi_name(class_id.as_str(), method_id.as_str())
    }

    fn constructor_symbol(&self, class_id: &ClassId, name: Option<&MethodId>) -> naming::Name<naming::GlobalSymbol> {
        match name {
            Some(n) => naming::method_ffi_name(class_id.as_str(), n.as_str()),
            None => naming::class_ffi_new(class_id.as_str()),
        }
    }

    fn call_symbol(&self, plan: &CallPlan) -> naming::Name<naming::GlobalSymbol> {
        match &plan.target {
            CallTarget::GlobalSymbol(symbol) => symbol.clone(),
            CallTarget::VtableField(_) => panic!("expected global symbol"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn primitive_to_abi(p: PrimitiveType) -> AbiType {
    match p {
        PrimitiveType::Bool => AbiType::Bool,
        PrimitiveType::I8 => AbiType::I8,
        PrimitiveType::U8 => AbiType::U8,
        PrimitiveType::I16 => AbiType::I16,
        PrimitiveType::U16 => AbiType::U16,
        PrimitiveType::I32 => AbiType::I32,
        PrimitiveType::U32 => AbiType::U32,
        PrimitiveType::I64 => AbiType::I64,
        PrimitiveType::U64 => AbiType::U64,
        PrimitiveType::ISize => AbiType::ISize,
        PrimitiveType::USize => AbiType::USize,
        PrimitiveType::F32 => AbiType::F32,
        PrimitiveType::F64 => AbiType::F64,
    }
}

fn align_up(offset: usize, alignment: usize) -> usize {
    (offset + alignment - 1) & !(alignment - 1)
}

fn compute_blittable_layout(def: &RecordDef) -> (usize, Vec<BlittableField>) {
    let (final_offset, fields) = def.fields.iter().fold((0usize, Vec::new()), |(offset, mut fields), field| {
        let TypeExpr::Primitive(p) = &field.type_expr else {
            panic!("blittable record should only have primitive fields");
        };

        let alignment = p.alignment().expect("blittable field must have fixed-size alignment");
        let size = p.size_bytes().expect("blittable field must have fixed size");
        let aligned_offset = align_up(offset, alignment);

        fields.push(BlittableField {
            name: field.name.clone(),
            offset: aligned_offset,
            primitive: *p,
        });

        (aligned_offset + size, fields)
    });

    let max_align = def
        .fields
        .iter()
        .filter_map(|f| match &f.type_expr {
            TypeExpr::Primitive(p) => p.alignment(),
            _ => None,
        })
        .max()
        .unwrap_or(1);

    let size = align_up(final_offset, max_align);
    (size, fields)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        contract::{FfiContract, PackageInfo, TypeCatalog},
        definitions::{CallbackKind, CallbackMethodDef, ClassDef, ConstructorDef, FieldDef, FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef},
        ids::{CallbackId, ClassId, FieldName, FunctionId, MethodId, ParamName, RecordId},
        types::{PrimitiveType, TypeExpr},
    };
    use boltffi_ffi_rules::naming;

    fn test_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog: TypeCatalog::default(),
            functions: vec![],
        }
    }

    fn lowerer_for_contract(contract: &FfiContract) -> Lowerer<'_> {
        Lowerer::new(contract)
    }

    #[test]
    fn param_strategy_primitive_is_direct() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::Primitive(PrimitiveType::I32), &ParamPassing::Value);

        assert!(matches!(strategy, ParamStrategy::Direct(DirectPlan { abi_type: AbiType::I32 })));
    }

    #[test]
    fn param_strategy_string_is_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::String, &ParamPassing::Ref);

        assert!(matches!(strategy, ParamStrategy::String { mutability: Mutability::Shared }));
    }

    #[test]
    fn param_strategy_vec_primitive_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::F32))), &ParamPassing::Ref);

        assert!(matches!(
            strategy,
            ParamStrategy::Buffer {
                element_abi: AbiType::F32,
                mutability: Mutability::Shared
            }
        ));
    }

    #[test]
    fn param_strategy_handle_non_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy = lowerer.param_strategy(&TypeExpr::Handle(class_id.clone()), &ParamPassing::Value);

        assert!(matches!(
            strategy,
            ParamStrategy::Handle { class_id: ref id, nullable: false } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy = lowerer.param_strategy(&TypeExpr::Option(Box::new(TypeExpr::Handle(class_id.clone()))), &ParamPassing::Value);

        assert!(matches!(
            strategy,
            ParamStrategy::Handle { class_id: ref id, nullable: true } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_callback_impl_trait() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.param_strategy(&TypeExpr::Callback(callback_id.clone()), &ParamPassing::ImplTrait);

        assert!(matches!(
            strategy,
            ParamStrategy::Callback {
                callback_id: ref id,
                style: CallbackStyle::ImplTrait,
                nullable: false
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn param_strategy_option_callback_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.param_strategy(&TypeExpr::Option(Box::new(TypeExpr::Callback(callback_id.clone()))), &ParamPassing::Value);

        assert!(matches!(
            strategy,
            ParamStrategy::Callback {
                callback_id: ref id,
                style: CallbackStyle::BoxedDyn,
                nullable: true
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn lower_return_void() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan = lowerer.lower_return(&ReturnDef::Void);

        assert!(matches!(plan, ReturnPlan::Value(ReturnValuePlan::Void)));
    }

    #[test]
    fn lower_return_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool)));

        assert!(matches!(plan, ReturnPlan::Value(ReturnValuePlan::Direct(DirectPlan { abi_type: AbiType::Bool }))));
    }

    #[test]
    fn lower_return_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        assert!(matches!(plan, ReturnPlan::Value(ReturnValuePlan::Handle { nullable: false, .. })));
    }

    #[test]
    fn lower_return_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Handle(class_id)))));

        assert!(matches!(plan, ReturnPlan::Value(ReturnValuePlan::Handle { nullable: true, .. })));
    }

    #[test]
    fn lower_return_result_handle_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Handle { nullable: false, .. },
                ..
            }
        ));
    }

    #[test]
    fn lower_return_result_callback_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Callback(callback_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: ReturnValuePlan::Callback { nullable: false, .. },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Primitive(PrimitiveType::U64));

        assert!(matches!(codec, CodecPlan::Primitive(PrimitiveType::U64)));
    }

    #[test]
    fn build_codec_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::String);

        assert!(matches!(codec, CodecPlan::String));
    }

    #[test]
    fn build_codec_option() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Option(Box::new(TypeExpr::String)));

        assert!(matches!(codec, CodecPlan::Option(_)));
    }

    #[test]
    fn build_codec_vec_primitive_is_blittable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Blittable { element_size: 4 },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_vec_string_is_encoded() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::String)));

        assert!(matches!(codec, CodecPlan::Vec { layout: VecLayout::Encoded, .. }));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_handle_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Handle(ClassId::new("Foo")));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_callback_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Callback(CallbackId::new("Bar")));
    }

    #[test]
    fn lower_function_sync() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Ref,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(
            &plan.target,
            CallTarget::GlobalSymbol(s) if s.as_str() == naming::function_ffi_name("greet").as_str()
        ));
        assert_eq!(plan.params.len(), 1);
        assert!(matches!(plan.kind, CallPlanKind::Sync { .. }));
    }

    #[test]
    fn lower_function_async() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("fetch"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            is_async: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(plan.kind, CallPlanKind::Async { .. }));
    }

    #[test]
    fn lower_method_inserts_self_handle() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Client");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("connect"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 1);
        assert!(matches!(&plan.params[0].strategy, ParamStrategy::Handle { nullable: false, .. }));
        assert_eq!(plan.params[0].name.as_str(), "self");
    }

    #[test]
    fn lower_method_static_no_self() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Utils");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("helper"),
            receiver: Receiver::Static,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 0);
    }

    #[test]
    fn lower_constructor_non_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Builder");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Value(ReturnValuePlan::Handle { nullable: false, .. })
            }
        ));
    }

    #[test]
    fn lower_constructor_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Parser");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("try_new"),
            is_fallible: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { .. }
            }
        ));
    }

    #[test]
    fn lower_callback_uses_vtable_field() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("EventHandler"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("on_event"),
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans.len(), 1);
        assert!(matches!(
            &plans[0].target,
            CallTarget::VtableField(id) if id.as_str() == naming::vtable_field_name("on_event").as_str()
        ));
    }

    #[test]
    fn lower_callback_inserts_callback_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("notify"),
                params: vec![ParamDef {
                    name: ParamName::new("msg"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Ref,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans[0].params.len(), 2);
        assert_eq!(plans[0].params[0].name.as_str(), "callback");
        assert!(matches!(&plans[0].params[0].strategy, ParamStrategy::Callback { nullable: false, .. }));
    }

    #[test]
    fn blittable_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Blittable { size: 8, .. }));
    }

    #[test]
    fn encoded_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Person");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("age"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Encoded { .. }));
    }

    #[test]
    fn async_result_handles_result_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Session");
        let async_plan = lowerer.build_async_plan(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id.clone()),
            err: TypeExpr::String,
        });

        match async_plan.result {
            AsyncResult::Fallible { ok, err_codec } => {
                match ok {
                    ReturnValuePlan::Handle { class_id: id, nullable } => {
                        assert_eq!(id.as_str(), "Session");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Fallible"),
        }
    }

    #[test]
    fn param_strategy_vec_primitive_owned_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))), &ParamPassing::Value);

        match strategy {
            ParamStrategy::Buffer { element_abi, mutability } => {
                assert_eq!(element_abi, AbiType::U8);
                assert_eq!(mutability, Mutability::Shared);
            }
            _ => panic!("expected Buffer for owned Vec<primitive>"),
        }
    }

    #[test]
    fn param_strategy_ref_mut_has_mutable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::String, &ParamPassing::RefMut);

        match strategy {
            ParamStrategy::String { mutability } => {
                assert_eq!(mutability, Mutability::Mutable);
            }
            _ => panic!("expected String"),
        }
    }

    #[test]
    fn param_strategy_bytes_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.param_strategy(&TypeExpr::Bytes, &ParamPassing::Ref);

        match strategy {
            ParamStrategy::Buffer { element_abi, mutability } => {
                assert_eq!(element_abi, AbiType::U8);
                assert_eq!(mutability, Mutability::Shared);
            }
            _ => panic!("expected Buffer for Bytes"),
        }
    }

    #[test]
    fn lower_constructor_fallible_verifies_ok_and_err() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("connect"),
            is_fallible: true,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        match plan.kind {
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { ok, err_codec },
            } => {
                match ok {
                    ReturnValuePlan::Handle { class_id: id, nullable } => {
                        assert_eq!(id.as_str(), "Connection");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle in ok"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Sync Fallible"),
        }
    }

    #[test]
    fn blittable_record_layout_verifies_offsets() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Packed");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Blittable { size, fields } => {
                assert_eq!(size, 12);
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "a");
                assert_eq!(fields[0].offset, 0);
                assert_eq!(fields[0].primitive, PrimitiveType::U8);
                assert_eq!(fields[1].name.as_str(), "b");
                assert_eq!(fields[1].offset, 4);
                assert_eq!(fields[1].primitive, PrimitiveType::U32);
                assert_eq!(fields[2].name.as_str(), "c");
                assert_eq!(fields[2].offset, 8);
                assert_eq!(fields[2].primitive, PrimitiveType::U8);
            }
            _ => panic!("expected Blittable"),
        }
    }

    #[test]
    fn vec_blittable_record_is_blittable() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Vec2");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Record(record_id))));

        match codec {
            CodecPlan::Vec { element, layout } => {
                assert!(matches!(layout, VecLayout::Blittable { element_size: 16 }));
                assert!(matches!(*element, CodecPlan::Record { .. }));
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn build_codec_result() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Result {
            ok: Box::new(TypeExpr::Primitive(PrimitiveType::I64)),
            err: Box::new(TypeExpr::String),
        });

        match codec {
            CodecPlan::Result { ok, err } => {
                assert!(matches!(*ok, CodecPlan::Primitive(PrimitiveType::I64)));
                assert!(matches!(*err, CodecPlan::String));
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn lower_return_verifies_class_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Database");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        match plan {
            ReturnPlan::Value(ReturnValuePlan::Handle { class_id, nullable }) => {
                assert_eq!(class_id.as_str(), "Database");
                assert!(!nullable);
            }
            _ => panic!("expected Value Handle"),
        }
    }

    #[test]
    fn lower_callback_verifies_callback_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("MyCallback"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("invoke"),
                params: vec![],
                returns: ReturnDef::Void,
                is_async: false,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        match &plans[0].params[0].strategy {
            ParamStrategy::Callback { callback_id, style, nullable } => {
                assert_eq!(callback_id.as_str(), "MyCallback");
                assert_eq!(*style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback strategy"),
        }
    }

    #[test]
    fn param_strategy_callback_boxed_dyn() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let strategy = lowerer.param_strategy(&TypeExpr::Callback(callback_id.clone()), &ParamPassing::BoxedDyn);

        match strategy {
            ParamStrategy::Callback { callback_id: id, style, nullable } => {
                assert_eq!(id.as_str(), "Handler");
                assert_eq!(style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback"),
        }
    }

    #[test]
    fn lower_method_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Service");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("start"),
            receiver: Receiver::RefMutSelf,
            params: vec![],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(s.as_str(), naming::method_ffi_name("Service", "start").as_str());
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn lower_constructor_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Factory");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();

        let default_ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &default_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(s.as_str(), naming::class_ffi_new("Factory").as_str())
            }
            _ => panic!("expected GlobalSymbol"),
        }

        let named_ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("with_config"),
            is_fallible: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &named_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(s.as_str(), naming::method_ffi_name("Factory", "with_config").as_str())
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn encoded_record_verifies_field_codecs() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Message");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("id"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("body"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("tags"),
                    type_expr: TypeExpr::Vec(Box::new(TypeExpr::String)),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Encoded { fields } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "id");
                assert!(matches!(fields[0].codec, CodecPlan::Primitive(PrimitiveType::U64)));
                assert_eq!(fields[1].name.as_str(), "body");
                assert!(matches!(fields[1].codec, CodecPlan::String));
                assert_eq!(fields[2].name.as_str(), "tags");
                assert!(matches!(fields[2].codec, CodecPlan::Vec { .. }));
            }
            _ => panic!("expected Encoded"),
        }
    }

    #[test]
    fn string_param_produces_synthetic_len() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let abi = lowerer.abi_call_for_function(&func);

        assert_eq!(abi.params.len(), 2);
        assert!(matches!(abi.params[0].input_shape, InputShape::Utf8Slice { .. }));
        assert_eq!(abi.params[0].name.as_str(), "name");
        match &abi.params[1].input_shape {
            InputShape::HiddenSyntheticLen { for_param } => {
                assert_eq!(for_param.as_str(), "name");
            }
            other => panic!("expected HiddenSyntheticLen, got {:?}", other),
        }
    }

    #[test]
    fn encoded_param_produces_synthetic_len() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("move_to"),
            params: vec![ParamDef {
                name: ParamName::new("point"),
                type_expr: TypeExpr::Record(record_id),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            is_async: false,
            doc: None,
            deprecated: None,
        };

        let abi = lowerer.abi_call_for_function(&func);

        assert_eq!(abi.params.len(), 2);
        assert!(matches!(abi.params[0].input_shape, InputShape::WirePacket { .. }));
        assert_eq!(abi.params[0].name.as_str(), "point");
        match &abi.params[1].input_shape {
            InputShape::HiddenSyntheticLen { for_param } => {
                assert_eq!(for_param.as_str(), "point");
            }
            other => panic!("expected HiddenSyntheticLen, got {:?}", other),
        }
    }

    #[test]
    fn fallible_constructor_produces_nullable_handle_not_panic() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![ConstructorDef::Default {
                params: vec![ParamDef {
                    name: ParamName::new("url"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                is_fallible: true,
                doc: None,
                deprecated: None,
            }],
            methods: vec![],
            streams: vec![],
            async_iterators: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let abi = lowerer.abi_call_for_constructor(class, &class.constructors[0], 0);

        assert!(matches!(abi.output_shape, OutputShape::Handle { nullable: true, .. }));
        assert!(matches!(abi.error, ErrorTransport::Encoded { .. }));
    }
}
