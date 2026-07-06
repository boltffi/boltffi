use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackId, ClassId, ClosureReturn, DirectValueType, DirectVectorElementType, Direction,
    EnumId, ErrorChannel, ErrorPlacement, ExecutionDecl, ExportedCallable, FunctionDecl,
    HandlePresence, HandleTarget, IncomingParam, IntoRust, Native, NativeSymbol, OutOfRust,
    ParamDecl, ParamPlan, ParamPlanRender, Primitive, Receive, RecordId, ReturnPlanRender,
    ReturnValueSlot, Surface, TypeRef, native,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::{EncodedWrite, MutableParameter, Reader, ScalarOption, WireBuffer},
        name_style::{KotlinPackage, Name},
        primitive::KotlinPrimitive,
        render::{
            callback::CallbackHandle,
            class::ClassHandle,
            closure::Closure,
            direct_vector::DirectVector,
            enumeration::Enumeration,
            native::NativeCall,
            record::Record,
            signature,
            type_name::{KotlinType, ParameterType},
        },
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/function.kt", escape = "none")]
struct FunctionTemplate {
    function: Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    name: Identifier,
    parameters: Vec<ExportedParameter>,
    returns: Option<TypeName>,
    setup: Vec<Statement>,
    call: Vec<Statement>,
    cleanup: Vec<Statement>,
    async_call: Option<AsyncCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedCall {
    name: Identifier,
    parameters: Vec<ExportedParameter>,
    returns: Option<TypeName>,
    setup: Vec<Statement>,
    call: Vec<Statement>,
    cleanup: Vec<Statement>,
    async_call: Option<AsyncCall>,
}

pub struct ExportedCallRenderer<'render> {
    host: &'render KotlinHost,
    bridge: &'render JniBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedParameter {
    signature: signature::Parameter,
    native_argument: Expression,
    mutation: Option<ParameterMutation>,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

struct NativeArgument {
    expression: Expression,
    mutation: Option<ParameterMutation>,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParameterMutation {
    destination: Identifier,
    reader: Identifier,
    result: Identifier,
    read: <IntoRust as Direction>::Codec,
}

struct FunctionReturn {
    ty: Option<TypeName>,
    conversion: ReturnConversion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedReceiverMutation {
    ty: TypeName,
    package: Option<KotlinPackage>,
}

enum ReceiverBinding {
    None,
    Package(KotlinPackage),
    EncodedMutation(EncodedReceiverMutation),
}

impl EncodedReceiverMutation {
    pub fn new(ty: TypeName) -> Self {
        Self { ty, package: None }
    }

    pub fn with_package(mut self, package: &KotlinPackage) -> Self {
        self.package = Some(package.clone());
        self
    }

    fn package(&self) -> Option<&KotlinPackage> {
        self.package.as_ref()
    }
}

impl ReceiverBinding {
    fn package(&self) -> Option<&KotlinPackage> {
        match self {
            Self::None => None,
            Self::Package(package) => Some(package),
            Self::EncodedMutation(receiver) => receiver.package(),
        }
    }

