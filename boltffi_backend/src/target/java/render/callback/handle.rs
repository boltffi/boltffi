use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandleMethod {
    name: Identifier,
    parameters: Vec<Parameter<ValueType>>,
    returns: ReturnType,
    body: Vec<Statement>,
    asynchronous: Option<AsyncHandleMethod>,
    wire_runtime: bool,
    direct_vector_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncHandleMethod {
    success: Identifier,
    failure: Identifier,
    result: Option<Parameter<ValueType>>,
    completion: Vec<Statement>,
}

struct HandleParameter {
    public: Parameter<ValueType>,
    acquire: Vec<Statement>,
    prepare: Vec<Statement>,
    arguments: Vec<Expression>,
    cleanup: Vec<Statement>,
    wire_runtime: bool,
    direct_vector_runtime: bool,
}

struct HandleParameterRender<'context> {
    source: Name,
    name: Identifier,
    version: JavaVersion,
    context: &'context RenderContext<'context, Native>,
}

enum HandleReturnConversion {
    Void,
    Direct,
    DirectRecord(TypeName),
    DirectEnum(TypeName),
    Encoded(boltffi_binding::ReadPlan),
    ScalarOption(Primitive),
    DirectVector(DirectVector),
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
}

struct HandleReturn {
    public: ReturnType,
    conversion: HandleReturnConversion,
    wire_runtime: bool,
    direct_vector_runtime: bool,
}

struct HandleReturnRender<'context> {
    version: JavaVersion,
    context: &'context RenderContext<'context, Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackHandle {
    ty: TypeName,
    bridge: TypeName,
    carrier: Primitive,
    presence: HandlePresence,
    version: JavaVersion,
}

