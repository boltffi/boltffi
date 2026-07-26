use super::codec::compute_blittable_layout;
use super::*;
use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};

#[derive(Debug, Clone)]
pub(super) struct AbiCallbackParamPlan {
    pub(super) name: ParamName,
    pub(super) strategy: AbiCallbackParamStrategy,
}

#[derive(Debug, Clone)]
pub(super) enum AbiCallbackParamStrategy {
    Scalar(ScalarOrigin),
    Direct(CompositeLayout),
    Encoded { codec: CodecPlan },
}

impl<'c> Lowerer<'c> {
    pub(super) fn lower_function(&self, func: &FunctionDef) -> CallPlan {
        let params = func
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let kind = match func.execution_kind() {
            ExecutionKind::Async => CallPlanKind::Async {
                async_plan: self.build_async_plan(&func.returns),
            },
            ExecutionKind::Sync => CallPlanKind::Sync {
                returns: self.lower_return(&func.returns),
            },
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.function_symbol(&func.id)),
            params,
            kind,
        }
    }

    pub(super) fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> CallPlan {
        let mut params: Vec<ParamPlan> = method
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        if method.callable_form() == CallableForm::InstanceMethod {
            params.insert(
                0,
                ParamPlan {
                    name: ParamName::new("self"),
                    contract: ParamContract::new(
                        ParamValueStrategy::ObjectHandle { nullable: false },
                        ParamPassingStrategy::ByValue,
                    ),
                    transport: Transport::Handle {
                        class_id: class.id.clone(),
                        nullable: false,
                    },
                    mutability: Mutability::Shared,
                },
            );
        }

        let kind = match method.execution_kind() {
            ExecutionKind::Async => CallPlanKind::Async {
                async_plan: self.build_async_plan(&method.returns),
            },
            ExecutionKind::Sync => CallPlanKind::Sync {
                returns: self.lower_return(&method.returns),
            },
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(self.method_symbol(&class.id, &method.id)),
            params,
            kind,
        }
    }

    pub(super) fn lower_constructor(&self, class: &ClassDef, ctor: &ConstructorDef) -> CallPlan {
        let params = ctor
            .params()
            .into_iter()
            .map(|param| self.lower_param(param))
            .collect();

        let returns = if ctor.is_fallible() {
            ReturnPlan::Fallible {
                ok: Transport::Handle {
                    class_id: class.id.clone(),
                    nullable: false,
                },
                err_codec: CodecPlan::String,
            }
        } else {
            ReturnPlan::Value(Transport::Handle {
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

    pub(super) fn lower_value_type_method(
        &self,
        host: &dyn MethodHost,
        method: &MethodDef,
    ) -> CallPlan {
        let mut params: Vec<ParamPlan> = method
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();

        let is_mut_receiver = method.receiver == Receiver::RefMutSelf;

        if method.receiver != Receiver::Static {
            let mutability = if is_mut_receiver {
                Mutability::Mutable
            } else {
                Mutability::Shared
            };
            let self_transport = host.classify(self);
            params.insert(
                0,
                ParamPlan {
                    name: ParamName::new("self"),
                    contract: ParamContract::new(
                        self.classify_param_value_strategy(&host.type_expr()),
                        if is_mut_receiver {
                            ParamPassingStrategy::MutableRef
                        } else {
                            ParamPassingStrategy::SharedRef
                        },
                    ),
                    transport: self_transport,
                    mutability,
                },
            );
        }

        let returns = if is_mut_receiver && matches!(method.returns, ReturnDef::Void) {
            ReturnPlan::Value(host.classify(self))
        } else {
            self.lower_return(&method.returns)
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(host.method_symbol(&method.id, self)),
            params,
            kind: CallPlanKind::Sync { returns },
        }
    }

    pub(super) fn lower_value_type_constructor(
        &self,
        host: &dyn MethodHost,
        ctor: &ConstructorDef,
    ) -> CallPlan {
        let params = ctor
            .params()
            .into_iter()
            .map(|param| self.lower_param(param))
            .collect();

        let transport = host.classify(self);

        let returns = if ctor.is_fallible() {
            ReturnPlan::Fallible {
                ok: transport,
                err_codec: CodecPlan::String,
            }
        } else if ctor.is_optional() {
            let inner_codec = self.codec_from_transport(&transport);
            let option_codec = CodecPlan::Option(Box::new(inner_codec));
            ReturnPlan::Value(Transport::Span(SpanContent::Encoded(option_codec)))
        } else {
            ReturnPlan::Value(transport)
        };

        CallPlan {
            target: CallTarget::GlobalSymbol(host.constructor_symbol(ctor.name(), self)),
            params,
            kind: CallPlanKind::Sync { returns },
        }
    }

    pub(super) fn lower_callback(&self, callback: &CallbackTraitDef) -> Vec<CallPlan> {
        callback
            .methods
            .iter()
            .map(|method| {
                let mut params: Vec<ParamPlan> = method
                    .params
                    .iter()
                    .map(|param| self.lower_param(param))
                    .collect();

                params.insert(
                    0,
                    ParamPlan {
                        name: ParamName::new("callback"),
                        contract: ParamContract::new(
                            ParamValueStrategy::CallbackHandle {
                                nullable: false,
                                style: CallbackParamStyle::BoxedDyn,
                            },
                            ParamPassingStrategy::ByValue,
                        ),
                        transport: Transport::Callback {
                            callback_id: callback.id.clone(),
                            style: CallbackStyle::BoxedDyn,
                            nullable: false,
                        },
                        mutability: Mutability::Shared,
                    },
                );

                let kind = match method.execution_kind() {
                    ExecutionKind::Async => CallPlanKind::Async {
                        async_plan: self.build_async_plan(&method.returns),
                    },
                    ExecutionKind::Sync => CallPlanKind::Sync {
                        returns: self.lower_return(&method.returns),
                    },
                };

                CallPlan {
                    target: CallTarget::VtableField(naming::vtable_field_name(method.id.as_str())),
                    params,
                    kind,
                }
            })
            .collect()
    }

    pub(super) fn lower_param(&self, param: &ParamDef) -> ParamPlan {
        let mutability = match param.passing {
            ParamPassing::RefMut => Mutability::Mutable,
            _ => Mutability::Shared,
        };
        ParamPlan {
            name: param.name.clone(),
            contract: self.classify_param_contract(&param.type_expr, &param.passing),
            transport: self.classify_param(&param.type_expr, &param.passing),
            mutability,
        }
    }

    pub(super) fn classify_param_contract(
        &self,
        type_expr: &TypeExpr,
        passing: &ParamPassing,
    ) -> ParamContract {
        if let (ParamPassing::ImplTrait | ParamPassing::BoxedDyn, TypeExpr::Callback(_)) =
            (passing, type_expr)
        {
            let style = match passing {
                ParamPassing::ImplTrait => CallbackParamStyle::ImplTrait,
                ParamPassing::BoxedDyn => CallbackParamStyle::BoxedDyn,
                _ => unreachable!(),
            };
            return ParamContract::new(
                ParamValueStrategy::CallbackHandle {
                    nullable: false,
                    style,
                },
                self.classify_param_passing_strategy(passing),
            );
        }

        ParamContract::new(
            self.classify_param_value_strategy(type_expr),
            self.classify_param_passing_strategy(passing),
        )
    }

    pub(super) fn classify_param_passing_strategy(
        &self,
        passing: &ParamPassing,
    ) -> ParamPassingStrategy {
        match passing {
            ParamPassing::Value | ParamPassing::ImplTrait | ParamPassing::BoxedDyn => {
                ParamPassingStrategy::ByValue
            }
            ParamPassing::Ref => ParamPassingStrategy::SharedRef,
            ParamPassing::RefMut => ParamPassingStrategy::MutableRef,
        }
    }

    pub(super) fn classify_param_value_strategy(&self, type_expr: &TypeExpr) -> ParamValueStrategy {
        match type_expr {
            TypeExpr::Primitive(_) => {
                ParamValueStrategy::Scalar(ScalarParamStrategy::PrimitiveValue)
            }
            TypeExpr::Enum(id) => match self
                .contract
                .catalog
                .resolve_enum(id)
                .map(|enumeration| &enumeration.repr)
            {
                Some(EnumRepr::CStyle { .. }) => {
                    ParamValueStrategy::Scalar(ScalarParamStrategy::CStyleEnumTag)
                }
                Some(EnumRepr::Data { .. }) | None => {
                    ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue)
                }
            },
            TypeExpr::String | TypeExpr::Str => ParamValueStrategy::Utf8String,
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => {
                    ParamValueStrategy::DirectBuffer(DirectBufferParamStrategy::ScalarElements)
                }
                TypeExpr::Record(record_id) => match self.classify_record(record_id) {
                    Transport::Composite(_) => ParamValueStrategy::DirectBuffer(
                        DirectBufferParamStrategy::CompositeElements,
                    ),
                    _ => ParamValueStrategy::WireEncoded(WireParamStrategy::Vec),
                },
                _ => ParamValueStrategy::WireEncoded(WireParamStrategy::Vec),
            },
            TypeExpr::Handle(_) => ParamValueStrategy::ObjectHandle { nullable: false },
            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(_) => ParamValueStrategy::ObjectHandle { nullable: true },
                TypeExpr::Callback(_) => ParamValueStrategy::CallbackHandle {
                    nullable: true,
                    style: CallbackParamStyle::BoxedDyn,
                },
                _ => ParamValueStrategy::WireEncoded(WireParamStrategy::Option),
            },
            TypeExpr::Callback(_) => ParamValueStrategy::CallbackHandle {
                nullable: false,
                style: CallbackParamStyle::BoxedDyn,
            },
            TypeExpr::Record(id) => match self.classify_record(id) {
                Transport::Composite(_) => ParamValueStrategy::CompositeValue,
                _ => ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue),
            },
            TypeExpr::Result { .. } | TypeExpr::Custom(_) | TypeExpr::Builtin(_) => {
                ParamValueStrategy::WireEncoded(WireParamStrategy::SingleValue)
            }
            TypeExpr::Void => ParamValueStrategy::Scalar(ScalarParamStrategy::PrimitiveValue),
        }
    }

    pub(super) fn classify_param(&self, type_expr: &TypeExpr, passing: &ParamPassing) -> Transport {
        if let (ParamPassing::ImplTrait | ParamPassing::BoxedDyn, TypeExpr::Callback(id)) =
            (passing, type_expr)
        {
            let style = match passing {
                ParamPassing::ImplTrait => CallbackStyle::ImplTrait,
                ParamPassing::BoxedDyn => CallbackStyle::BoxedDyn,
                _ => unreachable!(),
            };
            return Transport::Callback {
                callback_id: id.clone(),
                style,
                nullable: false,
            };
        }

        self.classify_type(type_expr)
    }

    pub(super) fn classify_type(&self, type_expr: &TypeExpr) -> Transport {
        match type_expr {
            TypeExpr::Primitive(primitive) => {
                Transport::Scalar(ScalarOrigin::Primitive(*primitive))
            }
            TypeExpr::Enum(id) => self.classify_enum(id),
            TypeExpr::String => Transport::Span(SpanContent::Utf8),
            TypeExpr::Str => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(primitive) => {
                    Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(*primitive)))
                }
                TypeExpr::Enum(_) => {
                    Transport::Span(SpanContent::Encoded(self.build_codec(type_expr)))
                }
                TypeExpr::Record(record_id) => match self.classify_record(record_id) {
                    Transport::Composite(layout) => Transport::Span(SpanContent::Composite(layout)),
                    _ => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
                },
                _ => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
            },
            TypeExpr::Handle(class_id) => Transport::Handle {
                class_id: class_id.clone(),
                nullable: false,
            },
            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(class_id) => Transport::Handle {
                    class_id: class_id.clone(),
                    nullable: true,
                },
                TypeExpr::Callback(callback_id) => Transport::Callback {
                    callback_id: callback_id.clone(),
                    style: CallbackStyle::BoxedDyn,
                    nullable: true,
                },
                _ => Transport::Span(SpanContent::Encoded(self.build_codec(type_expr))),
            },
            TypeExpr::Callback(callback_id) => Transport::Callback {
                callback_id: callback_id.clone(),
                style: CallbackStyle::BoxedDyn,
                nullable: false,
            },
            TypeExpr::Record(id) => self.classify_record(id),
            TypeExpr::Result { .. } | TypeExpr::Custom(_) | TypeExpr::Builtin(_) => {
                Transport::Span(SpanContent::Encoded(self.build_codec(type_expr)))
            }
            TypeExpr::Void => Transport::Scalar(ScalarOrigin::Primitive(PrimitiveType::U8)),
        }
    }

    pub(super) fn classify_enum(&self, id: &EnumId) -> Transport {
        let definition = self
            .contract
            .catalog
            .resolve_enum(id)
            .unwrap_or_else(|| panic!("unresolved enum: {}", id.as_str()));

        match &definition.repr {
            EnumRepr::CStyle { tag_type, .. } => Transport::Scalar(ScalarOrigin::CStyleEnum {
                tag_type: *tag_type,
                enum_id: id.clone(),
            }),
            EnumRepr::Data { .. } => Transport::Span(SpanContent::Encoded(
                self.build_codec(&TypeExpr::Enum(id.clone())),
            )),
        }
    }

    pub(super) fn classify_record(&self, id: &RecordId) -> Transport {
        let definition = self
            .contract
            .catalog
            .resolve_record(id)
            .unwrap_or_else(|| panic!("unresolved record: {}", id.as_str()));

        if self.is_blittable_record(definition) {
            let (total_size, blittable_fields) = compute_blittable_layout(definition);
            let fields = blittable_fields
                .into_iter()
                .map(|field| CompositeField {
                    name: field.name,
                    offset: field.offset,
                    primitive: field.primitive,
                })
                .collect();
            Transport::Composite(CompositeLayout {
                record_id: id.clone(),
                total_size,
                fields,
            })
        } else {
            Transport::Span(SpanContent::Encoded(
                self.build_codec(&TypeExpr::Record(id.clone())),
            ))
        }
    }

    pub(super) fn lower_return(&self, returns: &ReturnDef) -> ReturnPlan {
        match returns {
            ReturnDef::Void => ReturnPlan::Void,
            ReturnDef::Value(type_expr) => ReturnPlan::Value(self.classify_type(type_expr)),
            ReturnDef::Result { ok, err } => ReturnPlan::Fallible {
                ok: self.classify_type(ok),
                err_codec: self.build_codec(err),
            },
        }
    }

    pub(super) fn build_async_plan(&self, returns: &ReturnDef) -> AsyncPlan {
        AsyncPlan {
            completion_callback: CompletionCallback {
                param_name: ParamName::new("completion"),
                abi_type: AbiType::Pointer(PrimitiveType::U8),
            },
            result: self.lower_return(returns),
        }
    }

    pub(super) fn lower_callback_param(&self, param: &ParamDef) -> AbiCallbackParamPlan {
        let strategy = match self.classify_type(&param.type_expr) {
            Transport::Scalar(origin) => AbiCallbackParamStrategy::Scalar(origin),
            _ => AbiCallbackParamStrategy::Encoded {
                codec: self.build_codec(&param.type_expr),
            },
        };

        AbiCallbackParamPlan {
            name: param.name.clone(),
            strategy,
        }
    }

    pub(super) fn function_symbol(&self, id: &FunctionId) -> naming::Name<naming::GlobalSymbol> {
        naming::function_ffi_name(id.as_str())
    }

    pub(super) fn method_symbol(
        &self,
        class_id: &ClassId,
        method_id: &MethodId,
    ) -> naming::Name<naming::GlobalSymbol> {
        naming::method_ffi_name(class_id.as_str(), method_id.as_str())
    }

    pub(super) fn constructor_symbol(
        &self,
        class_id: &ClassId,
        name: Option<&MethodId>,
    ) -> naming::Name<naming::GlobalSymbol> {
        match name {
            Some(name) => naming::method_ffi_name(class_id.as_str(), name.as_str()),
            None => naming::class_ffi_new(class_id.as_str()),
        }
    }

    pub(super) fn call_symbol(&self, plan: &CallPlan) -> naming::Name<naming::GlobalSymbol> {
        match &plan.target {
            CallTarget::GlobalSymbol(symbol) => symbol.clone(),
            CallTarget::VtableField(_) => panic!("expected global symbol"),
        }
    }
}