    fn into_mutation(self) -> Option<EncodedReceiverMutation> {
        match self {
            Self::EncodedMutation(receiver) => Some(receiver),
            Self::None | Self::Package(_) => None,
        }
    }
}

enum ErrorConversion {
    None,
    Status,
    Encoded {
        ty: TypeRef,
        codec: <OutOfRust as Direction>::Codec,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncCall {
    create_setup: Vec<Statement>,
    create: Expression,
    create_cleanup: Vec<Statement>,
    poll: Identifier,
    complete_body: Vec<Statement>,
    free: Identifier,
    cancel: Identifier,
    returns_value: bool,
}

struct AsyncStart {
    call: Expression,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

struct AsyncProtocolFunctions {
    poll: Identifier,
    complete: Identifier,
    cancel: Identifier,
    free: Identifier,
}

enum ReturnConversion {
    Void,
    Direct(Primitive),
    ByteArrayValue(TypeName),
    DirectEnum(TypeName),
    DirectVector(DirectVector),
    Encoded {
        codec: <OutOfRust as Direction>::Codec,
        package: Option<KotlinPackage>,
    },
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
    ScalarOption(Primitive),
    ParameterMutation(ParameterMutation),
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        ExportedCallRenderer::new(host, bridge, context)
            .exported(
                Name::new(decl.name()).function()?,
                decl.symbol(),
                decl.callable(),
                Vec::new(),
            )
            .map(Self::from_call)
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            FunctionTemplate { function: self }.render()?,
        ))
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[ExportedParameter] {
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

    pub fn async_call(&self) -> Option<&AsyncCall> {
        self.async_call.as_ref()
    }

    fn from_call(call: ExportedCall) -> Self {
        Self {
            name: call.name,
            parameters: call.parameters,
            returns: call.returns,
            setup: call.setup,
            call: call.call,
            cleanup: call.cleanup,
            async_call: call.async_call,
        }
    }
}

impl<'render> ExportedCallRenderer<'render> {
    pub fn new(
        host: &'render KotlinHost,
        bridge: &'render JniBridgeContract,
        context: &'render RenderContext<'render, Native>,
    ) -> Self {
        Self {
            host,
            bridge,
            context,
        }
    }

    pub fn exported(
        &self,
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
    ) -> Result<ExportedCall> {
        self.build(name, symbol, callable, native_prefix, ReceiverBinding::None)
    }

    pub fn with_package(
        &self,
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        package: &KotlinPackage,
    ) -> Result<ExportedCall> {
        self.build(
            name,
            symbol,
            callable,
            native_prefix,
            ReceiverBinding::Package(package.clone()),
        )
    }

    pub fn with_encoded_receiver_mutation(
        &self,
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        receiver_mutation: EncodedReceiverMutation,
    ) -> Result<ExportedCall> {
        self.build(
            name,
            symbol,
            callable,
            native_prefix,
            ReceiverBinding::EncodedMutation(receiver_mutation),
        )
    }

    fn build(
        &self,
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        receiver: ReceiverBinding,
    ) -> Result<ExportedCall> {
        let package = receiver.package();
        let parameters = callable
            .params()
            .iter()
            .map(|parameter| {
                ExportedParameter::from_declaration(
                    parameter,
                    package,
                    self.host,
                    self.bridge,
                    self.context,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut function_return =
            callable
                .returns()
                .plan()
                .render_with(&mut FunctionReturnPlan::new(
                    self.context,
                    package,
                    callable,
                ))?;
        if let Some(receiver_mutation) = receiver.into_mutation() {
            function_return =
                function_return.with_byte_array_receiver_writeback(receiver_mutation.ty)?;
        }
        if let Some(mutation) = ExportedCall::parameter_mutation(&parameters)? {
            function_return = function_return.with_parameter_mutation(mutation)?;
        }
        let native_arguments = native_prefix
            .into_iter()
            .chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.native_argument().clone()),
            )
            .collect::<Vec<_>>();
        let native_call = NativeCall::new(
            Identifier::escape(symbol.name().as_str())?,
            native_arguments,
        );
        let error_conversion = ErrorConversion::from_channel(callable.error().channel())?;
        let setup = parameters
            .iter()
            .flat_map(|parameter| parameter.setup().iter().cloned())
            .collect::<Vec<_>>();
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup().iter().cloned())
            .collect::<Vec<_>>();
        let returns = function_return.ty.clone();
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => Ok(ExportedCall {
                name,
                parameters,
                returns,
                setup,
                call: function_return.return_statements(
                    error_conversion.wrap(native_call.expression(), self.host, self.context)?,
                    self.host,
                    self.context,
                )?,
                cleanup,
                async_call: None,
            }),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                ..
            }) => Ok(ExportedCall {
                name,
                parameters,
                returns,
                setup: Vec::new(),
                call: Vec::new(),
                cleanup: Vec::new(),
                async_call: Some(AsyncCall::new(
                    AsyncStart::new(native_call.expression(), setup, cleanup),
                    AsyncProtocolFunctions::new(poll, complete, cancel, free)?,
                    function_return,
                    error_conversion,
                    self.host,
                    self.context,
                )?),
            }),
            ExecutionDecl::Asynchronous(_) => Err(KotlinHost::unsupported(
                "unsupported async function protocol",
            )),
            _ => Err(KotlinHost::unsupported("unknown function execution")),
        }
    }
}

impl ExportedCall {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[ExportedParameter] {
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

