use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackDecl, CallbackId, ClosureReturn, DirectValueType, DirectVectorElementType, Direction,
    EnumId, ErrorChannel, ErrorPlacement, ExecutionDecl, HandlePresence, HandleTarget,
    ImportedMethodDecl, IntoRust, Native, OutOfRust, OutgoingParam, ParamDecl, ParamPlanRender,
    Primitive, ReadPlan, ReturnPlanRender, ReturnValueSlot, Surface, TypeRef, VTableSlot,
    WritePlan,
};

use crate::{
    bridge::jni::{
        CallbackCompletionArgument, CallbackCompletionInvoker, CallbackCompletionPayloadValue,
        CallbackHandleMethod as JniCallbackHandleMethod, CallbackMethod as JniCallbackMethod,
        CallbackRegistration, JniBridgeContract, SuccessOutArgument,
    },
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        KotlinApiStyle, KotlinHost,
        codec::{Reader, ScalarOption, WireBuffer},
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            class::ClassHandle,
            direct_vector::DirectVector,
            enumeration::Enumeration,
            jvm_invocation,
            native::NativeCall,
            record::Record,
            signature::{Parameter, validate_reserved_members},
            type_name::KotlinType,
        },
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/callback.kt", escape = "none")]
struct CallbackTemplate {
    callback: Callback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Callback {
    name: TypeName,
    map_name: TypeName,
    callbacks_name: TypeName,
    bridge_name: TypeName,
    handle_name: TypeName,
    handle_release: Option<Identifier>,
    methods: Vec<Method>,
    handle_methods: Vec<HandleMethod>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method {
    name: Identifier,
    jvm_name: Identifier,
    public_parameters: Vec<Parameter>,
    jvm_parameters: Vec<Parameter>,
    public_return: Option<TypeName>,
    jvm_return: Option<TypeName>,
    setup: Vec<Statement>,
    call_return: Vec<Statement>,
    asynchronous: bool,
    async_body: Option<AsyncMethodBody>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandleMethod {
    name: Identifier,
    parameters: Vec<Parameter>,
    returns: Option<TypeName>,
    setup: Vec<Statement>,
    call: Vec<Statement>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncMethodBody {
    statements: Vec<Statement>,
    failure: Statement,
}

pub struct CallbackHandle {
    ty: TypeName,
    bridge: TypeName,
    handle: TypeName,
    presence: HandlePresence,
}

struct CallbackRegistrationSet<'bridge> {
    registrations: &'bridge [CallbackRegistration],
}

struct ReturnRender<'context> {
    source_name: Name,
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
    fallible_success_out: bool,
}

struct HandleParameterRender<'context> {
    source_name: Name,
    name: Identifier,
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
}

struct HandleReturnRender<'context> {
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
}

struct ReturnValue {
    public_ty: Option<TypeName>,
    jvm_ty: Option<TypeName>,
    conversion: ReturnConversion,
}

enum ReturnConversion {
    Void,
    Direct(DirectValueType),
    DirectRecord,
    DirectEnum {
        repr: Primitive,
    },
    Encoded {
        codec: <IntoRust as Direction>::Codec,
        source_name: Name,
    },
    ScalarOption {
        primitive: Primitive,
        source_name: Name,
    },
    DirectVector(DirectVector),
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
}

struct FallibleReturn<'error> {
    source_name: Name,
    success_out: Option<SuccessOutArgument>,
    error_ty: &'error TypeRef,
    error_codec: &'error WritePlan,
}

struct HandleParameter {
    public: Parameter,
    native_arguments: Vec<Expression>,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

struct HandleReturn {
    ty: Option<TypeName>,
    conversion: HandleReturnConversion,
}

enum HandleReturnConversion {
    Void,
    Direct(Primitive),
    DirectRecord(TypeName),
    DirectEnum(TypeName),
    DirectVector(DirectVector),
    Encoded(ReadPlan),
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
    ScalarOption(Primitive),
}

struct AsyncCompletion {
    success: Identifier,
    failure: Identifier,
    error: Option<Identifier>,
    payload: Option<CallbackCompletionPayloadValue>,
    callback: Identifier,
    context: Identifier,
}

impl Callback {
    pub fn from_declaration(
        decl: &CallbackDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let registration = CallbackRegistrationSet::new(bridge.callbacks()).get(decl)?;
        let name = Name::new(decl.name()).type_name();
        if decl.protocol().vtable().methods().len() != registration.methods().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "callback declaration method count does not match JNI registration",
            });
        }
        let methods = decl
            .protocol()
            .vtable()
            .methods()
            .iter()
            .zip(registration.methods())
            .map(|(source, method)| Method::from_declaration(source, method, host, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        if !registration.handle_methods().is_empty()
            && decl.protocol().vtable().methods().len() != registration.handle_methods().len()
        {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "callback declaration method count does not match JNI handle methods",
            });
        }
        let handle_methods = decl
            .protocol()
            .vtable()
            .methods()
            .iter()
            .zip(registration.handle_methods())
            .map(|(source, method)| HandleMethod::from_declaration(source, method, host, context))
            .collect::<Result<Vec<_>>>()?;
        let handle_name = TypeName::new(format!("{name}Handle"));
        validate_reserved_members(
            &handle_name,
            &["close", "rawHandle", "requireOpen"],
            handle_methods
                .iter()
                .filter(|method| method.parameters().is_empty())
                .map(HandleMethod::name),
        )?;
        let handle_release = bridge
            .callback_handle_lifecycle()
            .map(|lifecycle| Identifier::escape(lifecycle.release_method().as_str()))
            .transpose()?;
        if !handle_methods.is_empty() && handle_release.is_none() {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "callback handle methods have no lifecycle methods",
            });
        }
        Ok(Self {
            map_name: TypeName::new(format!("{name}Map")),
            callbacks_name: TypeName::new(format!("{name}Callbacks")),
            bridge_name: TypeName::new(format!("{name}Bridge")),
            handle_name,
            handle_release,
            name,
            methods,
            handle_methods,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            CallbackTemplate { callback: self }.render()?,
        ))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn map_name(&self) -> &TypeName {
        &self.map_name
    }

    pub fn callbacks_name(&self) -> &TypeName {
        &self.callbacks_name
    }

    pub fn bridge_name(&self) -> &TypeName {
        &self.bridge_name
    }

    pub fn handle_name(&self) -> &TypeName {
        &self.handle_name
    }

    pub fn handle_release(&self) -> Option<&Identifier> {
        self.handle_release.as_ref()
    }

    pub fn methods(&self) -> &[Method] {
        &self.methods
    }

    /// Kotlin permits the `fun` modifier only on interfaces with exactly one
    /// abstract method; a `fun interface` gains SAM conversion, so callers can
    /// pass a lambda instead of an `object :` expression.
    pub fn fun_interface(&self) -> bool {
        self.methods.len() == 1
    }

    pub fn handle_methods(&self) -> &[HandleMethod] {
        &self.handle_methods
    }
}

