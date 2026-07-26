mod asynchronous;

use askama::Template as AskamaTemplate;
use asynchronous::{AsyncCall, BoundArguments};
use boltffi_binding::{
    ClassId, ClosureReturn, DataVariantPayload, DirectValueType, DirectVectorElementType,
    Direction, EnumDecl, ErrorChannel, ExecutionDecl, ExportedCallable, ExportedMethodDecl,
    FunctionDecl, HandlePresence, HandleTarget, InitializerDecl, IntoRust, Native, NativeSymbol,
    OutOfRust, ParamDecl, ParamPlanRender, Primitive as BindingPrimitive, ReadPlan, Receive,
    ReturnPlanRender, ReturnValueSlot, TypeRef, native,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{AuxChunk, Emitted, RenderContext, Result},
    target::java::{
        JavaHost, JavaPackage, JavaVersion,
        admission::{FunctionShape, ReceiverSupport},
        codec::{Reader, Runtime, WireBuffer},
        name_style::Name,
        primitive::Primitive,
        render::{
            ClosureHandle, DirectVector, Enumeration,
            callback::CallbackHandle,
            class::ClassHandle,
            native::Method,
            record::Record,
            signature::{CallSignature, Parameter, ReturnType, ValueType},
            type_name::JavaType,
        },
        syntax::{
            ArgumentList, Expression, Identifier, Javadoc, Statement, StringLiteral,
            TypeIdentifier, TypeName,
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/function.java", escape = "none")]
struct FunctionTemplate<'call> {
    call: &'call Call,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    signature: CallSignature,
    doc: Option<Javadoc>,
    execution: CallExecution,
    runtime: RuntimeRequirement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CallExecution {
    Synchronous {
        native: Method,
        body: Vec<Statement>,
    },
    Asynchronous(AsyncCall),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receiver {
    ty: TypeIdentifier,
    native: NativeArgument,
    mutation: Option<ReceiverMutation>,
    support: ReceiverSupport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueCalls {
    initializers: Vec<Call>,
    static_methods: Vec<Call>,
    instance_methods: Vec<Call>,
}

#[derive(Clone, Copy)]
pub struct AssociatedCallContext<'scope, 'bindings> {
    bridge: &'scope JniBridgeContract,
    native_owner: &'scope TypeIdentifier,
    package: Option<&'scope JavaPackage>,
    version: JavaVersion,
    context: &'scope RenderContext<'bindings, Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueReceiver {
    DirectRecord(TypeIdentifier),
    DirectEnum(TypeIdentifier),
    Encoded {
        ty: TypeIdentifier,
        codec: boltffi_binding::WritePlan,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReceiverMutation {
    Direct(Identifier),
    Encoded,
}

pub struct BoundParameter {
    signature: Parameter<ValueType>,
    native: NativeArgument,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NativeArgument {
    acquire: Vec<Statement>,
    prepare: Vec<Statement>,
    expressions: Vec<Expression>,
    cleanup: Vec<Statement>,
    runtime: RuntimeRequirement,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RuntimeRequirement {
    #[default]
    None,
    Wire,
    DirectVector,
    WireAndDirectVector,
}

struct NativeArgumentRender<'context> {
    source: Name,
    name: Identifier,
    version: JavaVersion,
    context: &'context RenderContext<'context, Native>,
}

enum ReturnConversion {
    Void,
    Direct,
    DirectRecord(TypeName),
    DirectEnum(TypeName),
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
    Encoded(ReadPlan),
    ScalarOption(Primitive),
    DirectVector(DirectVector),
}

#[derive(Clone, Copy)]
enum ReturnContext {
    Api,
    ClassFactory,
    ClassInitializer(ClassId),
}

pub struct CallReturn {
    ty: ReturnType,
    native: ReturnType,
    conversion: ReturnConversion,
}

struct CallReturnRender<'context> {
    version: JavaVersion,
    context: &'context RenderContext<'context, Native>,
    package: Option<&'context JavaPackage>,
    return_context: ReturnContext,
}

#[derive(Clone, Copy)]
pub struct CallScope<'scope, 'bindings> {
    bridge: &'scope JniBridgeContract,
    native_owner: &'scope TypeIdentifier,
    version: JavaVersion,
    context: &'scope RenderContext<'bindings, Native>,
    package: Option<&'scope JavaPackage>,
    return_context: ReturnContext,
}

#[derive(Clone)]
pub enum ErrorConversion {
    None,
    Status,
    Encoded { ty: TypeRef, codec: ReadPlan },
}

impl Call {
    pub fn from_function(
        declaration: &FunctionDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        FunctionShape::classify(declaration).require_supported()?;
        Self::build(
            Name::new(declaration.name()).function(version)?,
            declaration.symbol(),
            declaration.callable(),
            declaration.meta().doc().map(Javadoc::new),
            None,
            CallScope::api(bridge, native_owner, version, context, None),
        )
    }

    pub fn from_initializer(
        declaration: &InitializerDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        package: Option<&JavaPackage>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build_initializer(
            declaration,
            Name::new(declaration.name()).function(version)?,
            ReceiverSupport::Direct,
            CallScope::api(bridge, native_owner, version, context, package),
        )
    }

    pub fn from_class_initializer(
        declaration: &InitializerDecl<Native>,
        class: ClassId,
        name: Identifier,
        scope: AssociatedCallContext<'_, '_>,
    ) -> Result<Self> {
        Self::build_initializer(
            declaration,
            name,
            ReceiverSupport::Forbidden,
            CallScope::class_initializer(
                scope.bridge,
                scope.native_owner,
                scope.package,
                scope.version,
                scope.context,
                class,
            ),
        )
    }

    pub fn from_class_factory(
        declaration: &InitializerDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        package: Option<&JavaPackage>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build_initializer(
            declaration,
            Name::new(declaration.name()).function(version)?,
            ReceiverSupport::Direct,
            CallScope::class_factory(bridge, native_owner, version, context, package),
        )
    }

    pub fn from_method(
        declaration: &ExportedMethodDecl<Native, NativeSymbol>,
        receiver: Option<Receiver>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        package: Option<&JavaPackage>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        FunctionShape::classify_callable(
            declaration.callable(),
            receiver
                .as_ref()
                .map_or(ReceiverSupport::Direct, |receiver| receiver.support),
        )
        .require_supported()?;
        Self::build(
            Name::new(declaration.name()).function(version)?,
            declaration.target(),
            declaration.callable(),
            declaration.meta().doc().map(Javadoc::new),
            receiver,
            CallScope::api(bridge, native_owner, version, context, package),
        )
    }

    pub fn render(&self) -> Result<Emitted> {
        let emitted = self.native_forwards()?.into_iter().fold(
            Emitted::primary(FunctionTemplate { call: self }.render()?),
            Emitted::with_aux,
        );
        let emitted = match self.runtime.requires_wire() {
            true => emitted.with_aux(Runtime::helper()?),
            false => emitted,
        };
        let emitted = match self.runtime.requires_direct_vector() {
            true => emitted.with_aux(Runtime::direct_vector_helper()?),
            false => emitted,
        };
        match self.requires_async_runtime() {
            true => Ok(emitted.with_aux(Runtime::async_helper()?)),
            false => Ok(emitted),
        }
    }

    pub fn native_forwards(&self) -> Result<Vec<AuxChunk>> {
        self.execution.native_forwards()
    }

    pub fn signature(&self) -> &CallSignature {
        &self.signature
    }

    pub fn name(&self) -> &Identifier {
        self.signature.name()
    }

    pub fn parameters(&self) -> &[Parameter<ValueType>] {
        self.signature.parameters()
    }

    pub fn returns(&self) -> &ReturnType {
        self.signature.returns()
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    pub fn body(&self) -> &[Statement] {
        match &self.execution {
            CallExecution::Synchronous { body, .. } => body,
            CallExecution::Asynchronous(_) => &[],
        }
    }

    pub fn async_call(&self) -> Option<&AsyncCall> {
        match &self.execution {
            CallExecution::Synchronous { .. } => None,
            CallExecution::Asynchronous(call) => Some(call),
        }
    }

    pub fn requires_wire_runtime(&self) -> bool {
        self.runtime.requires_wire()
    }

    pub fn requires_direct_vector_runtime(&self) -> bool {
        self.runtime.requires_direct_vector()
    }

    pub fn requires_async_runtime(&self) -> bool {
        matches!(self.execution, CallExecution::Asynchronous(_))
    }

    fn build_initializer(
        declaration: &InitializerDecl<Native>,
        name: Identifier,
        receiver_support: ReceiverSupport,
        scope: CallScope<'_, '_>,
    ) -> Result<Self> {
        if declaration.callable().execution().uses_async_execution() {
            return Err(JavaHost::unsupported("asynchronous initializer"));
        }
        FunctionShape::classify_callable(declaration.callable(), receiver_support)
            .require_supported()?;
        Self::build(
            name,
            declaration.symbol(),
            declaration.callable(),
            declaration.meta().doc().map(Javadoc::new),
            None,
            scope,
        )
    }

    fn build(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        doc: Option<Javadoc>,
        receiver: Option<Receiver>,
        scope: CallScope<'_, '_>,
    ) -> Result<Self> {
        let parameters = callable
            .params()
            .iter()
            .map(|parameter| {
                BoundParameter::from_declaration(
                    parameter,
                    scope.bridge,
                    scope.version,
                    scope.context,
                    scope.package,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let declared_return = callable
            .returns()
            .plan()
            .render_with(&mut CallReturnRender {
                version: scope.version,
                context: scope.context,
                package: scope.package,
                return_context: scope.return_context,
            })?;
        let native = Method::from_symbol(symbol, scope.bridge, scope.version)?;
        let receiver_arguments = receiver
            .iter()
            .flat_map(|receiver| receiver.native.expressions.iter().cloned());
        let parameter_arguments = parameters
            .iter()
            .flat_map(|parameter| parameter.native.expressions.iter().cloned());
        let native_call = native.call(
            scope.native_owner,
            receiver_arguments.chain(parameter_arguments),
        )?;
        let error = ErrorConversion::from_channel(callable.error().channel())?;
        let runtime = receiver
            .iter()
            .map(|receiver| receiver.native.runtime)
            .chain(parameters.iter().map(|parameter| parameter.native.runtime))
            .fold(declared_return.runtime(), RuntimeRequirement::merge)
            .merge(error.runtime());
        if let ExecutionDecl::Asynchronous(protocol) = callable.execution() {
            let asynchronous = AsyncCall::new(
                protocol,
                native,
                native_call,
                &declared_return,
                &error,
                BoundArguments::new(receiver.as_ref(), &parameters),
                scope,
            )?;
            return Ok(Self {
                signature: CallSignature::new(
                    name,
                    parameters
                        .into_iter()
                        .map(|parameter| parameter.signature)
                        .collect(),
                    declared_return.ty.future(scope.version),
                )?,
                doc,
                execution: CallExecution::Asynchronous(asynchronous),
                runtime,
            });
        }
        if !matches!(callable.execution(), ExecutionDecl::Synchronous(_)) {
            return Err(JavaHost::unsupported("function execution"));
        }
        let (returns, native_returns, success) = match receiver
            .as_ref()
            .and_then(|receiver| receiver.mutation.as_ref())
        {
            Some(ReceiverMutation::Direct(buffer)) => {
                declared_return.ty.require_void()?;
                let ty = receiver
                    .as_ref()
                    .map(|receiver| receiver.ty.clone())
                    .ok_or(JavaHost::broken_bridge_contract(
                        "direct receiver mutation has no receiver",
                    ))?;
                (
                    ReturnType::Value(ValueType::Record(ty.clone())),
                    ReturnType::Void,
                    vec![
                        Statement::expression(native_call),
                        Statement::return_value(Expression::static_call(
                            TypeName::named(ty),
                            Identifier::known("fromDirectBuffer"),
                            [Expression::identifier(buffer.clone())]
                                .into_iter()
                                .collect(),
                        )),
                    ],
                )
            }
            Some(ReceiverMutation::Encoded) => {
                declared_return.ty.require_void()?;
                let ty = receiver
                    .as_ref()
                    .map(|receiver| receiver.ty.clone())
                    .ok_or(JavaHost::broken_bridge_contract(
                        "encoded receiver mutation has no receiver",
                    ))?;
                (
                    ReturnType::Value(ValueType::Record(ty.clone())),
                    ReturnType::Value(ValueType::Record(ty.clone())),
                    CallReturn::encoded_record_statements(TypeName::named(ty), native_call),
                )
            }
            None => (
                declared_return.ty.clone(),
                declared_return.native.clone(),
                declared_return.statements(
                    native_call,
                    scope.version,
                    scope.context,
                    scope.package,
                )?,
            ),
        };
        native.validate_return(&native_returns)?;
        let success = error.wrap(
            success,
            scope.version,
            scope.context,
            scope.package,
            scope.return_context,
        )?;
        let protected = receiver
            .iter()
            .flat_map(|receiver| receiver.native.prepare.iter().cloned())
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.native.prepare.iter().cloned()),
            )
            .chain(success)
            .collect::<Vec<_>>();
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.native.cleanup.iter().cloned())
            .chain(
                receiver
                    .iter()
                    .flat_map(|receiver| receiver.native.cleanup.iter().cloned()),
            )
            .collect::<Vec<_>>();
        let protected = match cleanup.is_empty() {
            true => protected,
            false => vec![Statement::try_finally(protected, cleanup)],
        };
        let body = receiver
            .iter()
            .flat_map(|receiver| receiver.native.acquire.iter().cloned())
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.native.acquire.iter().cloned()),
            )
            .chain(protected)
            .collect();
        Ok(Self {
            signature: CallSignature::new(
                name,
                parameters
                    .into_iter()
                    .map(|parameter| parameter.signature)
                    .collect(),
                returns,
            )?,
            doc,
            execution: CallExecution::Synchronous { native, body },
            runtime,
        })
    }
}

impl CallExecution {
    fn native_forwards(&self) -> Result<Vec<AuxChunk>> {
        let methods = match self {
            Self::Synchronous { native, .. } => std::slice::from_ref(native),
            Self::Asynchronous(call) => call.native_methods(),
        };
        methods
            .iter()
            .map(|method| method.render().map(Into::into).map(AuxChunk::ForwardDecl))
            .chain(matches!(self, Self::Asynchronous(_)).then(Runtime::async_callback))
            .collect()
    }
}

impl ReturnContext {
    fn initializer_failure(self) -> Option<&'static str> {
        match self {
            Self::Api => None,
            Self::ClassFactory => Some("Factory constructor failed"),
            Self::ClassInitializer(_) => Some("Constructor failed"),
        }
    }
}

impl ValueCalls {
    pub fn from_declarations(
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: ValueReceiver,
        scope: AssociatedCallContext<'_, '_>,
    ) -> Result<Self> {
        let initializers = initializers
            .iter()
            .map(|initializer| {
                Call::from_initializer(
                    initializer,
                    scope.bridge,
                    scope.native_owner,
                    scope.package,
                    scope.version,
                    scope.context,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let static_methods = methods
            .iter()
            .filter(|method| method.callable().receiver().is_none())
            .map(|method| {
                Call::from_method(
                    method,
                    None,
                    scope.bridge,
                    scope.native_owner,
                    scope.package,
                    scope.version,
                    scope.context,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let instance_methods = methods
            .iter()
            .filter_map(|method| {
                method
                    .callable()
                    .receiver()
                    .map(|receive| (method, receive))
            })
            .map(|(method, receive)| {
                receiver
                    .build(receive, scope.version, scope.context)
                    .and_then(|receiver| {
                        Call::from_method(
                            method,
                            Some(receiver),
                            scope.bridge,
                            scope.native_owner,
                            scope.package,
                            scope.version,
                            scope.context,
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            initializers,
            static_methods,
            instance_methods,
        })
    }

    pub fn initializers(&self) -> &[Call] {
        &self.initializers
    }

    pub fn static_methods(&self) -> &[Call] {
        &self.static_methods
    }

    pub fn instance_methods(&self) -> &[Call] {
        &self.instance_methods
    }

    pub fn iter(&self) -> impl Iterator<Item = &Call> {
        self.initializers
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    pub fn into_parts(self) -> (Vec<Call>, Vec<Call>, Vec<Call>) {
        (
            self.initializers,
            self.static_methods,
            self.instance_methods,
        )
    }
}

impl<'scope, 'bindings> AssociatedCallContext<'scope, 'bindings> {
    pub fn local(
        bridge: &'scope JniBridgeContract,
        native_owner: &'scope TypeIdentifier,
        version: JavaVersion,
        context: &'scope RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            bridge,
            native_owner,
            package: None,
            version,
            context,
        }
    }

    pub fn nested(
        bridge: &'scope JniBridgeContract,
        native_owner: &'scope TypeIdentifier,
        package: &'scope JavaPackage,
        version: JavaVersion,
        context: &'scope RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            bridge,
            native_owner,
            package: Some(package),
            version,
            context,
        }
    }
}

impl ValueReceiver {
    fn build<'context>(
        &self,
        receive: Receive,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
    ) -> Result<Receiver> {
        match self {
            Self::DirectRecord(ty) => Receiver::direct(ty.clone(), receive),
            Self::DirectEnum(ty) => Receiver::enumeration(ty.clone(), receive),
            Self::Encoded { ty, codec } => {
                Receiver::encoded(ty.clone(), receive, codec, version, context)
            }
        }
    }
}

impl<'scope, 'bindings> CallScope<'scope, 'bindings> {
    fn api(
        bridge: &'scope JniBridgeContract,
        native_owner: &'scope TypeIdentifier,
        version: JavaVersion,
        context: &'scope RenderContext<'bindings, Native>,
        package: Option<&'scope JavaPackage>,
    ) -> Self {
        Self {
            bridge,
            native_owner,
            version,
            context,
            package,
            return_context: ReturnContext::Api,
        }
    }

    fn class_initializer(
        bridge: &'scope JniBridgeContract,
        native_owner: &'scope TypeIdentifier,
        package: Option<&'scope JavaPackage>,
        version: JavaVersion,
        context: &'scope RenderContext<'bindings, Native>,
        class: ClassId,
    ) -> Self {
        Self {
            bridge,
            native_owner,
            version,
            context,
            package,
            return_context: ReturnContext::ClassInitializer(class),
        }
    }

    fn class_factory(
        bridge: &'scope JniBridgeContract,
        native_owner: &'scope TypeIdentifier,
        version: JavaVersion,
        context: &'scope RenderContext<'bindings, Native>,
        package: Option<&'scope JavaPackage>,
    ) -> Self {
        Self {
            bridge,
            native_owner,
            version,
            context,
            package,
            return_context: ReturnContext::ClassFactory,
        }
    }
}

impl BoundParameter {
    fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        bridge: &JniBridgeContract,
        version: JavaVersion,
        context: &RenderContext<Native>,
        package: Option<&JavaPackage>,
    ) -> Result<Self> {
        let source = Name::new(parameter.name());
        let name = source.parameter(version)?;
        let native = match parameter.payload().as_value() {
            Some(plan) => plan.render_with(&mut NativeArgumentRender {
                source,
                name: name.clone(),
                version,
                context,
            })?,
            None => ClosureHandle::new(
                parameter
                    .payload()
                    .as_closure()
                    .ok_or_else(FunctionShape::unexpected_shape)?,
                bridge,
                version,
            )?
            .native_argument(Expression::identifier(name.clone()))
            .map(NativeArgument::direct)?,
        };
        Ok(Self {
            signature: Parameter::from_declaration(parameter, version, context, package)?,
            native,
        })
    }
}

impl NativeArgument {
    fn direct(expression: Expression) -> Self {
        Self {
            acquire: Vec::new(),
            prepare: Vec::new(),
            expressions: vec![expression],
            cleanup: Vec::new(),
            runtime: RuntimeRequirement::None,
        }
    }

    fn encoded(write: crate::target::java::codec::EncodedWrite) -> Self {
        let (acquire, prepare, expressions, cleanup) = write.into_parts();
        Self {
            acquire,
            prepare,
            expressions,
            cleanup,
            runtime: RuntimeRequirement::Wire,
        }
    }
}

impl RuntimeRequirement {
    fn merge(self, other: Self) -> Self {
        match (
            self.requires_wire() || other.requires_wire(),
            self.requires_direct_vector() || other.requires_direct_vector(),
        ) {
            (false, false) => Self::None,
            (true, false) => Self::Wire,
            (false, true) => Self::DirectVector,
            (true, true) => Self::WireAndDirectVector,
        }
    }

    fn requires_wire(self) -> bool {
        matches!(self, Self::Wire | Self::WireAndDirectVector)
    }

    fn requires_direct_vector(self) -> bool {
        matches!(self, Self::DirectVector | Self::WireAndDirectVector)
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for NativeArgumentRender<'_> {
    type Output = Result<NativeArgument>;

    fn direct(&mut self, ty: &'plan DirectValueType, _receive: Receive) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match ty {
            DirectValueType::Primitive(_) => Ok(NativeArgument::direct(value)),
            DirectValueType::Record(_) => Ok(NativeArgument::direct(
                value.call(Identifier::known("toDirectBuffer"), ArgumentList::default()),
            )),
            DirectValueType::Enum(enumeration) => {
                Enumeration::type_name_for(*enumeration, self.context, self.version).map(|_| {
                    NativeArgument::direct(
                        value.call(Identifier::known("nativeValue"), ArgumentList::default()),
                    )
                })
            }
            _ => Err(JavaHost::unsupported("unknown direct function parameter")),
        }
    }

    fn encoded(
        &mut self,
        _ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        _shape: native::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        if receive == Receive::ByMutRef {
            return Err(JavaHost::unsupported("mutable encoded parameter"));
        }
        WireBuffer::new(&self.source, self.version)
            .and_then(|buffer| {
                buffer.write(
                    codec,
                    Expression::identifier(self.name.clone()),
                    self.context,
                )
            })
            .map(NativeArgument::encoded)
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        _receive: Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => {
                ClassHandle::new(*class, carrier, presence, self.version, self.context, None)
                    .and_then(|handle| {
                        handle.native_argument(Expression::identifier(self.name.clone()))
                    })
                    .map(NativeArgument::direct)
            }
            HandleTarget::Callback(callback) => CallbackHandle::new(
                *callback,
                carrier,
                presence,
                self.version,
                self.context,
                None,
            )
            .and_then(|handle| handle.native_argument(Expression::identifier(self.name.clone())))
            .map(NativeArgument::direct),
            _ => Err(JavaHost::unsupported("handle function parameter")),
        }
    }

    fn scalar_option(&mut self, primitive: BindingPrimitive) -> Self::Output {
        let primitive = Primitive::try_from(primitive)?;
        let value = Expression::identifier(self.name.clone());
        let writer = self.source.generated("writer", self.version)?;
        let payload = self.source.generated("value", self.version)?;
        WireBuffer::new(&self.source, self.version)
            .and_then(|buffer| {
                buffer.write_statements(
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
                )
            })
            .map(NativeArgument::encoded)
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        _: Receive,
    ) -> Self::Output {
        let vector = DirectVector::from_element(element, self.version, self.context)?;
        Ok(NativeArgument {
            acquire: Vec::new(),
            prepare: Vec::new(),
            expressions: vec![vector.native_argument(Expression::identifier(self.name.clone()))],
            cleanup: Vec::new(),
            runtime: RuntimeRequirement::DirectVector,
        })
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for CallReturnRender<'_> {
    type Output = Result<CallReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(CallReturn {
            ty: ReturnType::Void,
            native: ReturnType::Void,
            conversion: ReturnConversion::Void,
        })
    }

    fn direct(&mut self, _slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => Ok(CallReturn {
                ty: ReturnType::Value(ValueType::Primitive(Primitive::try_from(*primitive)?)),
                native: ReturnType::Value(ValueType::Primitive(Primitive::try_from(*primitive)?)),
                conversion: ReturnConversion::Direct,
            }),
            DirectValueType::Record(record) => {
                let record = Record::type_name_for(*record, self.context, self.version)?;
                let public = self.generated_type(record.clone());
                Ok(CallReturn {
                    ty: ReturnType::Value(ValueType::Reference(public)),
                    native: ReturnType::Value(ValueType::Record(record.clone())),
                    conversion: ReturnConversion::DirectRecord(self.generated_type(record)),
                })
            }
            DirectValueType::Enum(enumeration) => {
                let ty = Enumeration::type_name_for(*enumeration, self.context, self.version)?;
                let primitive = Enumeration::c_style_primitive(*enumeration, self.context)?;
                Ok(CallReturn {
                    ty: ReturnType::Value(ValueType::Reference(self.generated_type(ty.clone()))),
                    native: ReturnType::Value(ValueType::Primitive(primitive)),
                    conversion: ReturnConversion::DirectEnum(self.generated_type(ty)),
                })
            }
            _ => Err(JavaHost::unsupported("unknown direct function return")),
        }
    }

    fn encoded(
        &mut self,
        _slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        _shape: native::BufferShape,
    ) -> Self::Output {
        Ok(CallReturn {
            ty: ReturnType::Value(ValueType::Reference(self.type_ref(ty)?)),
            native: ReturnType::Value(ValueType::Reference(JavaType::type_ref(
                ty,
                self.version,
                self.context,
            )?)),
            conversion: ReturnConversion::Encoded(codec.clone()),
        })
    }

    fn handle(
        &mut self,
        _slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(
                    *class,
                    carrier,
                    presence,
                    self.version,
                    self.context,
                    self.package,
                )?;
                let carrier = ReturnType::Value(ValueType::Primitive(handle.carrier()));
                match self.return_context {
                    ReturnContext::Api | ReturnContext::ClassFactory => Ok(CallReturn {
                        ty: ReturnType::Value(ValueType::Reference(handle.ty().clone())),
                        native: carrier,
                        conversion: ReturnConversion::ClassHandle(handle),
                    }),
                    ReturnContext::ClassInitializer(expected)
                        if expected == *class && presence == HandlePresence::Required =>
                    {
                        Ok(CallReturn {
                            ty: carrier.clone(),
                            native: carrier,
                            conversion: ReturnConversion::Direct,
                        })
                    }
                    ReturnContext::ClassInitializer(_) => Err(JavaHost::broken_bridge_contract(
                        "class initializer returns its owner",
                    )),
                }
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(
                    *callback,
                    carrier,
                    presence,
                    self.version,
                    self.context,
                    self.package,
                )?;
                match self.return_context {
                    ReturnContext::Api | ReturnContext::ClassFactory => Ok(CallReturn {
                        ty: ReturnType::Value(ValueType::Reference(handle.ty().clone())),
                        native: ReturnType::Value(ValueType::Primitive(handle.carrier())),
                        conversion: ReturnConversion::CallbackHandle(handle),
                    }),
                    ReturnContext::ClassInitializer(_) => Err(JavaHost::broken_bridge_contract(
                        "class initializer returns its owner",
                    )),
                }
            }
            _ => Err(JavaHost::unsupported("handle function return")),
        }
    }

    fn scalar_option(&mut self, primitive: BindingPrimitive) -> Self::Output {
        let primitive = Primitive::try_from(primitive)?;
        Ok(CallReturn {
            ty: ReturnType::Value(ValueType::Reference(JavaType::optional_primitive(
                primitive,
                self.version,
            ))),
            native: ReturnType::Value(ValueType::Reference(JavaType::optional_primitive(
                primitive,
                self.version,
            ))),
            conversion: ReturnConversion::ScalarOption(primitive),
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let vector = DirectVector::from_element(element, self.version, self.context)?;
        Ok(CallReturn {
            ty: ReturnType::Value(ValueType::Reference(vector.ty().clone())),
            native: ReturnType::Value(ValueType::Reference(CallReturn::byte_array())),
            conversion: ReturnConversion::DirectVector(vector),
        })
    }

    fn closure(&mut self, _closure: &'plan ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(JavaHost::unsupported("closure function return"))
    }
}

impl CallReturnRender<'_> {
    fn generated_type(&self, name: TypeIdentifier) -> TypeName {
        match self.package {
            Some(package) => package.type_name(name),
            None => TypeName::named(name),
        }
    }

    fn type_ref(&self, ty: &TypeRef) -> Result<TypeName> {
        match self.package {
            Some(package) => JavaType::qualified_type_ref(ty, self.version, self.context, package),
            None => JavaType::type_ref(ty, self.version, self.context),
        }
    }
}

impl CallReturn {
    fn runtime(&self) -> RuntimeRequirement {
        match self.conversion {
            ReturnConversion::Encoded(_) | ReturnConversion::ScalarOption(_) => {
                RuntimeRequirement::Wire
            }
            ReturnConversion::DirectVector(_) => RuntimeRequirement::DirectVector,
            ReturnConversion::Void
            | ReturnConversion::Direct
            | ReturnConversion::DirectRecord(_)
            | ReturnConversion::DirectEnum(_)
            | ReturnConversion::ClassHandle(_)
            | ReturnConversion::CallbackHandle(_) => RuntimeRequirement::None,
        }
    }

    fn statements(
        &self,
        call: Expression,
        version: JavaVersion,
        context: &RenderContext<Native>,
        package: Option<&JavaPackage>,
    ) -> Result<Vec<Statement>> {
        match &self.conversion {
            ReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            ReturnConversion::Direct => Ok(vec![Statement::return_value(call)]),
            ReturnConversion::DirectRecord(record) => {
                Ok(Self::encoded_record_statements(record.clone(), call))
            }
            ReturnConversion::DirectEnum(enumeration) => {
                Ok(vec![Statement::return_value(Expression::static_call(
                    enumeration.clone(),
                    Identifier::known("fromValue"),
                    [call].into_iter().collect(),
                ))])
            }
            ReturnConversion::ClassHandle(handle) => handle.value_statements(call),
            ReturnConversion::CallbackHandle(handle) => handle.value_statements(call),
            ReturnConversion::Encoded(codec) => {
                let result = Identifier::known("__boltffi_result");
                let reader = Identifier::known("__boltffi_reader");
                let mut codec_reader = Reader::new(reader.clone(), version, context);
                if let Some(package) = package {
                    codec_reader = codec_reader.package(package);
                }
                let decoded = codec.render_with(&mut codec_reader)?.into_expression();
                Ok(vec![
                    Statement::value(Self::byte_array(), result.clone(), call),
                    Statement::value(
                        TypeName::named(TypeIdentifier::known("WireReader", version)),
                        reader,
                        Expression::construct(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            [Expression::identifier(result)].into_iter().collect(),
                        ),
                    ),
                    Statement::return_value(decoded),
                ])
            }
            ReturnConversion::ScalarOption(primitive) => {
                let result = Identifier::known("__boltffi_result");
                let reader = Identifier::known("__boltffi_reader");
                let reader_value = Expression::identifier(reader.clone());
                let decoded = reader_value.clone().call(
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
                );
                Ok(vec![
                    Statement::value(Self::byte_array(), result.clone(), call),
                    Statement::value(
                        TypeName::named(TypeIdentifier::known("WireReader", version)),
                        reader,
                        Expression::construct(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            [Expression::identifier(result)].into_iter().collect(),
                        ),
                    ),
                    Statement::return_value(decoded),
                ])
            }
            ReturnConversion::DirectVector(vector) => {
                let result = Identifier::known("__boltffi_result");
                Ok(vec![
                    Statement::value(Self::byte_array(), result.clone(), call),
                    Statement::return_value(
                        vector.returned_expression(Expression::identifier(result)),
                    ),
                ])
            }
        }
    }

    fn encoded_record_statements(record: TypeName, call: Expression) -> Vec<Statement> {
        vec![Statement::return_value(Expression::static_call(
            record,
            Identifier::known("fromByteArray"),
            [call].into_iter().collect(),
        ))]
    }

    fn byte_array() -> TypeName {
        TypeName::array(TypeName::primitive(Primitive::Byte))
    }
}

impl ErrorConversion {
    fn runtime(&self) -> RuntimeRequirement {
        match self {
            Self::Encoded { .. } => RuntimeRequirement::Wire,
            Self::None | Self::Status => RuntimeRequirement::None,
        }
    }

    fn from_channel(channel: ErrorChannel<'_, Native, OutOfRust>) -> Result<Self> {
        match channel {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Status => Ok(Self::Status),
            ErrorChannel::Encoded { ty, codec, .. } => Ok(Self::Encoded {
                ty: ty.clone(),
                codec: codec.clone(),
            }),
            _ => Err(JavaHost::unsupported("unknown function error channel")),
        }
    }

    fn wrap(
        self,
        success: Vec<Statement>,
        version: JavaVersion,
        context: &RenderContext<Native>,
        package: Option<&JavaPackage>,
        return_context: ReturnContext,
    ) -> Result<Vec<Statement>> {
        match (self, return_context.initializer_failure()) {
            (Self::None | Self::Status, _) => Ok(success),
            (Self::Encoded { .. }, Some(message)) => Ok(vec![Statement::try_catch(
                success,
                TypeName::named(TypeIdentifier::known(
                    "BoltFfiErrorBufferException",
                    version,
                )),
                Identifier::known("__boltffi_error"),
                vec![Statement::throw_value(Expression::construct(
                    TypeName::named(TypeIdentifier::known("RuntimeException", version)),
                    [Expression::string(StringLiteral::new(message))]
                        .into_iter()
                        .collect(),
                ))],
            )]),
            (Self::Encoded { ty, codec }, None) => {
                let error = Identifier::known("__boltffi_error");
                let reader = Identifier::known("__boltffi_error_reader");
                let mut codec_reader = Reader::new(reader.clone(), version, context);
                if let Some(package) = package {
                    codec_reader = codec_reader.package(package);
                }
                let decoded = codec.render_with(&mut codec_reader)?.into_expression();
                let thrown =
                    match ty {
                        TypeRef::String => Expression::construct(
                            TypeName::named(TypeIdentifier::known("RuntimeException", version)),
                            [decoded].into_iter().collect(),
                        ),
                        TypeRef::Record(_) => decoded,
                        TypeRef::Enum(id) => match context.enumeration(id).ok_or(
                            JavaHost::broken_bridge_contract(
                                "enum error type was not found in render context",
                            ),
                        )? {
                            EnumDecl::CStyle(_) => Expression::construct(
                                TypeName::nested(
                                    TypeName::named(Enumeration::type_name_for(
                                        id, context, version,
                                    )?),
                                    TypeIdentifier::known("Exception", version),
                                ),
                                [decoded].into_iter().collect(),
                            ),
                            EnumDecl::Data(enumeration)
                                if enumeration.variants().iter().all(|variant| {
                                    matches!(variant.payload(), DataVariantPayload::Unit)
                                }) =>
                            {
                                Expression::construct(
                                    TypeName::nested(
                                        TypeName::named(Enumeration::type_name_for(
                                            id, context, version,
                                        )?),
                                        TypeIdentifier::known("Exception", version),
                                    ),
                                    [decoded].into_iter().collect(),
                                )
                            }
                            EnumDecl::Data(_) => decoded,
                            _ => return Err(JavaHost::unsupported("unknown Java enum error type")),
                        },
                        _ => return Err(JavaHost::unsupported("Java throwable error type")),
                    };
                Ok(vec![Statement::try_catch(
                    success,
                    TypeName::named(TypeIdentifier::known(
                        "BoltFfiErrorBufferException",
                        version,
                    )),
                    error.clone(),
                    vec![
                        Statement::value(
                            TypeName::named(TypeIdentifier::known("WireReader", version)),
                            reader,
                            Expression::construct(
                                TypeName::named(TypeIdentifier::known("WireReader", version)),
                                [Expression::identifier(error)
                                    .call(Identifier::known("bytes"), ArgumentList::default())]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                        Statement::throw_value(thrown),
                    ],
                )])
            }
        }
    }
}

impl Receiver {
    pub fn class(
        ty: TypeIdentifier,
        carrier: native::HandleCarrier,
        receive: Receive,
    ) -> Result<Self> {
        Primitive::from_handle_carrier(carrier)?;
        match receive {
            Receive::ByRef | Receive::ByMutRef => Ok(Self {
                ty,
                native: NativeArgument::direct(
                    Expression::this().call(Identifier::known("rawHandle"), Default::default()),
                ),
                mutation: None,
                support: ReceiverSupport::Handle(carrier),
            }),
            _ => Err(JavaHost::unsupported("class method receiver")),
        }
    }

    pub fn direct(ty: TypeIdentifier, receive: Receive) -> Result<Self> {
        let value =
            Expression::this().call(Identifier::known("toDirectBuffer"), Default::default());
        match receive {
            Receive::ByMutRef => {
                let buffer = Identifier::known("__boltffi_receiver");
                Ok(Self {
                    ty,
                    native: NativeArgument {
                        expressions: vec![Expression::identifier(buffer.clone())],
                        acquire: Vec::new(),
                        prepare: vec![Statement::value(
                            TypeName::qualified(
                                [Identifier::known("java"), Identifier::known("nio")].into(),
                                TypeIdentifier::known("ByteBuffer", JavaVersion::JAVA_8),
                            ),
                            buffer.clone(),
                            value,
                        )],
                        cleanup: Vec::new(),
                        runtime: RuntimeRequirement::None,
                    },
                    mutation: Some(ReceiverMutation::Direct(buffer)),
                    support: ReceiverSupport::Direct,
                })
            }
            Receive::ByRef | Receive::ByValue => Ok(Self {
                ty,
                native: NativeArgument::direct(value),
                mutation: None,
                support: ReceiverSupport::Direct,
            }),
            _ => Err(JavaHost::unsupported("record method receiver")),
        }
    }

    pub fn encoded(
        ty: TypeIdentifier,
        receive: Receive,
        codec: &boltffi_binding::WritePlan,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        WireBuffer::receiver(version)
            .and_then(|buffer| buffer.write(codec, Expression::this(), context))
            .map(NativeArgument::encoded)
            .map(|native| Self {
                ty,
                native,
                mutation: (receive == Receive::ByMutRef).then_some(ReceiverMutation::Encoded),
                support: ReceiverSupport::Encoded,
            })
    }

    pub fn enumeration(ty: TypeIdentifier, receive: Receive) -> Result<Self> {
        match receive {
            Receive::ByRef | Receive::ByValue => Ok(Self {
                ty,
                native: NativeArgument::direct(
                    Expression::this().call(Identifier::known("nativeValue"), Default::default()),
                ),
                mutation: None,
                support: ReceiverSupport::Direct,
            }),
            _ => Err(JavaHost::unsupported("mutable c-style enum receiver")),
        }
    }
}
