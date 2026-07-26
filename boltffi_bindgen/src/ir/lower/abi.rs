use super::*;
use boltffi_ffi_rules::callable::ExecutionKind;

impl<'c> Lowerer<'c> {
    pub fn to_abi_contract(&self) -> AbiContract {
        let function_calls = self
            .contract
            .functions
            .iter()
            .map(|func| self.abi_call_for_function(func));

        let class_calls = self.contract.catalog.all_classes().flat_map(|class| {
            let ctor_calls = class
                .constructors
                .iter()
                .enumerate()
                .map(|(index, ctor)| self.abi_call_for_constructor(class, ctor, index));
            let method_calls = class
                .methods
                .iter()
                .map(|method| self.abi_call_for_method(class, method));
            ctor_calls.chain(method_calls)
        });

        let value_type_calls = self
            .contract
            .catalog
            .all_records()
            .filter(|r| r.has_methods())
            .map(|r| r as &dyn MethodHost)
            .chain(
                self.contract
                    .catalog
                    .all_enums()
                    .filter(|e| e.has_methods())
                    .map(|e| e as &dyn MethodHost),
            )
            .flat_map(|host| {
                let ctor_calls = host.constructors().iter().enumerate().map(|(index, ctor)| {
                    self.abi_call_for_value_type_constructor(host, ctor, index)
                });
                let method_calls = host
                    .methods()
                    .iter()
                    .filter(|method| method.execution_kind() == ExecutionKind::Sync)
                    .map(|method| self.abi_call_for_value_type_method(host, method));
                ctor_calls.chain(method_calls)
            });

        let calls = function_calls
            .chain(class_calls)
            .chain(value_type_calls)
            .collect();

        let callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .map(|callback| self.abi_callback_invocation(callback))
            .collect();

        let records = self
            .contract
            .catalog
            .all_records()
            .map(|record| self.abi_record(record))
            .collect();

        let enums = self
            .contract
            .catalog
            .all_enums()
            .map(|enumeration| self.abi_enum(enumeration))
            .collect();

        let streams = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| {
                class
                    .streams
                    .iter()
                    .map(|stream| self.abi_stream(&class.id, stream))
            })
            .collect();