impl Method {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn jvm_name(&self) -> &Identifier {
        &self.jvm_name
    }

    pub fn public_parameters(&self) -> &[Parameter] {
        &self.public_parameters
    }

    pub fn jvm_parameters(&self) -> &[Parameter] {
        &self.jvm_parameters
    }

    pub fn public_return(&self) -> Option<&TypeName> {
        self.public_return.as_ref()
    }

    pub fn jvm_return(&self) -> Option<&TypeName> {
        self.jvm_return.as_ref()
    }

    pub fn call_return(&self) -> &[Statement] {
        &self.call_return
    }

    pub fn setup(&self) -> &[Statement] {
        &self.setup
    }

    pub fn asynchronous(&self) -> bool {
        self.asynchronous
    }

    pub fn async_body(&self) -> Option<&AsyncMethodBody> {
        self.async_body.as_ref()
    }
}

impl AsyncMethodBody {
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    pub fn failure(&self) -> &Statement {
        &self.failure
    }
}

impl Method {
    fn from_declaration(
        source: &ImportedMethodDecl<Native, VTableSlot>,
        method: &JniCallbackMethod,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let callable = source.callable();
        if source.target().as_str() != method.method().as_str() {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "callback declaration method does not match JNI registration method",
            });
        }
        let asynchronous = match callable.execution() {
            ExecutionDecl::Synchronous(_) => false,
            ExecutionDecl::Asynchronous(_) => true,
            _ => {
                return Err(KotlinHost::unsupported("unknown callback method execution"));
            }
        };
        let async_completion = if asynchronous {
            Some(AsyncCompletion::from_method(method, bridge)?)
        } else {
            None
        };
        let parameters = callable
            .params()
            .iter()
            .map(|parameter| jvm_invocation::Parameter::from_declaration(parameter, host, context))
            .collect::<Result<Vec<_>>>()?;
        let source_name = Name::new(source.name());
        let fallible =
            FallibleReturn::from_method(source_name.clone(), callable.error().channel(), method)?;
        let return_value = callable.returns().plan().render_with(&mut ReturnRender {
            source_name,
            host,
            context,
            fallible_success_out: fallible.is_some(),
        })?;
        let implementation = Expression::identifier(Identifier::parse("impl")?);
        let interface_call = Expression::call(
            implementation,
            Name::new(source.name()).function()?,
            parameters
                .iter()
                .map(|parameter| parameter.argument().clone())
                .collect::<ArgumentList>(),
        );
        let public_return = return_value.public_ty.clone();
        let jvm_parameters = fallible
            .as_ref()
            .map(FallibleReturn::parameter)
            .transpose()?
            .into_iter()
            .flatten()
            .chain(parameters.iter().map(|parameter| parameter.jvm().clone()))
            .chain(
                async_completion
                    .as_ref()
                    .map(AsyncCompletion::parameters)
                    .unwrap_or_default(),
            )
            .collect();
        let call_return = match (&fallible, &async_completion) {
            (_, Some(_)) => Vec::new(),
            (Some(fallible), None) => return_value.throwing_fallible_statements(
                interface_call.clone(),
                fallible,
                host,
                context,
            )?,
            (None, None) => return_value.statements(interface_call.clone(), host, context)?,
        };
        let async_body = async_completion
            .as_ref()
            .map(|completion| {
                AsyncMethodBody::from_call(
                    interface_call,
                    &return_value,
                    fallible.as_ref(),
                    completion,
                    host,
                    context,
                )
            })
            .transpose()?;
        Ok(Self {
            name: Name::new(source.name()).function()?,
            jvm_name: Identifier::escape(method.method().as_str())?,
            public_parameters: parameters
                .iter()
                .map(|parameter| parameter.public().clone())
                .collect(),
            setup: parameters
                .iter()
                .flat_map(|parameter| parameter.setup().iter().cloned())
                .collect(),
            jvm_parameters,
            public_return,
            jvm_return: if asynchronous {
                None
            } else {
                fallible
                    .as_ref()
                    .map(|_| TypeName::byte_array(false))
                    .or_else(|| return_value.jvm_ty.clone())
            },
            call_return,
            asynchronous,
            async_body,
        })
    }
}

impl AsyncMethodBody {
    fn from_call(
        call: Expression,
        return_value: &ReturnValue,
        fallible: Option<&FallibleReturn<'_>>,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            statements: match fallible {
                Some(fallible) => return_value.throwing_completion_fallible_statements(
                    call, fallible, completion, host, context,
                )?,
                None => {
                    return_value.completion_success_statements(call, completion, host, context)?
                }
            },
            failure: completion.failure_statement(),
        })
    }
}