    pub fn async_call(&self) -> Option<&AsyncCall> {
        self.async_call.as_ref()
    }

    fn parameter_mutation(parameters: &[ExportedParameter]) -> Result<Option<ParameterMutation>> {
        let mut mutations = parameters
            .iter()
            .filter_map(ExportedParameter::mutation)
            .cloned();
        let mutation = mutations.next();
        if mutations.next().is_some() {
            return Err(KotlinHost::unsupported(
                "multiple mutable encoded parameters",
            ));
        }
        Ok(mutation)
    }
}

impl ExportedParameter {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        package: Option<&KotlinPackage>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source_name = Name::new(parameter.name());
        let name = source_name.parameter()?;
        let (ty, native_argument) = match parameter.payload() {
            IncomingParam::Value(plan) => (
                Self::type_name(plan, package, context)?,
                Self::native_argument_for(source_name, name.clone(), plan, host, context)?,
            ),
            IncomingParam::Closure(closure) => (
                Closure::type_name(closure, host, context)?,
                NativeArgument::direct(Closure::native_argument(
                    closure,
                    Expression::identifier(name.clone()),
                    bridge,
                )?),
            ),
        };
        Ok(Self {
            native_argument: native_argument.expression,
            mutation: native_argument.mutation,
            signature: signature::Parameter::new(name, ty),
            setup: native_argument.setup,
            cleanup: native_argument.cleanup,
        })
    }

    pub fn name(&self) -> &Identifier {
        self.signature.name()
    }

    pub fn ty(&self) -> &TypeName {
        self.signature.ty()
    }

    fn native_argument(&self) -> &Expression {
        &self.native_argument
    }

    fn setup(&self) -> &[Statement] {
        &self.setup
    }

    fn cleanup(&self) -> &[Statement] {
        &self.cleanup
    }

    fn mutation(&self) -> Option<&ParameterMutation> {
        self.mutation.as_ref()
    }

    fn type_name(
        plan: &ParamPlan<Native, IntoRust>,
        package: Option<&KotlinPackage>,
        context: &RenderContext<Native>,
    ) -> Result<TypeName> {
        plan.render_with(&mut ParameterType::new(context).package(package))
    }
}

impl ExportedParameter {
    fn native_argument_for(
        source_name: Name,
        name: Identifier,
        plan: &ParamPlan<Native, IntoRust>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<NativeArgument> {
        plan.render_with(&mut NativeArgumentRender {
            source_name,
            name,
            host,
            context,
        })
    }
}

struct NativeArgumentRender<'context> {
    source_name: Name,
    name: Identifier,
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
}

impl NativeArgument {
    fn direct(expression: Expression) -> Self {
        Self {
            expression,
            mutation: None,
            setup: Vec::new(),
            cleanup: Vec::new(),
        }
    }

    fn encoded(write: EncodedWrite, mutation: Option<ParameterMutation>) -> Self {
        let (setup, expression, cleanup) = write.into_parts();
        Self {
            expression,
            mutation,
            setup,
            cleanup,
        }
    }
}

impl ParameterMutation {
    fn from_encoded(
        source_name: &Name,
        destination: Identifier,
        codec: &<IntoRust as Direction>::Codec,
        shape: native::BufferShape,
        receive: Receive,
    ) -> Result<Option<Self>> {
        if receive != Receive::ByMutRef {
            return Ok(None);
        }
        if shape != native::BufferShape::Slice {
            return Err(KotlinHost::unsupported("mutable encoded parameter"));
        }
        MutableParameter::validate(&codec.read_plan())?;
        Self::new(source_name, destination, codec).map(Some)
    }