        AbiContract {
            package: self.contract.package.clone(),
            calls,
            callbacks,
            streams,
            records,
            enums,
            free_buf: naming::free_buf(),
            atomic_cas: naming::atomic_u8_cas(),
        }
    }

    pub(super) fn abi_call_for_function(&self, func: &FunctionDef) -> AbiCall {
        let plan = self.lower_function(func);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (mode, returns, error) = self.abi_mode_returns_error_for_function(func, &plan.kind);

        AbiCall {
            id: CallId::Function(func.id.clone()),
            symbol,
            mode,
            params,
            returns,
            error,
        }
    }

    pub(super) fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> AbiCall {
        let plan = self.lower_method(class, method);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (mode, returns, error) =
            self.abi_mode_returns_error_for_method(class, method, &plan.kind);

        AbiCall {
            id: CallId::Method {
                class_id: class.id.clone(),
                method_id: method.id.clone(),
            },
            symbol,
            mode,
            params,
            returns,
            error,
        }
    }

    pub(super) fn abi_call_for_constructor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        index: usize,
    ) -> AbiCall {
        let plan = self.lower_constructor(class, ctor);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (returns, error) = self.return_shape_and_error(match &plan.kind {
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
            returns,
            error,
        }
    }

    pub(super) fn abi_call_for_value_type_method(
        &self,
        host: &dyn MethodHost,
        method: &MethodDef,
    ) -> AbiCall {
        let plan = self.lower_value_type_method(host, method);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (returns, error) = self.return_shape_and_error(match &plan.kind {
            CallPlanKind::Sync { returns } => returns,
            CallPlanKind::Async { .. } => unreachable!("value type methods are always sync"),
        });

        AbiCall {
            id: host.method_call_id(&method.id),
            symbol,
            mode: CallMode::Sync,
            params,
            returns,
            error,
        }
    }

    pub(super) fn abi_call_for_value_type_constructor(
        &self,
        host: &dyn MethodHost,
        ctor: &ConstructorDef,
        index: usize,
    ) -> AbiCall {
        let plan = self.lower_value_type_constructor(host, ctor);
        let symbol = self.call_symbol(&plan);
        let params = self.abi_params_from_plan(&plan.params);
        let (returns, error) = self.return_shape_and_error(match &plan.kind {
            CallPlanKind::Sync { returns } => returns,
            CallPlanKind::Async { .. } => unreachable!("value type constructors are always sync"),
        });

        AbiCall {
            id: host.constructor_call_id(index),
            symbol,
            mode: CallMode::Sync,
            params,
            returns,
            error,
        }
    }

    pub(super) fn abi_callback_invocation(
        &self,
        callback: &CallbackTraitDef,
    ) -> AbiCallbackInvocation {
        let methods = callback
            .methods
            .iter()
            .map(|method| {
                let params = self.abi_callback_params(callback, method).collect();
                let (returns, error) = self.callback_return_shape_and_error(&method.returns);

                AbiCallbackMethod {
                    id: method.id.clone(),
                    vtable_field: naming::vtable_field_name(method.id.as_str()),
                    execution_kind: method.execution_kind(),
                    params,
                    returns,
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

    pub(super) fn abi_stream(&self, class_id: &ClassId, stream: &StreamDef) -> AbiStream {
        let class_name = class_id.as_str();
        let stream_name = stream.id.as_str();
        let item_codec = self.build_codec(&stream.item_type);
        let decode_ops = self.expand_decode(&item_codec);
        let item_transport = self.classify_type(&stream.item_type);
        let item_size = match &item_transport {
            Transport::Scalar(origin) => Some(match origin.primitive() {
                PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => 1,
                PrimitiveType::I16 | PrimitiveType::U16 => 2,
                PrimitiveType::I32 | PrimitiveType::U32 | PrimitiveType::F32 => 4,
                PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize
                | PrimitiveType::F64 => 8,
            }),
            Transport::Composite(layout) => Some(layout.total_size),
            _ => None,
        };

        AbiStream {
            class_id: class_id.clone(),
            stream_id: stream.id.clone(),
            mode: stream.mode,
            item: StreamItemTransport::WireEncoded { decode_ops },
            item_transport,
            item_size,
            subscribe: naming::stream_ffi_subscribe(class_name, stream_name),
            poll: naming::stream_ffi_poll(class_name, stream_name),
            pop_batch: naming::stream_ffi_pop_batch(class_name, stream_name),
            wait: naming::stream_ffi_wait(class_name, stream_name),
            unsubscribe: naming::stream_ffi_unsubscribe(class_name, stream_name),
            free: naming::stream_ffi_free(class_name, stream_name),
        }
    }

    pub(super) fn abi_record(&self, record: &RecordDef) -> AbiRecord {
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

    pub(super) fn abi_enum(&self, enumeration: &EnumDef) -> AbiEnum {
        let codec = self.build_codec(&TypeExpr::Enum(enumeration.id.clone()));
        let decode_ops = self.expand_decode(&codec);
        let encode_ops = self.expand_encode(&codec, ValueExpr::Instance);
        let (is_c_style, codec_tag_strategy, variants) = match codec {
            CodecPlan::Enum {
                layout: EnumLayout::CStyle { tag_strategy, .. },
                ..
            } => (
                true,
                tag_strategy,
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
                layout:
                    EnumLayout::Data {
                        tag_strategy,
                        variants,
                        ..
                    },
                ..
            } => (
                false,
                tag_strategy,
                match &enumeration.repr {
                    EnumRepr::Data {
                        variants: data_variants,
                        ..
                    } => {
                        let layout_fields = variants
                            .iter()
                            .map(|variant| {
                                let fields = match &variant.payload {
                                    VariantPayloadLayout::Unit => Vec::new(),
                                    VariantPayloadLayout::Fields(fields) => fields
                                        .iter()
                                        .map(|field| self.abi_enum_field(field))
                                        .collect(),
                                };
                                (variant.name.clone(), fields)
                            })
                            .collect::<HashMap<_, _>>();

                        data_variants
                            .iter()
                            .map(|variant| {
                                let fields = layout_fields
                                    .get(&variant.name)
                                    .cloned()
                                    .unwrap_or_default();
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
            _ => (false, EnumTagStrategy::OrdinalIndex, vec![]),
        };

        AbiEnum {
            id: enumeration.id.clone(),
            decode_ops,
            encode_ops,
            is_c_style,
            codec_tag_strategy,
            variants,
        }
    }

    pub(super) fn abi_enum_field(&self, field: &EncodedField) -> AbiEnumField {
        let decode = self.expand_decode(&field.codec);
        let encode = self.expand_encode(
            &field.codec,
            ValueExpr::Named(field.name.as_str().to_string()),
        );
        AbiEnumField {
            name: field.name.clone(),
            type_expr: TypeExpr::from(&field.codec),
            decode,
            encode,
        }
    }

    pub(super) fn abi_mode_returns_error_for_function(
        &self,
        func: &FunctionDef,
        kind: &CallPlanKind,
    ) -> (CallMode, ReturnShape, ErrorTransport) {
        match kind {
            CallPlanKind::Sync { returns } => {
                let (ret, error) = self.return_shape_and_error(returns);
                (CallMode::Sync, ret, error)
            }
            CallPlanKind::Async { async_plan } => {
                let mode =
                    CallMode::Async(Box::new(self.async_call_for_function(func, async_plan)));
                let ret = ReturnShape {
                    contract: ReturnContract::infallible(ValueReturnStrategy::Scalar(
                        ScalarReturnStrategy::PrimitiveValue,
                    )),
                    transport: Some(Transport::Scalar(ScalarOrigin::Primitive(
                        PrimitiveType::USize,
                    ))),
                    decode_ops: None,
                    encode_ops: None,
                };
                (mode, ret, ErrorTransport::None)
            }
        }
    }

    pub(super) fn abi_mode_returns_error_for_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        kind: &CallPlanKind,
    ) -> (CallMode, ReturnShape, ErrorTransport) {
        match kind {
            CallPlanKind::Sync { returns } => {
                let (ret, error) = self.return_shape_and_error(returns);
                (CallMode::Sync, ret, error)
            }
            CallPlanKind::Async { async_plan } => {
                let mode = CallMode::Async(Box::new(
                    self.async_call_for_method(class, method, async_plan),
                ));
                let ret = ReturnShape {
                    contract: ReturnContract::infallible(ValueReturnStrategy::Scalar(
                        ScalarReturnStrategy::PrimitiveValue,
                    )),
                    transport: Some(Transport::Scalar(ScalarOrigin::Primitive(
                        PrimitiveType::USize,
                    ))),
                    decode_ops: None,
                    encode_ops: None,
                };
                (mode, ret, ErrorTransport::None)
            }
        }
    }

    pub(super) fn async_call_for_function(
        &self,
        func: &FunctionDef,
        plan: &AsyncPlan,
    ) -> AsyncCall {
        let (result, _) = self.return_shape_and_error(&plan.result);
        AsyncCall {
            poll: naming::function_ffi_poll(func.id.as_str()),
            complete: naming::function_ffi_complete(func.id.as_str()),
            cancel: naming::function_ffi_cancel(func.id.as_str()),
            free: naming::function_ffi_free(func.id.as_str()),
            result: result.with_error_strategy(ErrorReturnStrategy::StatusCode),
            error: ErrorTransport::StatusCode,
        }
    }

    pub(super) fn async_call_for_method(
        &self,
        class: &ClassDef,
        method: &MethodDef,
        plan: &AsyncPlan,
    ) -> AsyncCall {
        let (result, _) = self.return_shape_and_error(&plan.result);
        AsyncCall {
            poll: naming::method_ffi_poll(class.id.as_str(), method.id.as_str()),
            complete: naming::method_ffi_complete(class.id.as_str(), method.id.as_str()),
            cancel: naming::method_ffi_cancel(class.id.as_str(), method.id.as_str()),
            free: naming::method_ffi_free(class.id.as_str(), method.id.as_str()),
            result: result.with_error_strategy(ErrorReturnStrategy::StatusCode),
            error: ErrorTransport::StatusCode,
        }
    }

    pub(super) fn return_shape_and_error(
        &self,
        returns: &ReturnPlan,
    ) -> (ReturnShape, ErrorTransport) {
        match returns {
            ReturnPlan::Void => (ReturnShape::void(), ErrorTransport::None),
            ReturnPlan::Value(v) => (self.return_shape_from_transport(v), ErrorTransport::None),
            ReturnPlan::Fallible {
                ok: Transport::Handle { class_id, .. },
                err_codec,
            } => (
                ReturnShape {
                    contract: ReturnContract::new(
                        ValueReturnStrategy::ObjectHandle,
                        ErrorReturnStrategy::Encoded,
                    ),
                    transport: Some(Transport::Handle {
                        class_id: class_id.clone(),
                        nullable: true,
                    }),
                    decode_ops: None,
                    encode_ops: None,
                },
                ErrorTransport::Encoded {
                    decode_ops: self.expand_decode(err_codec),
                    encode_ops: None,
                },
            ),
            ReturnPlan::Fallible { ok, err_codec } => {
                let ok_codec = self.codec_from_transport(ok);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("value".into()));
                let wire_transport = Transport::Span(SpanContent::Encoded(result_codec));
                (
                    ReturnShape::from_transport_with_ops(wire_transport, decode_ops, encode_ops)
                        .with_error_strategy(ErrorReturnStrategy::Encoded),
                    ErrorTransport::Encoded {
                        decode_ops: self.expand_decode(err_codec),
                        encode_ops: None,
                    },
                )
            }
        }
    }

    pub(super) fn return_shape_from_transport(&self, value: &Transport) -> ReturnShape {
        match value {
            Transport::Scalar(origin) => ReturnShape {
                contract: ReturnContract::infallible(value.value_return_strategy()),
                transport: Some(Transport::Scalar(origin.clone())),
                decode_ops: None,
                encode_ops: None,
            },
            Transport::Composite(layout) => {
                let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                let decode_ops = self.expand_decode(&codec);
                let encode_ops = self.expand_encode(&codec, ValueExpr::Var("value".into()));
                ReturnShape {
                    contract: ReturnContract::infallible(value.value_return_strategy()),
                    transport: Some(value.clone()),
                    decode_ops: Some(decode_ops),
                    encode_ops: Some(encode_ops),
                }
            }
            Transport::Span(SpanContent::Composite(_) | SpanContent::Scalar(_)) => ReturnShape {
                contract: ReturnContract::infallible(value.value_return_strategy()),
                transport: Some(value.clone()),
                decode_ops: None,
                encode_ops: None,
            },
            Transport::Span(content) => {
                if let Some(composite_transport) = self.try_promote_to_composite_span(content) {
                    return ReturnShape {
                        contract: ReturnContract::infallible(
                            composite_transport.value_return_strategy(),
                        ),
                        transport: Some(composite_transport),
                        decode_ops: None,
                        encode_ops: None,
                    };
                }
                let codec = self.codec_from_span_content(content);
                let decode_ops = self.expand_decode(&codec);
                let encode_ops = self.expand_encode(&codec, ValueExpr::Var("value".into()));
                ReturnShape::from_transport_with_ops(value.clone(), decode_ops, encode_ops)
            }
            transport @ (Transport::Handle { .. } | Transport::Callback { .. }) => ReturnShape {
                contract: ReturnContract::infallible(transport.value_return_strategy()),
                transport: Some(transport.clone()),
                decode_ops: None,
                encode_ops: None,
            },
        }
    }

    pub(super) fn codec_from_transport(&self, value: &Transport) -> CodecPlan {
        match value {
            Transport::Scalar(ScalarOrigin::CStyleEnum { enum_id, .. }) => {
                self.build_codec(&TypeExpr::Enum(enum_id.clone()))
            }
            Transport::Scalar(origin) => CodecPlan::Primitive(origin.primitive()),
            Transport::Composite(layout) => {
                self.build_codec(&TypeExpr::Record(layout.record_id.clone()))
            }
            Transport::Span(content) => self.codec_from_span_content(content),
            Transport::Handle { .. } | Transport::Callback { .. } => {
                panic!("Handle and Callback types cannot be wire-encoded")
            }
        }
    }

    pub(super) fn try_promote_to_composite_span(&self, content: &SpanContent) -> Option<Transport> {
        let SpanContent::Encoded(CodecPlan::Vec { element, .. }) = content else {
            return None;
        };
        let CodecPlan::Record { id, .. } = element.as_ref() else {
            return None;
        };
        match self.classify_record(id) {
            Transport::Composite(layout) => Some(Transport::Span(SpanContent::Composite(layout))),
            _ => None,
        }
    }

    pub(super) fn codec_from_span_content(&self, content: &SpanContent) -> CodecPlan {
        match content {
            SpanContent::Scalar(origin) => {
                let p = origin.primitive();
                CodecPlan::Vec {
                    element: Box::new(CodecPlan::Primitive(p)),
                    layout: VecLayout::Blittable {
                        element_size: p.wire_size_bytes(),
                    },
                }
            }
            SpanContent::Composite(layout) => {
                let element_codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                CodecPlan::Vec {
                    element: Box::new(element_codec),
                    layout: VecLayout::Blittable {
                        element_size: layout.total_size,
                    },
                }
            }
            SpanContent::Utf8 => CodecPlan::String,
            SpanContent::Encoded(codec) => codec.clone(),
        }
    }

    pub(super) fn abi_params_from_plan(&self, params: &[ParamPlan]) -> Vec<AbiParam> {
        params
            .iter()
            .flat_map(|param| self.abi_param_from_plan(param))
            .collect()
    }

    pub(super) fn abi_param_from_plan(&self, param: &ParamPlan) -> Vec<AbiParam> {
        let len_name = ParamName::new(format!("{}_len", param.name.as_str()));

        let make_span_params = |transport: Transport,
                                contract: ParamContract,
                                mutability: Mutability,
                                decode_ops: Option<ReadSeq>,
                                encode_ops: Option<WriteSeq>|
         -> Vec<AbiParam> {
            let ptr_element = match &transport {
                Transport::Span(SpanContent::Scalar(origin)) => origin.primitive(),
                _ => PrimitiveType::U8,
            };
            vec![
                AbiParam {
                    name: param.name.clone(),
                    abi_type: AbiType::Pointer(ptr_element),
                    role: ParamRole::Input {
                        contract,
                        transport,
                        mutability,
                        len_param: Some(len_name.clone()),
                        decode_ops,
                        encode_ops,
                    },
                },
                AbiParam {
                    name: len_name.clone(),
                    abi_type: AbiType::USize,
                    role: ParamRole::SyntheticLen {
                        for_param: param.name.clone(),
                    },
                },
            ]
        };

        match &param.transport {
            Transport::Scalar(origin) => vec![AbiParam {
                name: param.name.clone(),
                abi_type: AbiType::from(origin.primitive()),
                role: ParamRole::Input {
                    contract: param.contract,
                    transport: param.transport.clone(),
                    mutability: param.mutability,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            Transport::Composite(layout) => {
                let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                let decode_ops = self.expand_decode(&codec);
                let encode_ops =
                    self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                vec![AbiParam {
                    name: param.name.clone(),
                    abi_type: AbiType::Struct(layout.record_id.clone()),
                    role: ParamRole::Input {
                        contract: param.contract,
                        transport: Transport::Composite(layout.clone()),
                        mutability: param.mutability,
                        len_param: None,
                        decode_ops: Some(decode_ops),
                        encode_ops: Some(encode_ops),
                    },
                }]
            }
            span @ Transport::Span(content) => match content {
                SpanContent::Scalar(_) | SpanContent::Utf8 => {
                    make_span_params(span.clone(), param.contract, param.mutability, None, None)
                }
                SpanContent::Composite(_) => {
                    let codec = self.codec_from_span_content(content);
                    let decode_ops = self.expand_decode(&codec);
                    let encode_ops = self
                        .expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                    make_span_params(
                        span.clone(),
                        param.contract,
                        param.mutability,
                        Some(decode_ops),
                        Some(encode_ops),
                    )
                }
                SpanContent::Encoded(codec) => {
                    let decode_ops = self.expand_decode(codec);
                    let encode_ops = self
                        .expand_encode(codec, ValueExpr::Named(param.name.as_str().to_string()));
                    make_span_params(
                        span.clone(),
                        param.contract,
                        param.mutability,
                        Some(decode_ops),
                        Some(encode_ops),
                    )
                }
            },
            Transport::Handle { class_id, .. } => vec![AbiParam {
                name: param.name.clone(),
                abi_type: AbiType::Handle(class_id.clone()),
                role: ParamRole::Input {
                    contract: param.contract,
                    transport: param.transport.clone(),
                    mutability: param.mutability,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            Transport::Callback {
                style: CallbackStyle::BoxedDyn,
                ..
            } => vec![AbiParam {
                name: param.name.clone(),
                abi_type: AbiType::CallbackHandle,
                role: ParamRole::Input {
                    contract: param.contract,
                    transport: param.transport.clone(),
                    mutability: param.mutability,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            Transport::Callback {
                style: CallbackStyle::ImplTrait,
                callback_id,
                ..
            } => {
                let ud_name = ParamName::new(format!("{}_ud", param.name.as_str()));
                let (fn_params, fn_return_type) =
                    self.inline_callback_fn_abi_signature(callback_id);
                vec![
                    AbiParam {
                        name: param.name.clone(),
                        abi_type: AbiType::InlineCallbackFn {
                            params: fn_params,
                            return_type: Box::new(fn_return_type),
                        },
                        role: ParamRole::Input {
                            contract: param.contract,
                            transport: param.transport.clone(),
                            mutability: param.mutability,
                            len_param: Some(ud_name.clone()),
                            decode_ops: None,
                            encode_ops: None,
                        },
                    },
                    AbiParam {
                        name: ud_name,
                        abi_type: AbiType::Pointer(PrimitiveType::U8),
                        role: ParamRole::CallbackContext {
                            for_param: param.name.clone(),
                        },
                    },
                ]
            }
        }
    }

    pub(super) fn inline_callback_fn_abi_signature(
        &self,
        callback_id: &CallbackId,
    ) -> (Vec<AbiType>, AbiType) {
        let callback_def = self
            .contract
            .catalog
            .resolve_callback(callback_id)
            .unwrap_or_else(|| panic!("callback {} not found", callback_id.as_str()));

        let mut abi_params = vec![];

        for method in &callback_def.methods {
            for param in &method.params {
                let transport = self.classify_type(&param.type_expr);
                match &transport {
                    Transport::Scalar(origin) => {
                        abi_params.push(AbiType::from(origin.primitive()));
                    }
                    _ => {
                        abi_params.push(AbiType::Pointer(PrimitiveType::U8));
                        abi_params.push(AbiType::USize);
                    }
                }
            }
        }

        let return_type = callback_def
            .methods
            .first()
            .and_then(|method| match &method.returns {
                ReturnDef::Void => None,
                ReturnDef::Value(ty) => {
                    let transport = self.classify_type(ty);
                    match &transport {
                        Transport::Scalar(origin) => Some(AbiType::from(origin.primitive())),
                        Transport::Composite(layout) => {
                            Some(AbiType::Struct(layout.record_id.clone()))
                        }
                        _ => Some(AbiType::OwnedBuffer),
                    }
                }
                ReturnDef::Result { .. } => Some(AbiType::OwnedBuffer),
            })
            .unwrap_or(AbiType::Void);

        (abi_params, return_type)
    }

    pub(super) fn callback_return_shape_and_error(
        &self,
        returns: &ReturnDef,
    ) -> (ReturnShape, ErrorTransport) {
        match returns {
            ReturnDef::Void => (ReturnShape::void(), ErrorTransport::None),
            ReturnDef::Value(ty) => {
                let transport = self.classify_type(ty);
                let shape = match &transport {
                    Transport::Scalar(_)
                    | Transport::Composite(_)
                    | Transport::Handle { .. }
                    | Transport::Callback { .. } => self.return_shape_from_transport(&transport),
                    _ => {
                        let codec = self.build_codec(ty);
                        let decode_ops = self.expand_decode(&codec);
                        let encode_ops = self.expand_encode(&codec, ValueExpr::Var("value".into()));
                        let wire_transport = Transport::Span(SpanContent::Encoded(codec));
                        ReturnShape::from_transport_with_ops(wire_transport, decode_ops, encode_ops)
                    }
                };
                (shape, ErrorTransport::None)
            }
            ReturnDef::Result { ok, err } => {
                let ok_codec = self.build_codec(ok);
                let err_codec = self.build_codec(err);
                let result_codec = CodecPlan::Result {
                    ok: Box::new(ok_codec),
                    err: Box::new(err_codec.clone()),
                };
                let decode_ops = self.expand_decode(&result_codec);
                let encode_ops = self.expand_encode(&result_codec, ValueExpr::Var("result".into()));
                let wire_transport = Transport::Span(SpanContent::Encoded(result_codec));
                (
                    ReturnShape::from_transport_with_ops(wire_transport, decode_ops, encode_ops)
                        .with_error_strategy(ErrorReturnStrategy::Encoded),
                    ErrorTransport::Encoded {
                        decode_ops: self.expand_decode(&err_codec),
                        encode_ops: Some(
                            self.expand_encode(&err_codec, ValueExpr::Var("error".into())),
                        ),
                    },
                )
            }
        }
    }

    pub(super) fn abi_callback_params<'a>(
        &'a self,
        callback: &'a CallbackTraitDef,
        method: &'a CallbackMethodDef,
    ) -> impl Iterator<Item = AbiParam> + 'a {
        let handle_param = AbiParam {
            name: ParamName::new("handle"),
            abi_type: AbiType::Pointer(PrimitiveType::U8),
            role: ParamRole::Input {
                contract: ParamContract::new(
                    ParamValueStrategy::CallbackHandle {
                        nullable: false,
                        style: CallbackParamStyle::BoxedDyn,
                    },
                    ParamPassingStrategy::ByValue,
                ),
                transport: Transport::Callback {
                    callback_id: callback.id.clone(),
                    nullable: false,
                    style: CallbackStyle::BoxedDyn,
                },
                mutability: Mutability::Shared,
                len_param: None,
                decode_ops: None,
                encode_ops: None,
            },
        };

        let method_params = method
            .params
            .iter()
            .map(|param| self.lower_callback_param(param))
            .flat_map(|param| self.abi_callback_param_from_plan(param));

        let out_params = self.abi_callback_out_params(&method.returns, method.execution_kind());

        std::iter::once(handle_param)
            .chain(method_params)
            .chain(out_params)
    }

    pub(super) fn abi_callback_param_from_plan(
        &self,
        param: AbiCallbackParamPlan,
    ) -> Vec<AbiParam> {
        let len_name = ParamName::new(format!("{}_len", param.name.as_str()));

        match param.strategy {
            AbiCallbackParamStrategy::Scalar(origin) => vec![AbiParam {
                name: param.name,
                abi_type: AbiType::from(origin.primitive()),
                role: ParamRole::Input {
                    contract: ParamContract::new(
                        ParamValueStrategy::Scalar(ScalarParamStrategy::PrimitiveValue),
                        ParamPassingStrategy::ByValue,
                    ),
                    transport: Transport::Scalar(origin),
                    mutability: Mutability::Shared,
                    len_param: None,
                    decode_ops: None,
                    encode_ops: None,
                },
            }],
            AbiCallbackParamStrategy::Direct(layout) => {
                let codec = self.build_codec(&TypeExpr::Record(layout.record_id.clone()));
                let decode_ops = self.expand_decode(&codec);
                let encode_ops =
                    self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                vec![AbiParam {
                    name: param.name.clone(),
                    abi_type: AbiType::Struct(layout.record_id.clone()),
                    role: ParamRole::Input {
                        contract: ParamContract::new(
                            ParamValueStrategy::CompositeValue,
                            ParamPassingStrategy::ByValue,
                        ),
                        transport: Transport::Composite(layout),
                        mutability: Mutability::Shared,
                        len_param: None,
                        decode_ops: Some(decode_ops),
                        encode_ops: Some(encode_ops),
                    },
                }]
            }
            AbiCallbackParamStrategy::Encoded { codec } => {
                let decode_ops = self.expand_decode(&codec);
                let encode_ops =
                    self.expand_encode(&codec, ValueExpr::Named(param.name.as_str().to_string()));
                vec![
                    AbiParam {
                        name: param.name.clone(),
                        abi_type: AbiType::Pointer(PrimitiveType::U8),
                        role: ParamRole::Input {
                            contract: ParamContract::new(
                                ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue),
                                ParamPassingStrategy::ByValue,
                            ),
                            transport: Transport::Span(SpanContent::Encoded(codec)),
                            mutability: Mutability::Shared,
                            len_param: Some(len_name.clone()),
                            decode_ops: Some(decode_ops),
                            encode_ops: Some(encode_ops),
                        },
                    },
                    AbiParam {
                        name: len_name,
                        abi_type: AbiType::USize,
                        role: ParamRole::SyntheticLen {
                            for_param: param.name,
                        },
                    },
                ]
            }
        }
    }

    pub(super) fn abi_callback_out_params(
        &self,
        returns: &ReturnDef,
        execution_kind: ExecutionKind,
    ) -> Vec<AbiParam> {
        let has_return =
            !matches!(returns, ReturnDef::Void) && execution_kind == ExecutionKind::Sync;
        let out_ptr_name = ParamName::new("out_ptr");
        let out_len_name = ParamName::new("out_len");

        if !has_return {
            return Vec::new();
        }

        let (ret, _) = self.callback_return_shape_and_error(returns);

        match &ret.transport {
            Some(Transport::Scalar(origin)) => vec![AbiParam {
                name: out_ptr_name,
                abi_type: AbiType::from(origin.primitive()),
                role: ParamRole::OutDirect,
            }],
            Some(Transport::Handle { .. } | Transport::Callback { .. }) | None => {
                vec![AbiParam {
                    name: out_ptr_name,
                    abi_type: AbiType::Pointer(PrimitiveType::U8),
                    role: ParamRole::OutDirect,
                }]
            }
            Some(_) => {
                vec![
                    AbiParam {
                        name: out_ptr_name.clone(),
                        abi_type: AbiType::Pointer(PrimitiveType::U8),
                        role: ParamRole::OutDirect,
                    },
                    AbiParam {
                        name: out_len_name,
                        abi_type: AbiType::USize,
                        role: ParamRole::OutLen {
                            for_param: out_ptr_name,
                        },
                    },
                ]
            }
        }
    }
}