impl AsyncCompletion {
    fn from_method(method: &JniCallbackMethod, bridge: &JniBridgeContract) -> Result<Self> {
        let completions = method.completions();
        let completion = match completions.as_slice() {
            [completion] => completion,
            [] => {
                return Err(Error::BrokenBridgeContract {
                    bridge: "jni",
                    invariant: "async callback method has no JNI completion argument",
                });
            }
            _ => {
                return Err(Error::BrokenBridgeContract {
                    bridge: "jni",
                    invariant: "async callback method has more than one JNI completion argument",
                });
            }
        };
        let invoker = Self::invoker(completion, bridge)?;
        Ok(Self {
            success: Identifier::escape(invoker.success_method().as_str())?,
            failure: Identifier::escape(invoker.failure_method().as_str())?,
            error: invoker
                .error_method()
                .map(|method| Identifier::escape(method.as_str()))
                .transpose()?,
            payload: invoker.payload().map(|payload| payload.value()),
            callback: Identifier::parse("callbackToken")?,
            context: Identifier::parse("callbackContext")?,
        })
    }

    fn parameters(&self) -> Vec<Parameter> {
        [
            Parameter::new(self.callback.clone(), TypeName::long()),
            Parameter::new(self.context.clone(), TypeName::long()),
        ]
        .into_iter()
        .collect()
    }

    fn success_statement(&self, payload: Option<Expression>) -> Statement {
        self.call(&self.success, payload)
    }

    fn error_statement(&self, payload: Expression) -> Result<Statement> {
        self.error
            .as_ref()
            .map(|method| self.call(method, Some(payload)))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "fallible async callback method has no JNI error completion invoker",
            })
    }

    fn failure_statement(&self) -> Statement {
        self.call(&self.failure, None)
    }

    fn call(&self, method: &Identifier, payload: Option<Expression>) -> Statement {
        let arguments = [
            Expression::identifier(self.callback.clone()),
            Expression::identifier(self.context.clone()),
        ]
        .into_iter()
        .chain(payload)
        .collect::<Vec<_>>();
        Statement::expression(NativeCall::new(method.clone(), arguments).expression())
    }

    fn invoker<'bridge>(
        completion: &CallbackCompletionArgument<'_>,
        bridge: &'bridge JniBridgeContract,
    ) -> Result<&'bridge CallbackCompletionInvoker> {
        bridge
            .callback_completions()
            .iter()
            .find(|invoker| invoker.payload() == completion.payload())
            .ok_or(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "async callback completion has no shared JNI invoker",
            })
    }
}

impl<'error> FallibleReturn<'error> {
    fn from_method(
        source_name: Name,
        channel: ErrorChannel<'error, Native, IntoRust>,
        method: &JniCallbackMethod,
    ) -> Result<Option<Self>> {
        match channel {
            ErrorChannel::None => Ok(None),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ty,
                codec,
                ..
            } => Ok(Some(Self {
                source_name,
                success_out: method.success_out(),
                error_ty: ty,
                error_codec: codec,
            })),
            ErrorChannel::Encoded { .. } | ErrorChannel::Status => {
                Err(KotlinHost::unsupported("callback method error return"))
            }
            _ => Err(KotlinHost::unsupported(
                "unknown callback method error return",
            )),
        }
    }

    fn parameter(&self) -> Result<Option<Parameter>> {
        self.success_out
            .as_ref()
            .map(|success_out| {
                Ok(Parameter::new(
                    Identifier::escape(success_out.name().as_str())?,
                    KotlinType::jni(success_out.jni_type())?,
                ))
            })
            .transpose()
    }
}

impl HandleMethod {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    pub fn returns(&self) -> Option<&TypeName> {
        self.returns.as_ref()
    }

    pub fn setup(&self) -> &[Statement] {
        &self.setup
    }

    pub fn call(&self) -> &[Statement] {
        &self.call
    }

    pub fn cleanup(&self) -> &[Statement] {
        &self.cleanup
    }

    pub fn has_cleanup(&self) -> bool {
        !self.cleanup.is_empty()
    }
}

impl HandleMethod {
    fn from_declaration(
        source: &ImportedMethodDecl<Native, VTableSlot>,
        method: &JniCallbackHandleMethod,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let callable = source.callable();
        if !matches!(callable.error().channel(), ErrorChannel::None) {
            return Err(KotlinHost::unsupported("callback method error return"));
        }
        if !matches!(callable.execution(), ExecutionDecl::Synchronous(_)) {
            return Err(KotlinHost::unsupported("async callback method"));
        }
        if source.target().as_str() != method.slot().as_str() {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "callback declaration method does not match JNI handle method",
            });
        }
        let parameters = callable
            .params()
            .iter()
            .map(|parameter| HandleParameter::from_declaration(parameter, host, context))
            .collect::<Result<Vec<_>>>()?;
        let returned = callable
            .returns()
            .plan()
            .render_with(&mut HandleReturnRender { host, context })?;
        let native_arguments = std::iter::once(Expression::call(
            Expression::this(),
            Identifier::parse("requireOpen")?,
            ArgumentList::default(),
        ))
        .chain(
            parameters
                .iter()
                .flat_map(|parameter| parameter.native_arguments.iter().cloned()),
        )
        .collect::<Vec<_>>();
        let native_call = NativeCall::new(
            Identifier::escape(method.method().as_str())?,
            native_arguments,
        );
        Ok(Self {
            name: Name::new(source.name()).function()?,
            parameters: parameters
                .iter()
                .map(|parameter| parameter.public.clone())
                .collect(),
            returns: returned.ty.clone(),
            setup: parameters
                .iter()
                .flat_map(|parameter| parameter.setup.iter().cloned())
                .collect(),
            call: returned.statements(native_call.expression(), host, context)?,
            cleanup: parameters
                .into_iter()
                .flat_map(|parameter| parameter.cleanup)
                .collect(),
        })
    }
}

impl CallbackHandle {
    pub fn new(
        id: CallbackId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let callback = context.callback(id).ok_or(KotlinHost::unsupported(
            "callback handle without declaration",
        ))?;
        let ty = Name::new(callback.name()).type_name();
        Ok(Self {
            bridge: TypeName::new(format!("{ty}Bridge")),
            handle: TypeName::new(format!("{ty}Handle")),
            ty,
            presence,
        })
    }