    fn new(
        source_name: &Name,
        destination: Identifier,
        codec: &<IntoRust as Direction>::Codec,
    ) -> Result<Self> {
        Ok(Self {
            destination,
            reader: source_name.generated("mutation_reader")?,
            result: source_name.generated("mutation")?,
            read: codec.clone(),
        })
    }

    fn statements(
        &self,
        call: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
            "null mutation buffer returned",
        )));
        let mut reader = Reader::new(self.reader.clone(), host, context);
        let decoded = self
            .read
            .read_plan()
            .render_with(&mut reader)?
            .into_expression();
        Ok(vec![
            Statement::value(self.result.clone(), payload),
            Statement::value(
                self.reader.clone(),
                Expression::construct(
                    TypeName::new("WireReader"),
                    [Expression::identifier(self.result.clone())]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            ),
            Statement::expression(Expression::call(
                decoded,
                Identifier::parse("copyInto")?,
                [Expression::identifier(self.destination.clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )),
        ])
    }
}

impl AsyncStart {
    fn new(call: Expression, setup: Vec<Statement>, cleanup: Vec<Statement>) -> Self {
        Self {
            call,
            setup,
            cleanup,
        }
    }
}

impl AsyncCall {
    fn new(
        start: AsyncStart,
        functions: AsyncProtocolFunctions,
        returns: FunctionReturn,
        error_conversion: ErrorConversion,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let future = Expression::identifier(Identifier::parse("future")?);
        let complete_call = error_conversion.wrap(
            NativeCall::new(functions.complete.clone(), vec![future]).expression(),
            host,
            context,
        )?;
        Ok(Self {
            create_setup: start.setup,
            create: start.call,
            create_cleanup: start.cleanup,
            poll: functions.poll,
            complete_body: returns.value_statements(complete_call, host, context)?,
            free: functions.free,
            cancel: functions.cancel,
            returns_value: returns.ty.is_some(),
        })
    }

    pub fn create_setup(&self) -> &[Statement] {
        &self.create_setup
    }

    pub fn create(&self) -> &Expression {
        &self.create
    }

    pub fn create_cleanup(&self) -> &[Statement] {
        &self.create_cleanup
    }

    pub fn has_create_cleanup(&self) -> bool {
        !self.create_cleanup.is_empty()
    }

    pub fn poll(&self) -> &Identifier {
        &self.poll
    }

    pub fn complete_body(&self) -> &[Statement] {
        &self.complete_body
    }

    pub fn free(&self) -> &Identifier {
        &self.free
    }

    pub fn cancel(&self) -> &Identifier {
        &self.cancel
    }

    pub fn returns_value(&self) -> bool {
        self.returns_value
    }
}

impl ErrorConversion {
    fn from_channel(channel: ErrorChannel<'_, Native, OutOfRust>) -> Result<Self> {
        match channel {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Status => Ok(Self::Status),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ty,
                codec,
                ..
            } => Ok(Self::Encoded {
                ty: ty.clone(),
                codec: codec.clone(),
            }),
            ErrorChannel::Encoded { .. } => {
                Err(KotlinHost::unsupported("encoded error out-pointer"))
            }
            _ => Err(KotlinHost::unsupported("unknown function error channel")),
        }
    }

    fn wrap(
        &self,
        call: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        match self {
            Self::None => Ok(call),
            Self::Status => self.status(call),
            Self::Encoded { ty, codec } => self.encoded(call, ty, codec, host, context),
        }
    }

    fn status(&self, call: Expression) -> Result<Expression> {
        let error = Identifier::parse("__boltffi_error")?;
        let message = Expression::property(
            Expression::identifier(error.clone()),
            Identifier::parse("message")?,
        )
        .or_else(Expression::literal(Literal::string("BoltFFI call failed")));
        Ok(call.try_catch(
            error,
            TypeName::new("RuntimeException"),
            Expression::throwing(Expression::construct(
                TypeName::new("FfiException"),
                [message].into_iter().collect::<ArgumentList>(),
            )),
        ))
    }

    fn encoded(
        &self,
        call: Expression,
        ty: &TypeRef,
        codec: &<OutOfRust as Direction>::Codec,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let error = Identifier::parse("__boltffi_error")?;
        let reader = Identifier::parse("__boltffi_error_reader")?;
        let mut codec_reader = Reader::new(reader.clone(), host, context);
        let decoded = codec.render_with(&mut codec_reader)?.into_expression();
        let thrown = match ty {
            TypeRef::String => Expression::construct(
                TypeName::new("FfiException"),
                [decoded].into_iter().collect::<ArgumentList>(),
            ),
            TypeRef::Record(_) | TypeRef::Enum(_) => decoded,
            _ => {
                return Err(KotlinHost::unsupported("kotlin throwable error type"));
            }
        };
        Ok(call.try_catch(
            error.clone(),
            TypeName::new("BoltFfiErrorBufferException"),
            Expression::run(
                vec![Statement::value(
                    reader,
                    Expression::construct(
                        TypeName::new("WireReader"),
                        [Expression::property(
                            Expression::identifier(error),
                            Identifier::parse("bytes")?,
                        )]
                        .into_iter()
                        .collect::<ArgumentList>(),
                    ),
                )],
                Expression::throwing(thrown),
            ),
        ))
    }
}

impl AsyncProtocolFunctions {
    fn new(
        poll: &NativeSymbol,
        complete: &NativeSymbol,
        cancel: &NativeSymbol,
        free: &NativeSymbol,
    ) -> Result<Self> {
        Ok(Self {
            poll: Identifier::escape(poll.name().as_str())?,
            complete: Identifier::escape(complete.name().as_str())?,
            cancel: Identifier::escape(cancel.name().as_str())?,
            free: Identifier::escape(free.name().as_str())?,
        })
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for NativeArgumentRender<'_> {
    type Output = Result<NativeArgument>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match ty {
            DirectValueType::Primitive(primitive) => KotlinPrimitive::new(*primitive)
                .native_argument(value)
                .map(NativeArgument::direct),
            DirectValueType::Record(_) => {
                Record::encode_expression(value).map(NativeArgument::direct)
            }
            DirectValueType::Enum(enumeration) => {
                Enumeration::native_argument(*enumeration, value, self.context)
                    .map(NativeArgument::direct)
            }
            _ => Err(KotlinHost::unsupported("unknown direct function parameter")),
        }
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
        receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        let mutation = ParameterMutation::from_encoded(
            &self.source_name,
            self.name.clone(),
            codec,
            shape,
            receive,
        )?;
        WireBuffer::new(&self.source_name)
            .and_then(|buffer| buffer.write(codec, self.host, self.context))
            .map(|write| NativeArgument::encoded(write, mutation))
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => ClassHandle::new(*class, presence, self.context)
                .and_then(|handle| {
                    handle
                        .parameter_argument(Expression::identifier(self.name.clone()))
                        .map(NativeArgument::direct)
                }),
            HandleTarget::Callback(callback) => {
                CallbackHandle::new(*callback, presence, self.context).and_then(|handle| {
                    handle
                        .parameter_argument(Expression::identifier(self.name.clone()))
                        .map(NativeArgument::direct)
                })
            }
            HandleTarget::Stream(_) => Err(KotlinHost::unsupported("handle function parameter")),
            _ => Err(KotlinHost::unsupported("unknown handle function parameter")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        ScalarOption::new(primitive)
            .write(&self.source_name)
            .map(|write| NativeArgument::encoded(write, None))
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        DirectVector::from_element(element, self.context).and_then(|vector| {
            vector
                .native_argument(Expression::identifier(self.name.clone()))
                .map(NativeArgument::direct)
        })
    }
}

struct FunctionReturnPlan<'context> {
    context: &'context RenderContext<'context, Native>,
    package: Option<KotlinPackage>,
    fallible_success_out: bool,
}

impl<'context> FunctionReturnPlan<'context> {
    fn new(
        context: &'context RenderContext<'context, Native>,
        package: Option<&KotlinPackage>,
        callable: &ExportedCallable<Native>,
    ) -> Self {
        let error_channel = callable.error().channel();
        Self {
            context,
            package: package.cloned(),
            fallible_success_out: matches!(
                error_channel,
                ErrorChannel::Status
                    | ErrorChannel::Encoded {
                        placement: ErrorPlacement::ReturnSlot,
                        ..
                    }
            ),
        }
    }