impl HandleMethod {
    pub fn from_declaration(
        source: &ImportedMethodDecl<Native, VTableSlot>,
        method: &JniCallbackHandleMethod,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if source.target().as_str() != method.slot().as_str() {
            return Err(JavaHost::broken_bridge_contract(
                "callback handle method matches the callback declaration",
            ));
        }
        let completion = match source.callable().execution() {
            ExecutionDecl::Synchronous(_) if method.completion().is_none() => None,
            ExecutionDecl::Asynchronous(native::AsyncProtocol::CallbackCompletion) => {
                Some(method.completion().ok_or(JavaHost::broken_bridge_contract(
                    "asynchronous callback handle method has a JNI completion",
                ))?)
            }
            ExecutionDecl::Synchronous(_) => {
                return Err(JavaHost::broken_bridge_contract(
                    "synchronous callback handle method has no JNI completion",
                ));
            }
            ExecutionDecl::Asynchronous(_) => {
                return Err(JavaHost::unsupported(
                    "asynchronous callback handle protocol",
                ));
            }
            _ => return Err(JavaHost::unsupported("callback handle method execution")),
        };
        if !matches!(source.callable().error().channel(), ErrorChannel::None) {
            return Err(JavaHost::unsupported("fallible callback handle method"));
        }
        let parameters = source
            .callable()
            .params()
            .iter()
            .map(|parameter| HandleParameter::from_declaration(parameter, version, context))
            .collect::<Result<Vec<_>>>()?;
        let returned = source
            .callable()
            .returns()
            .plan()
            .render_with(&mut HandleReturnRender { version, context })?;
        let native = NativeMethod::from_callback_handle_method(method, version)?;
        let callback_data = completion
            .map(|completion| Identifier::parse_for(completion.context().as_str(), version))
            .transpose()?;
        let call = native.call(
            &TypeIdentifier::known("Native", version),
            std::iter::once(
                Expression::this().call(Identifier::known("rawHandle"), ArgumentList::default()),
            )
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.arguments.iter().cloned()),
            )
            .chain(
                callback_data
                    .as_ref()
                    .map(|name| Expression::identifier(name.clone())),
            ),
        )?;
        let protected = parameters
            .iter()
            .flat_map(|parameter| parameter.prepare.iter().cloned())
            .chain(match completion {
                Some(_) => vec![Statement::expression(call)],
                None => returned.statements(call, version, context)?,
            })
            .collect::<Vec<_>>();
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup.iter().cloned())
            .collect::<Vec<_>>();
        let protected = match cleanup.is_empty() {
            true => protected,
            false => vec![Statement::try_finally(protected, cleanup)],
        };
        let asynchronous = completion
            .map(|completion| AsyncHandleMethod::new(completion, &returned, version, context))
            .transpose()?;
        let body = match (&asynchronous, callback_data) {
            (Some(_), Some(callback_data)) => {
                let future = Identifier::known("__boltffi_future");
                let failure = Identifier::known("__boltffi_failure");
                let ReturnType::Value(ValueType::Reference(future_type)) =
                    returned.public.future(version)
                else {
                    return Err(JavaHost::broken_bridge_contract(
                        "asynchronous callback handle return is a future",
                    ));
                };
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.acquire.iter().cloned())
                    .chain([
                        Statement::value(
                            future_type.clone(),
                            future.clone(),
                            Expression::construct(future_type, ArgumentList::default()),
                        ),
                        Statement::value(
                            TypeName::primitive(Primitive::Long),
                            callback_data.clone(),
                            Expression::static_call(
                                TypeName::named(TypeIdentifier::known(
                                    "BoltFfiCallbackFutures",
                                    version,
                                )),
                                Identifier::known("insert"),
                                [Expression::identifier(future.clone())]
                                    .into_iter()
                                    .collect(),
                            ),
                        ),
                        Statement::try_catch(
                            protected,
                            TypeName::named(TypeIdentifier::known("Throwable", version)),
                            failure.clone(),
                            vec![Self::failure_statement(
                                Expression::identifier(callback_data),
                                Expression::identifier(failure),
                                version,
                            )],
                        ),
                        Statement::return_value(Expression::identifier(future)),
                    ])
                    .collect()
            }
            (None, None) => parameters
                .iter()
                .flat_map(|parameter| parameter.acquire.iter().cloned())
                .chain(protected)
                .collect(),
            _ => {
                return Err(JavaHost::broken_bridge_contract(
                    "callback handle completion and context agree",
                ));
            }
        };
        Ok(Self {
            name: Name::new(source.name()).function(version)?,
            parameters: parameters
                .iter()
                .map(|parameter| parameter.public.clone())
                .collect(),
            returns: match asynchronous.as_ref() {
                Some(_) => returned.public.future(version),
                None => returned.public.clone(),
            },
            body,
            asynchronous,
            wire_runtime: returned.wire_runtime
                || parameters.iter().any(|parameter| parameter.wire_runtime),
            direct_vector_runtime: returned.direct_vector_runtime
                || parameters
                    .iter()
                    .any(|parameter| parameter.direct_vector_runtime),
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[Parameter<ValueType>] {
        &self.parameters
    }

    pub fn returns(&self) -> &ReturnType {
        &self.returns
    }

    pub fn body(&self) -> &[Statement] {
        &self.body
    }

    pub fn asynchronous(&self) -> Option<&AsyncHandleMethod> {
        self.asynchronous.as_ref()
    }

    pub fn is_asynchronous(&self) -> bool {
        self.asynchronous.is_some()
    }

    pub fn requires_wire_runtime(&self) -> bool {
        self.wire_runtime
    }

    pub fn requires_direct_vector_runtime(&self) -> bool {
        self.direct_vector_runtime
    }

    fn failure_statement(
        callback_data: Expression,
        failure: Expression,
        version: JavaVersion,
    ) -> Statement {
        Statement::expression(Expression::static_call(
            TypeName::named(TypeIdentifier::known("BoltFfiCallbackFutures", version)),
            Identifier::known("failure"),
            [callback_data, failure].into_iter().collect(),
        ))
    }
}

impl AsyncHandleMethod {
    pub fn success(&self) -> &Identifier {
        &self.success
    }

    pub fn failure(&self) -> &Identifier {
        &self.failure
    }

    pub fn result(&self) -> Option<&Parameter<ValueType>> {
        self.result.as_ref()
    }

    pub fn completion(&self) -> &[Statement] {
        &self.completion
    }

    fn new(
        completion: &CallbackHandleCompletion,
        returned: &HandleReturn,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let result = completion.payload().map(|payload| {
            Parameter::new(
                Identifier::known("result"),
                Self::payload_type(payload.value()),
            )
        });
        Ok(Self {
            success: Identifier::parse_for(completion.success_method().as_str(), version)?,
            failure: Identifier::parse_for(completion.failure_method().as_str(), version)?,
            completion: returned.completion_statements(
                result
                    .as_ref()
                    .map(|result| Expression::identifier(result.name().clone())),
                completion.payload().map(|payload| payload.value()),
                version,
                context,
            )?,
            result,
        })
    }

    fn payload_type(payload: CallbackCompletionPayloadValue) -> ValueType {
        match payload {
            CallbackCompletionPayloadValue::Scalar(ty) => ValueType::Primitive(Primitive::from(ty)),
            CallbackCompletionPayloadValue::Bytes | CallbackCompletionPayloadValue::Record => {
                ValueType::Reference(TypeName::array(TypeName::primitive(Primitive::Byte)))
            }
            CallbackCompletionPayloadValue::CallbackHandle => ValueType::Primitive(Primitive::Long),
        }
    }
}

impl HandleParameter {
    fn from_declaration(
        parameter: &ParamDecl<Native, OutOfRust>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source = Name::new(parameter.name());
        let name = source.parameter(version)?;
        let plan = parameter
            .payload()
            .as_value()
            .ok_or(JavaHost::unsupported("callback handle closure parameter"))?;
        plan.render_with(&mut HandleParameterRender {
            source,
            name,
            version,
            context,
        })
    }

    fn direct(public: Parameter<ValueType>, argument: Expression) -> Self {
        Self {
            public,
            acquire: Vec::new(),
            prepare: Vec::new(),
            arguments: vec![argument],
            cleanup: Vec::new(),
            wire_runtime: false,
            direct_vector_runtime: false,
        }
    }

    fn encoded(
        public: Parameter<ValueType>,
        write: crate::target::java::codec::EncodedWrite,
    ) -> Self {
        let (acquire, prepare, arguments, cleanup) = write.into_parts();
        Self {
            public,
            acquire,
            prepare,
            arguments,
            cleanup,
            wire_runtime: true,
            direct_vector_runtime: false,
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for HandleParameterRender<'_> {
    type Output = Result<HandleParameter>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match ty {
            DirectValueType::Primitive(primitive) => Ok(HandleParameter::direct(
                Parameter::new(
                    self.name.clone(),
                    ValueType::Primitive(Primitive::try_from(*primitive)?),
                ),
                value,
            )),
            DirectValueType::Record(record) => {
                let record = Record::type_name_for(*record, self.context, self.version)?;
                Ok(HandleParameter::direct(
                    Parameter::new(
                        self.name.clone(),
                        ValueType::Reference(TypeName::named(record)),
                    ),
                    value.call(Identifier::known("toDirectBuffer"), ArgumentList::default()),
                ))
            }
            DirectValueType::Enum(enumeration) => {
                let enumeration =
                    Enumeration::type_name_for(*enumeration, self.context, self.version)?;
                Ok(HandleParameter::direct(
                    Parameter::new(
                        self.name.clone(),
                        ValueType::Reference(TypeName::named(enumeration)),
                    ),
                    value.call(Identifier::known("nativeValue"), ArgumentList::default()),
                ))
            }
            _ => Err(JavaHost::unsupported("callback handle direct parameter")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        _: native::BufferShape,
        _: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let plan = codec.write_self_value();
        let write = WireBuffer::new(&self.source, self.version)?.write(
            &plan,
            Expression::identifier(self.name.clone()),
            self.context,
        )?;
        Ok(HandleParameter::encoded(
            Parameter::new(
                self.name.clone(),
                ValueType::Reference(JavaType::type_ref(ty, self.version, self.context)?),
            ),
            write,
        ))
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        _: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match target {
            HandleTarget::Class(class) => {
                let handle =
                    ClassHandle::new(*class, carrier, presence, self.version, self.context, None)?;
                Ok(HandleParameter::direct(
                    Parameter::new(self.name.clone(), ValueType::Reference(handle.ty().clone())),
                    handle.native_argument(value)?,
                ))
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(
                    *callback,
                    carrier,
                    presence,
                    self.version,
                    self.context,
                    None,
                )?;
                Ok(HandleParameter::direct(
                    Parameter::new(self.name.clone(), ValueType::Reference(handle.ty().clone())),
                    handle.native_argument(value)?,
                ))
            }
            _ => Err(JavaHost::unsupported("callback handle parameter")),
        }
    }

    fn scalar_option(&mut self, primitive: BindingPrimitive) -> Self::Output {
        let primitive = Primitive::try_from(primitive)?;
        let value = Expression::identifier(self.name.clone());
        let writer = self.source.generated("writer", self.version)?;
        let payload = self.source.generated("value", self.version)?;
        let write = WireBuffer::new(&self.source, self.version)?.write_statements(
            Expression::integer(1).add(
                value
                    .clone()
                    .call(Identifier::known("isPresent"), ArgumentList::default())
                    .conditional(
                        Expression::integer(primitive.wire_size()),
                        Expression::integer(0),
                    ),
            ),
            vec![Statement::expression(
                Expression::identifier(writer.clone()).call(
                    Identifier::known("writeOptional"),
                    [
                        value,
                        Expression::lambda_statement(
                            [payload.clone()],
                            Statement::expression(Expression::identifier(writer).call(
                                Identifier::parse_for(
                                    format!("write{}", primitive.wire_method_suffix()),
                                    self.version,
                                )?,
                                [Expression::identifier(payload)].into_iter().collect(),
                            )),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            )],
        )?;
        Ok(HandleParameter::encoded(
            Parameter::new(
                self.name.clone(),
                ValueType::Reference(JavaType::optional_primitive(primitive, self.version)),
            ),
            write,
        ))
    }

    fn direct_vector(
        &mut self,
        element: &'plan boltffi_binding::DirectVectorElementType,
        _: (),
    ) -> Self::Output {
        let vector = DirectVector::from_element(element, self.version, self.context)?;
        Ok(HandleParameter {
            public: Parameter::new(self.name.clone(), ValueType::Reference(vector.ty().clone())),
            acquire: Vec::new(),
            prepare: Vec::new(),
            arguments: vec![vector.native_argument(Expression::identifier(self.name.clone()))],
            cleanup: Vec::new(),
            wire_runtime: false,
            direct_vector_runtime: true,
        })
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for HandleReturnRender<'_> {
    type Output = Result<HandleReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(HandleReturn {
            public: ReturnType::Void,
            conversion: HandleReturnConversion::Void,
            wire_runtime: false,
            direct_vector_runtime: false,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Err(JavaHost::unsupported("callback handle out-pointer return"));
        }
        match ty {
            DirectValueType::Primitive(primitive) => Ok(HandleReturn {
                public: ReturnType::Value(ValueType::Primitive(Primitive::try_from(*primitive)?)),
                conversion: HandleReturnConversion::Direct,
                wire_runtime: false,
                direct_vector_runtime: false,
            }),
            DirectValueType::Record(record) => {
                let record = Record::type_name_for(*record, self.context, self.version)?;
                Ok(HandleReturn {
                    public: ReturnType::Value(ValueType::Reference(TypeName::named(
                        record.clone(),
                    ))),
                    conversion: HandleReturnConversion::DirectRecord(TypeName::named(record)),
                    wire_runtime: false,
                    direct_vector_runtime: false,
                })
            }
            DirectValueType::Enum(enumeration) => {
                let enumeration =
                    Enumeration::type_name_for(*enumeration, self.context, self.version)?;
                Ok(HandleReturn {
                    public: ReturnType::Value(ValueType::Reference(TypeName::named(
                        enumeration.clone(),
                    ))),
                    conversion: HandleReturnConversion::DirectEnum(TypeName::named(enumeration)),
                    wire_runtime: false,
                    direct_vector_runtime: false,
                })
            }
            _ => Err(JavaHost::unsupported("callback handle direct return")),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        _: native::BufferShape,
    ) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Err(JavaHost::unsupported(
                "callback handle encoded out-pointer return",
            ));
        }
        Ok(HandleReturn {
            public: ReturnType::Value(ValueType::Reference(JavaType::type_ref(
                ty,
                self.version,
                self.context,
            )?)),
            conversion: HandleReturnConversion::Encoded(codec.read_plan()),
            wire_runtime: true,
            direct_vector_runtime: false,
        })
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Err(JavaHost::unsupported(
                "callback handle value out-pointer return",
            ));
        }
        match target {
            HandleTarget::Class(class) => {
                let handle =
                    ClassHandle::new(*class, carrier, presence, self.version, self.context, None)?;
                Ok(HandleReturn {
                    public: ReturnType::Value(ValueType::Reference(handle.ty().clone())),
                    conversion: HandleReturnConversion::ClassHandle(handle),
                    wire_runtime: false,
                    direct_vector_runtime: false,
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(
                    *callback,
                    carrier,
                    presence,
                    self.version,
                    self.context,
                    None,
                )?;
                Ok(HandleReturn {
                    public: ReturnType::Value(ValueType::Reference(handle.ty().clone())),
                    conversion: HandleReturnConversion::CallbackHandle(handle),
                    wire_runtime: false,
                    direct_vector_runtime: false,
                })
            }
            _ => Err(JavaHost::unsupported("callback handle value return")),
        }
    }

    fn scalar_option(&mut self, primitive: BindingPrimitive) -> Self::Output {
        let primitive = Primitive::try_from(primitive)?;
        Ok(HandleReturn {
            public: ReturnType::Value(ValueType::Reference(JavaType::optional_primitive(
                primitive,
                self.version,
            ))),
            conversion: HandleReturnConversion::ScalarOption(primitive),
            wire_runtime: true,
            direct_vector_runtime: false,
        })
    }

    fn direct_vector(
        &mut self,
        element: &'plan boltffi_binding::DirectVectorElementType,
    ) -> Self::Output {
        let vector = DirectVector::from_element(element, self.version, self.context)?;
        Ok(HandleReturn {
            public: ReturnType::Value(ValueType::Reference(vector.ty().clone())),
            conversion: HandleReturnConversion::DirectVector(vector),
            wire_runtime: false,
            direct_vector_runtime: true,
        })
    }

    fn closure(
        &mut self,
        _: &'plan boltffi_binding::ClosureReturn<Native, IntoRust>,
    ) -> Self::Output {
        Err(JavaHost::unsupported("callback handle closure return"))
    }
}

impl HandleReturn {
    fn completion_statements(
        &self,
        value: Option<Expression>,
        payload: Option<CallbackCompletionPayloadValue>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        let required = || {
            value.clone().ok_or(JavaHost::broken_bridge_contract(
                "callback handle completion has its result parameter",
            ))
        };
        match (&self.conversion, payload) {
            (HandleReturnConversion::Void, None) => {
                Ok(vec![Self::complete(Expression::null(), version)])
            }
            (HandleReturnConversion::Direct, Some(CallbackCompletionPayloadValue::Scalar(_))) => {
                Ok(vec![Self::complete(required()?, version)])
            }
            (
                HandleReturnConversion::DirectRecord(record),
                Some(CallbackCompletionPayloadValue::Record),
            ) => Ok(vec![Self::complete(
                Expression::static_call(
                    record.clone(),
                    Identifier::known("fromByteArray"),
                    [required()?].into_iter().collect(),
                ),
                version,
            )]),
            (
                HandleReturnConversion::DirectEnum(enumeration),
                Some(CallbackCompletionPayloadValue::Scalar(_)),
            ) => Ok(vec![Self::complete(
                Expression::static_call(
                    enumeration.clone(),
                    Identifier::known("fromValue"),
                    [required()?].into_iter().collect(),
                ),
                version,
            )]),
            (
                HandleReturnConversion::Encoded(codec),
                Some(CallbackCompletionPayloadValue::Bytes),
            ) => {
                let reader = Identifier::known("__boltffi_reader");
                let decoded = codec
                    .render_with(&mut Reader::new(reader.clone(), version, context))?
                    .into_expression();
                Ok(vec![
                    Statement::value(
                        TypeName::named(TypeIdentifier::known("WireReader", version)),
                        reader,
                        Expression::construct(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            [required()?].into_iter().collect(),
                        ),
                    ),
                    Self::complete(decoded, version),
                ])
            }
            (
                HandleReturnConversion::ScalarOption(primitive),
                Some(CallbackCompletionPayloadValue::Bytes),
            ) => {
                let reader = Identifier::known("__boltffi_reader");
                let reader_value = Expression::identifier(reader.clone());
                Ok(vec![
                    Statement::value(
                        TypeName::named(TypeIdentifier::known("WireReader", version)),
                        reader,
                        Expression::construct(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            [required()?].into_iter().collect(),
                        ),
                    ),
                    Self::complete(
                        reader_value.clone().call(
                            Identifier::known("readOptional"),
                            [Expression::lambda(
                                [],
                                reader_value.call(
                                    Identifier::parse_for(
                                        format!("read{}", primitive.wire_method_suffix()),
                                        version,
                                    )?,
                                    ArgumentList::default(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        version,
                    ),
                ])
            }
            (
                HandleReturnConversion::DirectVector(vector),
                Some(CallbackCompletionPayloadValue::Bytes),
            ) => Ok(vec![Self::complete(
                vector.returned_expression(required()?),
                version,
            )]),
            (
                HandleReturnConversion::ClassHandle(handle),
                Some(CallbackCompletionPayloadValue::Scalar(_)),
            ) => Ok(vec![Self::complete(
                handle.value_expression(required()?)?,
                version,
            )]),
            (
                HandleReturnConversion::CallbackHandle(handle),
                Some(CallbackCompletionPayloadValue::CallbackHandle),
            ) => Ok(vec![Self::complete(
                handle.value_expression(required()?)?,
                version,
            )]),
            _ => Err(JavaHost::broken_bridge_contract(
                "callback handle return matches its completion payload",
            )),
        }
    }

    fn complete(value: Expression, version: JavaVersion) -> Statement {
        Statement::expression(Expression::static_call(
            TypeName::named(TypeIdentifier::known("BoltFfiCallbackFutures", version)),
            Identifier::known("success"),
            [
                Expression::identifier(Identifier::known("callbackData")),
                value,
            ]
            .into_iter()
            .collect(),
        ))
    }

    fn statements(
        &self,
        call: Expression,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match &self.conversion {
            HandleReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            HandleReturnConversion::Direct => Ok(vec![Statement::return_value(call)]),
            HandleReturnConversion::DirectRecord(record) => {
                Ok(vec![Statement::return_value(Expression::static_call(
                    record.clone(),
                    Identifier::known("fromByteArray"),
                    [call].into_iter().collect(),
                ))])
            }
            HandleReturnConversion::DirectEnum(enumeration) => {
                Ok(vec![Statement::return_value(Expression::static_call(
                    enumeration.clone(),
                    Identifier::known("fromValue"),
                    [call].into_iter().collect(),
                ))])
            }
            HandleReturnConversion::Encoded(codec) => {
                let bytes = Identifier::known("__boltffi_result");
                let reader = Identifier::known("__boltffi_reader");
                let decoded = codec
                    .render_with(&mut Reader::new(reader.clone(), version, context))?
                    .into_expression();
                Ok(vec![
                    Statement::value(
                        TypeName::array(TypeName::primitive(Primitive::Byte)),
                        bytes.clone(),
                        call,
                    ),
                    Statement::value(
                        TypeName::named(TypeIdentifier::known("WireReader", version)),
                        reader,
                        Expression::construct(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            [Expression::identifier(bytes)].into_iter().collect(),
                        ),
                    ),
                    Statement::return_value(decoded),
                ])
            }
            HandleReturnConversion::ScalarOption(primitive) => {
                let bytes = Identifier::known("__boltffi_result");
                let reader = Identifier::known("__boltffi_reader");
                let reader_value = Expression::identifier(reader.clone());
                Ok(vec![
                    Statement::value(
                        TypeName::array(TypeName::primitive(Primitive::Byte)),
                        bytes.clone(),
                        call,
                    ),
                    Statement::value(
                        TypeName::named(TypeIdentifier::known("WireReader", version)),
                        reader,
                        Expression::construct(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            [Expression::identifier(bytes)].into_iter().collect(),
                        ),
                    ),
                    Statement::return_value(
                        reader_value.clone().call(
                            Identifier::known("readOptional"),
                            [Expression::lambda(
                                [],
                                reader_value.call(
                                    Identifier::parse_for(
                                        format!("read{}", primitive.wire_method_suffix()),
                                        version,
                                    )?,
                                    ArgumentList::default(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    ),
                ])
            }
            HandleReturnConversion::DirectVector(vector) => {
                let bytes = Identifier::known("__boltffi_result");
                Ok(vec![
                    Statement::value(
                        TypeName::array(TypeName::primitive(Primitive::Byte)),
                        bytes.clone(),
                        call,
                    ),
                    Statement::return_value(
                        vector.returned_expression(Expression::identifier(bytes)),
                    ),
                ])
            }
            HandleReturnConversion::ClassHandle(handle) => handle.value_statements(call),
            HandleReturnConversion::CallbackHandle(handle) => handle.value_statements(call),
        }
    }
}

impl CallbackHandle {
    pub fn new(
        id: CallbackId,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        version: JavaVersion,
        context: &RenderContext<Native>,
        package: Option<&JavaPackage>,
    ) -> Result<Self> {
        let name = Callback::type_name_for(id, context, version)?;
        let callback = match package {
            Some(package) => package.type_name(name.clone()),
            None => TypeName::named(name.clone()),
        };
        let ty = match presence {
            HandlePresence::Required => callback,
            HandlePresence::Nullable => JavaType::optional(callback, version),
            _ => return Err(JavaHost::unsupported("callback handle presence")),
        };
        let bridge_name = TypeIdentifier::parse(format!("{name}Bridge"), version)?;
        let bridge = match package {
            Some(package) => package.type_name(bridge_name),
            None => TypeName::named(bridge_name),
        };
        Ok(Self {
            ty,
            bridge,
            carrier: Primitive::from_handle_carrier(carrier)?,
            presence,
            version,
        })
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub const fn carrier(&self) -> Primitive {
        self.carrier
    }

    pub fn native_argument(&self, value: Expression) -> Result<Expression> {
        let create = Identifier::known("create");
        let required = Expression::static_call(
            self.bridge.clone(),
            create.clone(),
            [value.clone()].into_iter().collect(),
        );
        match self.presence {
            HandlePresence::Required => Ok(required),
            HandlePresence::Nullable => Ok(value
                .clone()
                .call(Identifier::known("isPresent"), ArgumentList::default())
                .conditional(
                    Expression::static_call(
                        self.bridge.clone(),
                        create,
                        [value.call(Identifier::known("get"), ArgumentList::default())]
                            .into_iter()
                            .collect(),
                    ),
                    Expression::long(0),
                )),
            _ => Err(JavaHost::unsupported("callback handle presence")),
        }
    }

    pub fn value_statements(&self, value: Expression) -> Result<Vec<Statement>> {
        let wrap = |value| {
            Expression::static_call(
                self.bridge.clone(),
                Identifier::known("wrap"),
                [value].into_iter().collect::<ArgumentList>(),
            )
        };
        match self.presence {
            HandlePresence::Required => Ok(vec![Statement::return_value(wrap(value))]),
            HandlePresence::Nullable => {
                let handle = Identifier::known("__boltffi_handle");
                Ok(vec![
                    Statement::value(TypeName::primitive(self.carrier), handle.clone(), value),
                    Statement::return_value(
                        Expression::identifier(handle.clone())
                            .equal(Expression::long(0))
                            .conditional(
                                self.empty(),
                                self.present(wrap(Expression::identifier(handle))),
                            ),
                    ),
                ])
            }
            _ => Err(JavaHost::unsupported("callback handle presence")),
        }
    }

    pub fn value_expression(&self, value: Expression) -> Result<Expression> {
        let wrapped = Expression::static_call(
            self.bridge.clone(),
            Identifier::known("wrap"),
            [value.clone()].into_iter().collect(),
        );
        match self.presence {
            HandlePresence::Required => Ok(wrapped),
            HandlePresence::Nullable => Ok(value
                .equal(Expression::long(0))
                .conditional(self.empty(), self.present(wrapped))),
            _ => Err(JavaHost::unsupported("callback handle presence")),
        }
    }

    fn empty(&self) -> Expression {
        Expression::static_call(
            JavaType::optional_type(self.version),
            Identifier::known("empty"),
            ArgumentList::default(),
        )
    }

    fn present(&self, value: Expression) -> Expression {
        Expression::static_call(
            JavaType::optional_type(self.version),
            Identifier::known("of"),
            [value].into_iter().collect(),
        )
    }
}