    pub fn ty(&self) -> Result<TypeName> {
        match self.presence {
            HandlePresence::Required => Ok(self.ty.clone()),
            HandlePresence::Nullable => Ok(self.ty.clone().nullable()),
            _ => Err(KotlinHost::unsupported("unknown callback handle presence")),
        }
    }

    pub fn parameter_argument(&self, value: Expression) -> Result<Expression> {
        let create = Identifier::parse("create")?;
        let required = Expression::call(
            self.bridge.clone(),
            create.clone(),
            [value.clone()].into_iter().collect::<ArgumentList>(),
        );
        Ok(match self.presence {
            HandlePresence::Required => required,
            HandlePresence::Nullable => value.let_or_else(
                Identifier::parse("__boltffi_callback")?,
                Expression::call(
                    self.bridge.clone(),
                    create,
                    [Expression::identifier(Identifier::parse(
                        "__boltffi_callback",
                    )?)]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
                Expression::long(0),
            ),
            _ => {
                return Err(KotlinHost::unsupported("unknown callback handle presence"));
            }
        })
    }

    pub fn value_expression(&self, value: Expression) -> Result<Expression> {
        let handle = Expression::construct(
            self.handle.clone(),
            [value.clone()].into_iter().collect::<ArgumentList>(),
        );
        Ok(match self.presence {
            HandlePresence::Required => handle,
            HandlePresence::Nullable => Expression::conditional(
                value.equal(Expression::long(0)),
                Expression::null(),
                handle,
            ),
            _ => {
                return Err(KotlinHost::unsupported("unknown callback handle presence"));
            }
        })
    }

    pub fn value_statements(&self, call: Expression) -> Result<Vec<Statement>> {
        let result = Identifier::parse("__boltffi_result")?;
        let value = Expression::identifier(result.clone());
        let returned = self.value_expression(value)?;
        Ok(vec![
            Statement::value(result, call),
            Statement::expression(returned),
        ])
    }
}

impl<'bridge> CallbackRegistrationSet<'bridge> {
    fn new(registrations: &'bridge [CallbackRegistration]) -> Self {
        Self { registrations }
    }

    fn get(&self, decl: &CallbackDecl<Native>) -> Result<&'bridge CallbackRegistration> {
        self.registrations
            .iter()
            .find(|registration| {
                registration.register().as_str() == decl.protocol().register().name().as_str()
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "callback declaration has no JNI registration",
            })
    }
}