    fn require_supported_slot(&self, slot: ReturnValueSlot, shape: &'static str) -> Result<()> {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(()),
            ReturnValueSlot::OutPointer if self.fallible_success_out => Ok(()),
            ReturnValueSlot::OutPointer => Err(KotlinHost::unsupported(shape)),
            _ => Err(KotlinHost::unsupported("unknown function return slot")),
        }
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for FunctionReturnPlan<'_> {
    type Output = Result<FunctionReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(FunctionReturn::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        self.require_supported_slot(slot, "out-pointer function return")?;
        match (slot, ty) {
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Primitive(primitive),
            ) => FunctionReturn::direct(*primitive),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Record(record),
            ) => FunctionReturn::direct_record(*record, self.context, self.package.as_ref()),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Enum(enumeration),
            ) => FunctionReturn::direct_enum(*enumeration, self.context),
            _ => Err(KotlinHost::unsupported("unknown direct function return")),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        self.require_supported_slot(slot, "out-pointer encoded function return")?;
        match slot {
            ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer => {
                FunctionReturn::encoded(ty, codec.clone(), self.context, self.package.as_ref())
            }
            _ => Err(KotlinHost::unsupported("unknown encoded function return")),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        self.require_supported_slot(slot, "out-pointer handle function return")?;
        match (slot, target) {
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                HandleTarget::Class(class),
            ) => FunctionReturn::class_handle(*class, presence, self.context),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                HandleTarget::Callback(callback),
            ) => FunctionReturn::callback_handle(*callback, presence, self.context),
            (_, HandleTarget::Stream(_)) => Err(KotlinHost::unsupported("handle function return")),
            _ => Err(KotlinHost::unsupported("unknown handle function return")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        FunctionReturn::scalar_option(primitive)
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        FunctionReturn::direct_vector(element, self.context)
    }

    fn closure(&mut self, _closure: &'plan ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(KotlinHost::unsupported("closure function return"))
    }
}

impl FunctionReturn {
    fn void() -> Self {
        Self {
            ty: None,
            conversion: ReturnConversion::Void,
        }
    }

    fn direct(primitive: Primitive) -> Result<Self> {
        let ty = KotlinPrimitive::new(primitive).api_type()?;
        Ok(Self {
            ty: Some(ty),
            conversion: ReturnConversion::Direct(primitive),
        })
    }

    fn direct_record(
        record: RecordId,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        let ty = Record::type_name_from_id(record, context).map(|record| {
            package.map_or(record.clone(), |package| {
                TypeName::qualified(package, record)
            })
        })?;
        Ok(Self::byte_array_value(ty))
    }

    fn byte_array_value(ty: TypeName) -> Self {
        Self {
            ty: Some(ty.clone()),
            conversion: ReturnConversion::ByteArrayValue(ty),
        }
    }

    fn direct_enum(enumeration: EnumId, context: &RenderContext<Native>) -> Result<Self> {
        let ty = Enumeration::type_name_from_id(enumeration, context)?;
        Ok(Self {
            ty: Some(ty.clone()),
            conversion: ReturnConversion::DirectEnum(ty),
        })
    }