impl HandleParameter {
    fn from_declaration(
        parameter: &ParamDecl<Native, OutOfRust>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let OutgoingParam::Value(plan) = parameter.payload() else {
            return Err(KotlinHost::unsupported(
                "callback handle method closure parameter",
            ));
        };
        let source_name = Name::new(parameter.name());
        let name = source_name.parameter()?;
        plan.render_with(&mut HandleParameterRender {
            source_name,
            name,
            host,
            context,
        })
    }
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for HandleParameterRender<'_> {
    type Output = Result<HandleParameter>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match ty {
            DirectValueType::Primitive(primitive) => Ok(HandleParameter {
                public: Parameter::new(
                    self.name.clone(),
                    KotlinPrimitive::new(*primitive).api_type()?,
                ),
                native_arguments: vec![KotlinPrimitive::new(*primitive).native_argument(value)?],
                setup: Vec::new(),
                cleanup: Vec::new(),
            }),
            DirectValueType::Record(record) => {
                let ty = Record::type_name_from_id(*record, self.context)?;
                Ok(HandleParameter {
                    public: Parameter::new(self.name.clone(), ty),
                    native_arguments: vec![Record::direct_buffer_expression(value)?],
                    setup: Vec::new(),
                    cleanup: Vec::new(),
                })
            }
            DirectValueType::Enum(enumeration) => {
                let enumeration = Enumeration::from_id(*enumeration, self.host, self.context)?;
                Ok(HandleParameter {
                    public: Parameter::new(self.name.clone(), enumeration.name().clone()),
                    native_arguments: vec![
                        KotlinPrimitive::new(enumeration.repr()?).native_argument(
                            Expression::property(value, Identifier::parse("value")?),
                        )?,
                    ],
                    setup: Vec::new(),
                    cleanup: Vec::new(),
                })
            }
            _ => Err(KotlinHost::unsupported(
                "callback handle method direct parameter",
            )),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
        _receive: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let write = WireBuffer::new(&self.source_name)?.write_value(
            &codec.write_self_value(),
            Expression::identifier(self.name.clone()),
            self.host,
            self.context,
        )?;
        let (setup, native_arguments, cleanup) = write.into_direct_parts();
        Ok(HandleParameter {
            public: Parameter::new(self.name.clone(), KotlinType::type_ref(ty, self.context)?),
            native_arguments,
            setup,
            cleanup,
        })
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(*class, presence, self.context)?;
                Ok(HandleParameter {
                    public: Parameter::new(self.name.clone(), handle.ty()?),
                    native_arguments: vec![handle.parameter_argument(value)?],
                    setup: Vec::new(),
                    cleanup: Vec::new(),
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(HandleParameter {
                    public: Parameter::new(self.name.clone(), handle.ty()?),
                    native_arguments: vec![handle.parameter_argument(value)?],
                    setup: Vec::new(),
                    cleanup: Vec::new(),
                })
            }
            HandleTarget::Stream(_) => Err(KotlinHost::unsupported(
                "callback handle method stream parameter",
            )),
            _ => Err(KotlinHost::unsupported(
                "unknown callback handle method handle parameter",
            )),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let write = ScalarOption::new(primitive).write(&self.source_name)?;
        let (setup, native_arguments, cleanup) = write.into_direct_parts();
        Ok(HandleParameter {
            public: Parameter::new(self.name.clone(), ScalarOption::new(primitive).ty()?),
            native_arguments,
            setup,
            cleanup,
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType, _: ()) -> Self::Output {
        let vector = DirectVector::from_element(element, self.context)?;
        Ok(HandleParameter {
            public: Parameter::new(self.name.clone(), vector.ty().clone()),
            native_arguments: vec![
                vector.native_argument(Expression::identifier(self.name.clone()))?,
            ],
            setup: Vec::new(),
            cleanup: Vec::new(),
        })
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ReturnRender<'_> {
    type Output = Result<ReturnValue>;

    fn void(&mut self) -> Self::Output {
        Ok(ReturnValue {
            public_ty: None,
            jvm_ty: None,
            conversion: ReturnConversion::Void,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        self.require_supported_slot(slot, "callback method out-pointer return")?;
        match (slot, ty) {
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Primitive(primitive),
            ) => Ok(ReturnValue {
                public_ty: Some(KotlinPrimitive::new(*primitive).api_type()?),
                jvm_ty: Some(KotlinPrimitive::new(*primitive).native_type()?),
                conversion: ReturnConversion::Direct(ty.clone()),
            }),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Record(record),
            ) => {
                let ty = Record::type_name_from_id(*record, self.context)
                    .map(|name| self.host.generated_type(name))?;
                Ok(ReturnValue {
                    public_ty: Some(ty.clone()),
                    jvm_ty: Some(TypeName::byte_array(false)),
                    conversion: ReturnConversion::DirectRecord,
                })
            }
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Enum(enumeration),
            ) => Self::direct_enum(*enumeration, self.host, self.context),
            _ => Err(KotlinHost::unsupported("callback method direct return")),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        self.require_supported_slot(slot, "callback method out-pointer encoded return")?;
        Ok(ReturnValue {
            public_ty: Some(KotlinType::type_ref(ty, self.context)?),
            jvm_ty: Some(TypeName::byte_array(false)),
            conversion: ReturnConversion::Encoded {
                codec: codec.clone(),
                source_name: self.source_name.clone(),
            },
        })
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        self.require_supported_slot(slot, "callback method out-pointer handle return")?;
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(*class, presence, self.context)?;
                Ok(ReturnValue {
                    public_ty: Some(handle.ty()?),
                    jvm_ty: Some(TypeName::long()),
                    conversion: ReturnConversion::ClassHandle(handle),
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(ReturnValue {
                    public_ty: Some(handle.ty()?),
                    jvm_ty: Some(TypeName::long()),
                    conversion: ReturnConversion::CallbackHandle(handle),
                })
            }
            HandleTarget::Stream(_) => {
                Err(KotlinHost::unsupported("callback method stream return"))
            }
            _ => Err(KotlinHost::unsupported(
                "unknown callback method handle return",
            )),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(ReturnValue {
            public_ty: Some(ScalarOption::new(primitive).ty()?),
            jvm_ty: Some(TypeName::byte_array(false)),
            conversion: ReturnConversion::ScalarOption {
                primitive,
                source_name: self.source_name.clone(),
            },
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let vector = DirectVector::from_element(element, self.context)?;
        Ok(ReturnValue {
            public_ty: Some(vector.ty().clone()),
            jvm_ty: Some(TypeName::byte_array(false)),
            conversion: ReturnConversion::DirectVector(vector),
        })
    }

    fn closure(&mut self, _closure: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(KotlinHost::unsupported("callback method closure return"))
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for HandleReturnRender<'_> {
    type Output = Result<HandleReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(HandleReturn {
            ty: None,
            conversion: HandleReturnConversion::Void,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        if !matches!(slot, ReturnValueSlot::ReturnSlot) {
            return Err(KotlinHost::unsupported(
                "callback handle method out-pointer return",
            ));
        }
        match ty {
            DirectValueType::Primitive(primitive) => Ok(HandleReturn {
                ty: Some(KotlinPrimitive::new(*primitive).api_type()?),
                conversion: HandleReturnConversion::Direct(*primitive),
            }),
            DirectValueType::Record(record) => {
                let ty = Record::type_name_from_id(*record, self.context)?;
                Ok(HandleReturn {
                    ty: Some(ty.clone()),
                    conversion: HandleReturnConversion::DirectRecord(ty),
                })
            }
            DirectValueType::Enum(enumeration) => {
                let enumeration = Enumeration::from_id(*enumeration, self.host, self.context)?;
                let ty = enumeration.name().clone();
                Ok(HandleReturn {
                    ty: Some(ty.clone()),
                    conversion: HandleReturnConversion::DirectEnum(ty),
                })
            }
            _ => Err(KotlinHost::unsupported(
                "callback handle method direct return",
            )),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        if !matches!(slot, ReturnValueSlot::ReturnSlot) {
            return Err(KotlinHost::unsupported(
                "callback handle method out-pointer encoded return",
            ));
        }
        Ok(HandleReturn {
            ty: Some(KotlinType::type_ref(ty, self.context)?),
            conversion: HandleReturnConversion::Encoded(codec.read_plan()),
        })
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        if !matches!(slot, ReturnValueSlot::ReturnSlot) {
            return Err(KotlinHost::unsupported(
                "callback handle method out-pointer handle return",
            ));
        }
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(*class, presence, self.context)?;
                Ok(HandleReturn {
                    ty: Some(handle.ty()?),
                    conversion: HandleReturnConversion::ClassHandle(handle),
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(HandleReturn {
                    ty: Some(handle.ty()?),
                    conversion: HandleReturnConversion::CallbackHandle(handle),
                })
            }
            HandleTarget::Stream(_) => Err(KotlinHost::unsupported(
                "callback handle method stream return",
            )),
            _ => Err(KotlinHost::unsupported(
                "unknown callback handle method handle return",
            )),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(HandleReturn {
            ty: Some(ScalarOption::new(primitive).ty()?),
            conversion: HandleReturnConversion::ScalarOption(primitive),
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let vector = DirectVector::from_element(element, self.context)?;
        Ok(HandleReturn {
            ty: Some(vector.ty().clone()),
            conversion: HandleReturnConversion::DirectVector(vector),
        })
    }

    fn closure(&mut self, _closure: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(KotlinHost::unsupported(
            "callback handle method closure return",
        ))
    }
}

impl ReturnRender<'_> {
    fn require_supported_slot(&self, slot: ReturnValueSlot, shape: &'static str) -> Result<()> {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(()),
            ReturnValueSlot::OutPointer if self.fallible_success_out => Ok(()),
            ReturnValueSlot::OutPointer => Err(KotlinHost::unsupported(shape)),
            _ => Err(KotlinHost::unsupported(
                "unknown callback method return slot",
            )),
        }
    }

    fn direct_enum(
        enumeration: EnumId,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<ReturnValue> {
        let enumeration = Enumeration::from_id(enumeration, host, context)?;
        let repr = enumeration.repr()?;
        let ty = host.generated_type(enumeration.name().clone());
        Ok(ReturnValue {
            public_ty: Some(ty),
            jvm_ty: Some(KotlinPrimitive::new(repr).native_type()?),
            conversion: ReturnConversion::DirectEnum { repr },
        })
    }
}

impl ReturnValue {
    fn throwing_fallible_statements(
        &self,
        call: Expression,
        fallible: &FallibleReturn,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        let error = fallible.source_name.generated("error")?;
        Ok(vec![Statement::return_value(
            self.fallible_success_expression(call, fallible, host, context)?
                .try_catch(
                    error.clone(),
                    fallible.catch_type(host, context)?,
                    fallible.error_expression(
                        fallible.error_payload(Expression::identifier(error))?,
                        host,
                        context,
                    )?,
                ),
        )])
    }

    fn completion_success_statements(
        &self,
        call: Expression,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match completion.payload {
            None if matches!(&self.conversion, ReturnConversion::Void) => Ok(vec![
                Statement::expression(call),
                completion.success_statement(None),
            ]),
            None => Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "async callback return payload is missing",
            }),
            Some(CallbackCompletionPayloadValue::Bytes)
                if matches!(&self.conversion, ReturnConversion::Void) =>
            {
                Ok(vec![
                    Statement::expression(call),
                    completion.success_statement(Some(Self::empty_error_payload())),
                ])
            }
            Some(_) => {
                let result = Identifier::parse("__boltffi_result")?;
                let (setup, payload, cleanup) =
                    self.success_value(Expression::identifier(result.clone()), host, context)?;
                Ok(std::iter::once(Statement::value(result, call))
                    .chain(setup)
                    .chain(std::iter::once(completion.success_statement(Some(payload))))
                    .chain(cleanup)
                    .collect())
            }
        }
    }

    fn throwing_completion_fallible_statements(
        &self,
        call: Expression,
        fallible: &FallibleReturn,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        let error = fallible.source_name.generated("error")?;
        Ok(vec![Statement::expression(
            self.fallible_completion_success_expression(call, fallible, completion, host, context)?
                .try_catch(
                    error.clone(),
                    fallible.catch_type(host, context)?,
                    fallible.completion_error_expression(
                        fallible.error_payload(Expression::identifier(error))?,
                        completion,
                        host,
                        context,
                    )?,
                ),
        )])
    }

    fn fallible_completion_success_expression(
        &self,
        value: Expression,
        fallible: &FallibleReturn,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let statements = match &self.conversion {
            ReturnConversion::Void => {
                self.fallible_completion_success_value(value, fallible, completion, host, context)?
            }
            _ => {
                let result = Identifier::parse("__boltffi_result")?;
                std::iter::once(Statement::value(result.clone(), value))
                    .chain(self.fallible_completion_success_value(
                        Expression::identifier(result),
                        fallible,
                        completion,
                        host,
                        context,
                    )?)
                    .collect()
            }
        };
        Ok(Expression::run(
            statements,
            Expression::identifier(Identifier::parse("Unit")?),
        ))
    }

    fn statements(
        &self,
        call: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match &self.conversion {
            ReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            ReturnConversion::Direct(DirectValueType::Primitive(primitive)) => {
                KotlinPrimitive::new(*primitive)
                    .native_argument(call)
                    .map(Statement::return_value)
                    .map(|statement| vec![statement])
            }
            ReturnConversion::DirectEnum { repr, .. } => KotlinPrimitive::new(*repr)
                .native_argument(Expression::property(call, Identifier::parse("value")?))
                .map(Statement::return_value)
                .map(|statement| vec![statement]),
            ReturnConversion::DirectRecord => Ok(vec![Statement::return_value(
                Record::encode_expression(call)?,
            )]),
            ReturnConversion::Encoded { codec, source_name } => {
                let result = source_name.generated("result")?;
                let bytes = source_name.generated("bytes")?;
                let write = WireBuffer::new(source_name)?.write_value(
                    codec,
                    Expression::identifier(result.clone()),
                    host,
                    context,
                )?;
                let (setup, expression, cleanup) = write.into_array_parts();
                Ok(std::iter::once(Statement::value(result, call))
                    .chain(setup)
                    .chain(std::iter::once(Statement::value(bytes.clone(), expression)))
                    .chain(cleanup)
                    .chain(std::iter::once(Statement::return_value(
                        Expression::identifier(bytes),
                    )))
                    .collect())
            }
            ReturnConversion::ScalarOption {
                primitive,
                source_name,
            } => {
                let result = source_name.generated("result")?;
                let bytes = source_name.generated("bytes")?;
                let write = ScalarOption::new(*primitive)
                    .write_value(source_name, Expression::identifier(result.clone()))?;
                let (setup, expression, cleanup) = write.into_array_parts();
                Ok(std::iter::once(Statement::value(result, call))
                    .chain(setup)
                    .chain(std::iter::once(Statement::value(bytes.clone(), expression)))
                    .chain(cleanup)
                    .chain(std::iter::once(Statement::return_value(
                        Expression::identifier(bytes),
                    )))
                    .collect())
            }
            ReturnConversion::DirectVector(vector) => Ok(vec![Statement::return_value(
                vector.byte_array_expression(call)?,
            )]),
            ReturnConversion::ClassHandle(handle) => handle
                .parameter_argument(call)
                .map(Statement::return_value)
                .map(|statement| vec![statement]),
            ReturnConversion::CallbackHandle(handle) => handle
                .parameter_argument(call)
                .map(Statement::return_value)
                .map(|statement| vec![statement]),
            ReturnConversion::Direct(_) => {
                Err(KotlinHost::unsupported("callback method direct return"))
            }
        }
    }

    fn fallible_success_expression(
        &self,
        value: Expression,
        fallible: &FallibleReturn,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let ReturnConversion::Void = &self.conversion else {
            let success_out = fallible
                .success_out
                .as_ref()
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "jni",
                    invariant: "fallible callback method has no success out-pointer",
                })?;
            let result = fallible.source_name.generated("result")?;
            let (setup, payload, cleanup) =
                self.success_value(Expression::identifier(result.clone()), host, context)?;
            let write = Statement::expression(Expression::call(
                "Native",
                Identifier::escape(success_out.writer().as_str())?,
                [
                    Expression::identifier(Identifier::escape(success_out.name().as_str())?),
                    payload,
                ]
                .into_iter()
                .collect::<ArgumentList>(),
            ));
            return Ok(Expression::run(
                std::iter::once(Statement::value(result, value))
                    .chain(setup)
                    .chain(std::iter::once(write))
                    .chain(cleanup)
                    .collect(),
                Self::empty_error_payload(),
            ));
        };
        Ok(Expression::run(
            vec![Statement::expression(value)],
            Self::empty_error_payload(),
        ))
    }

    fn completion_success_value(
        &self,
        value: Expression,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match completion.payload {
            None if matches!(&self.conversion, ReturnConversion::Void) => Ok(vec![
                Statement::expression(value),
                completion.success_statement(None),
            ]),
            None => Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "async callback return payload is missing",
            }),
            Some(CallbackCompletionPayloadValue::Bytes)
                if matches!(&self.conversion, ReturnConversion::Void) =>
            {
                Ok(vec![
                    Statement::expression(value),
                    completion.success_statement(Some(Self::empty_error_payload())),
                ])
            }
            Some(_) => {
                let (setup, payload, cleanup) = self.success_value(value, host, context)?;
                Ok(setup
                    .into_iter()
                    .chain(std::iter::once(completion.success_statement(Some(payload))))
                    .chain(cleanup)
                    .collect())
            }
        }
    }

    fn fallible_completion_success_value(
        &self,
        value: Expression,
        fallible: &FallibleReturn,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match completion.payload {
            Some(CallbackCompletionPayloadValue::Bytes) => {
                let (setup, payload, cleanup) =
                    self.byte_success_value(value, fallible, host, context)?;
                Ok(setup
                    .into_iter()
                    .chain(std::iter::once(completion.success_statement(Some(payload))))
                    .chain(cleanup)
                    .collect())
            }
            _ => self.completion_success_value(value, completion, host, context),
        }
    }

    fn byte_success_value(
        &self,
        value: Expression,
        fallible: &FallibleReturn,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<(Vec<Statement>, Expression, Vec<Statement>)> {
        match &self.conversion {
            ReturnConversion::Void => Ok((
                vec![Statement::expression(value)],
                Self::empty_error_payload(),
                Vec::new(),
            )),
            ReturnConversion::Direct(DirectValueType::Primitive(primitive)) => {
                Self::primitive_bytes(&fallible.source_name, *primitive, value)
            }
            ReturnConversion::DirectEnum { repr } => Self::native_primitive_bytes(
                &fallible.source_name,
                *repr,
                Expression::property(value, Identifier::parse("value")?),
            ),
            ReturnConversion::DirectRecord => {
                Ok((Vec::new(), Record::encode_expression(value)?, Vec::new()))
            }
            _ => self.success_value(value, host, context),
        }
    }

    fn success_value(
        &self,
        value: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<(Vec<Statement>, Expression, Vec<Statement>)> {
        match &self.conversion {
            ReturnConversion::Direct(DirectValueType::Primitive(primitive)) => Ok((
                Vec::new(),
                KotlinPrimitive::new(*primitive).native_argument(value)?,
                Vec::new(),
            )),
            ReturnConversion::DirectEnum { repr } => Ok((
                Vec::new(),
                KotlinPrimitive::new(*repr)
                    .native_argument(Expression::property(value, Identifier::parse("value")?))?,
                Vec::new(),
            )),
            ReturnConversion::DirectRecord => {
                Ok((Vec::new(), Record::encode_expression(value)?, Vec::new()))
            }
            ReturnConversion::Encoded { codec, source_name } => Ok(WireBuffer::new(source_name)?
                .write_value(codec, value, host, context)?
                .into_array_parts()),
            ReturnConversion::ScalarOption {
                primitive,
                source_name,
            } => Ok(ScalarOption::new(*primitive)
                .write_value(source_name, value)?
                .into_array_parts()),
            ReturnConversion::DirectVector(vector) => {
                Ok((Vec::new(), vector.byte_array_expression(value)?, Vec::new()))
            }
            ReturnConversion::ClassHandle(handle) => {
                Ok((Vec::new(), handle.parameter_argument(value)?, Vec::new()))
            }
            ReturnConversion::CallbackHandle(handle) => {
                Ok((Vec::new(), handle.parameter_argument(value)?, Vec::new()))
            }
            ReturnConversion::Void | ReturnConversion::Direct(_) => Err(KotlinHost::unsupported(
                "fallible callback method success return",
            )),
        }
    }

    fn empty_error_payload() -> Expression {
        Expression::construct(
            TypeName::byte_array(false),
            [Expression::integer(0)]
                .into_iter()
                .collect::<ArgumentList>(),
        )
    }

    fn primitive_bytes(
        source_name: &Name,
        primitive: Primitive,
        value: Expression,
    ) -> Result<(Vec<Statement>, Expression, Vec<Statement>)> {
        Self::primitive_bytes_with_method(
            source_name,
            primitive,
            value,
            KotlinPrimitive::new(primitive).wire_method_suffix()?,
        )
    }

    fn native_primitive_bytes(
        source_name: &Name,
        primitive: Primitive,
        value: Expression,
    ) -> Result<(Vec<Statement>, Expression, Vec<Statement>)> {
        Self::primitive_bytes_with_method(
            source_name,
            primitive,
            value,
            KotlinPrimitive::new(primitive).native_wire_method_suffix()?,
        )
    }

    fn primitive_bytes_with_method(
        source_name: &Name,
        primitive: Primitive,
        value: Expression,
        suffix: &str,
    ) -> Result<(Vec<Statement>, Expression, Vec<Statement>)> {
        let buffer = WireBuffer::new(source_name)?;
        let writer = buffer.writer().clone();
        let encoded = buffer
            .write_statements(
                Expression::integer(i128::from(KotlinPrimitive::new(primitive).wire_size()?)),
                vec![Statement::expression(Expression::call(
                    Expression::identifier(writer),
                    Identifier::parse(format!("write{suffix}"))?,
                    [value].into_iter().collect::<ArgumentList>(),
                ))],
            )?
            .into_array_parts();
        Ok(encoded)
    }
}

impl FallibleReturn<'_> {
    fn catch_type(&self, host: &KotlinHost, context: &RenderContext<Native>) -> Result<TypeName> {
        match self.error_ty {
            TypeRef::Record(record) => Record::type_name_from_id(*record, context)
                .map(|name| self.generated_error_type(name, host)),
            TypeRef::Enum(enumeration) => Enumeration::type_name_from_id(*enumeration, context)
                .map(|name| self.generated_error_type(name, host)),
            TypeRef::String => Ok(TypeName::new("Throwable")),
            _ => Err(KotlinHost::unsupported("callback method error return")),
        }
    }

    fn error_payload(&self, value: Expression) -> Result<Expression> {
        match self.error_ty {
            TypeRef::Record(_) | TypeRef::Enum(_) => Ok(value),
            TypeRef::String => Ok(Expression::property(
                value.clone(),
                Identifier::parse("message")?,
            )
            .or_else(Expression::call(
                value,
                Identifier::parse("toString")?,
                ArgumentList::default(),
            ))),
            _ => Err(KotlinHost::unsupported("callback method error return")),
        }
    }

    fn generated_error_type(&self, name: TypeName, host: &KotlinHost) -> TypeName {
        match host.api_layout() {
            KotlinApiStyle::TopLevel => name,
            KotlinApiStyle::ModuleObject => TypeName::qualified(host.file(), name),
        }
    }

    fn error_expression(
        &self,
        value: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let bytes = self.source_name.generated("error_bytes")?;
        let write = WireBuffer::new(&self.source_name)?.write_value(
            self.error_codec,
            value,
            host,
            context,
        )?;
        let (setup, expression, cleanup) = write.into_array_parts();
        Ok(Expression::run(
            setup
                .into_iter()
                .chain(std::iter::once(Statement::value(bytes.clone(), expression)))
                .chain(cleanup)
                .collect(),
            Expression::identifier(bytes),
        ))
    }

    fn completion_error_expression(
        &self,
        value: Expression,
        completion: &AsyncCompletion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let bytes = self.source_name.generated("error_bytes")?;
        let write = WireBuffer::new(&self.source_name)?.write_value(
            self.error_codec,
            value,
            host,
            context,
        )?;
        let (setup, expression, cleanup) = write.into_array_parts();
        Ok(Expression::run(
            setup
                .into_iter()
                .chain(std::iter::once(Statement::value(bytes.clone(), expression)))
                .chain(std::iter::once(
                    completion.error_statement(Expression::identifier(bytes))?,
                ))
                .chain(cleanup)
                .collect(),
            Expression::identifier(Identifier::parse("Unit")?),
        ))
    }
}

impl HandleReturn {
    fn statements(
        &self,
        call: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        self.value_statements(call, host, context).map(|body| {
            if self.ty.is_some() {
                Statement::with_return_value(body)
            } else {
                body
            }
        })
    }

    fn value_statements(
        &self,
        call: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match &self.conversion {
            HandleReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            HandleReturnConversion::Direct(primitive) => Ok(vec![
                KotlinPrimitive::new(*primitive)
                    .public_return(call)
                    .map(Statement::expression)?,
            ]),
            HandleReturnConversion::DirectRecord(record) => {
                let result = Identifier::parse("__boltffi_result")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                Ok(vec![
                    Statement::value(result.clone(), payload),
                    Statement::expression(Record::decode_expression(
                        record.clone(),
                        Expression::identifier(result),
                    )?),
                ])
            }
            HandleReturnConversion::DirectEnum(ty) => {
                Ok(vec![Statement::expression(Expression::call(
                    ty.clone(),
                    Identifier::parse("fromValue")?,
                    [call].into_iter().collect::<ArgumentList>(),
                ))])
            }
            HandleReturnConversion::DirectVector(vector) => vector.value_statements(call),
            HandleReturnConversion::Encoded(codec) => {
                let result = Identifier::parse("__boltffi_result")?;
                let reader = Identifier::parse("__boltffi_reader")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                let value = codec
                    .render_with(&mut Reader::new(reader.clone(), host, context))?
                    .into_expression();
                Ok(vec![
                    Statement::value(result.clone(), payload),
                    Statement::value(
                        reader,
                        Expression::construct(
                            TypeName::new("WireReader"),
                            [Expression::identifier(result)]
                                .into_iter()
                                .collect::<ArgumentList>(),
                        ),
                    ),
                    Statement::expression(value),
                ])
            }
            HandleReturnConversion::ClassHandle(handle) => handle.value_statements(call),
            HandleReturnConversion::CallbackHandle(handle) => handle.value_statements(call),
            HandleReturnConversion::ScalarOption(primitive) => {
                ScalarOption::new(*primitive).read_value(call)
            }
        }
    }
}