    fn direct_vector(
        element: &DirectVectorElementType,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let vector = DirectVector::from_element(element, context)?;
        Ok(Self {
            ty: Some(vector.ty().clone()),
            conversion: ReturnConversion::DirectVector(vector),
        })
    }

    fn encoded(
        ty: &TypeRef,
        codec: <OutOfRust as Direction>::Codec,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        Ok(Self {
            ty: Some(match package {
                Some(package) => KotlinType::type_ref_with_package(ty, context, package)?,
                None => KotlinType::type_ref(ty, context)?,
            }),
            conversion: ReturnConversion::Encoded {
                codec,
                package: package.cloned(),
            },
        })
    }

    fn class_handle(
        class: ClassId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let handle = ClassHandle::new(class, presence, context)?;
        let ty = handle.ty()?;
        Ok(Self {
            ty: Some(ty),
            conversion: ReturnConversion::ClassHandle(handle),
        })
    }

    fn callback_handle(
        callback: CallbackId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let handle = CallbackHandle::new(callback, presence, context)?;
        let ty = handle.ty()?;
        Ok(Self {
            ty: Some(ty),
            conversion: ReturnConversion::CallbackHandle(handle),
        })
    }

    fn scalar_option(primitive: Primitive) -> Result<Self> {
        Ok(Self {
            ty: Some(ScalarOption::new(primitive).ty()?),
            conversion: ReturnConversion::ScalarOption(primitive),
        })
    }

    fn with_byte_array_receiver_writeback(self, receiver_type: TypeName) -> Result<Self> {
        match self.ty {
            None => Ok(Self::byte_array_value(receiver_type)),
            Some(_) => Err(KotlinHost::unsupported(
                "mutable receiver with explicit return",
            )),
        }
    }

    fn with_parameter_mutation(self, mutation: ParameterMutation) -> Result<Self> {
        match self.ty {
            None => Ok(Self {
                ty: None,
                conversion: ReturnConversion::ParameterMutation(mutation),
            }),
            Some(_) => Err(KotlinHost::unsupported(
                "mutable encoded parameter with explicit return",
            )),
        }
    }

    fn return_statements(
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
            ReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            ReturnConversion::Direct(primitive) => Ok(vec![
                KotlinPrimitive::new(*primitive)
                    .public_return(call)
                    .map(Statement::expression)?,
            ]),
            ReturnConversion::ByteArrayValue(ty) => {
                let result = Identifier::parse("__boltffi_result")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                Ok(vec![
                    Statement::value(result.clone(), payload),
                    Statement::expression(Expression::call(
                        ty.clone(),
                        Identifier::parse("fromByteArray")?,
                        [Expression::identifier(result)]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    )),
                ])
            }
            ReturnConversion::DirectEnum(ty) => Ok(vec![Statement::expression(Expression::call(
                ty.clone(),
                Identifier::parse("fromValue")?,
                [call].into_iter().collect::<ArgumentList>(),
            ))]),
            ReturnConversion::DirectVector(vector) => vector.value_statements(call),
            ReturnConversion::Encoded { codec, package } => {
                let result = Identifier::parse("__boltffi_result")?;
                let reader = Identifier::parse("__boltffi_reader")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                let mut codec_reader = Reader::new(reader.clone(), host, context);
                if let Some(package) = package {
                    codec_reader = codec_reader.package(package);
                }
                let value = codec.render_with(&mut codec_reader)?.into_expression();
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
            ReturnConversion::ClassHandle(handle) => handle.value_statements(call),
            ReturnConversion::CallbackHandle(handle) => handle.value_statements(call),
            ReturnConversion::ScalarOption(primitive) => {
                ScalarOption::new(*primitive).read_value(call)
            }
            ReturnConversion::ParameterMutation(mutation) => {
                mutation.statements(call, host, context)
            }
        }
    }
}
