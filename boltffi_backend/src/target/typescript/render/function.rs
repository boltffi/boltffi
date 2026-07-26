use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackId, CanonicalName, ClassId, DirectValueType, DirectVectorElementType, EnumDecl, EnumId,
    ErrorChannel, ErrorPlacement, ExecutionDecl, ExportedCallable, ExportedMethodDecl,
    FunctionDecl, HandlePresence, HandleTarget, InitializerDecl, IntoRust, NativeSymbol,
    ParamPlanRender, Primitive, Receive, RecordDecl, RecordId, ReturnPlanRender, ReturnValueSlot,
    TypeRef, Wasm32, WasmIncomingClosure, wasm32,
};

use crate::core::{CoverageMode, Diagnostic, Emitted, Error, RenderContext, Result};

use super::super::{
    codec::{ReadKind, Reader, SizeKind, Sizer, WriteKind, Writer},
    name_style::Name,
    primitive::Scalar,
    render::{Type, direct_vector::DirectVector, scalar_option::ScalarOption},
    syntax::{
        ArgumentList, Expression, Identifier, MemberName, MethodDeclaration, Statement,
        StringLiteral, TypeName,
    },
};
use super::closure::ClosureAdapter;

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/function.ts", escape = "none")]
pub struct Function {
    name: Identifier,
    member: MemberName,
    parameters: Vec<Parameter>,
    returns: TypeName,
    body: Vec<Statement>,
    asynchronous: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Parameter {
    name: Identifier,
    ty: TypeName,
    setup: Vec<Statement>,
    arguments: Vec<Expression>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Return {
    ty: TypeName,
    conversion: ReturnConversion,
    setup: Vec<Statement>,
    arguments: Vec<Expression>,
    cleanup: Vec<Statement>,
    success: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ReturnConversion {
    Void,
    Direct,
    Boolean,
    String,
    Utf8String,
    Bytes,
    Encoded {
        reader: Identifier,
        decode: Expression,
    },
    PackedOptional {
        take: Identifier,
    },
    DirectVector {
        take: Expression,
    },
    ScalarOption {
        unpack: Identifier,
    },
    DirectRecord {
        writer: Identifier,
        codec: Identifier,
    },
    ClassHandle {
        class: TypeName,
        nullable: bool,
    },
    CallbackHandle {
        wrap: Identifier,
        nullable: bool,
    },
    Out,
}

enum Failure {
    None,
    Encoded {
        value: FailureValue,
        action: FailureAction,
    },
}

#[derive(Clone, Copy)]
enum FailurePolicy {
    Throw,
    ReturnNull,
}

enum FailureAction {
    Throw(Exception),
    ReturnNull,
}

enum FailureValue {
    String,
    Utf8String,
    Encoded {
        reader: Identifier,
        decode: Expression,
    },
}

enum Exception {
    String,
    Typed(TypeName),
}

struct ParameterRenderer<'context> {
    name: Identifier,
    context: &'context RenderContext<'context, Wasm32>,
}

struct ReturnRenderer<'context> {
    context: &'context RenderContext<'context, Wasm32>,
}

struct CallReceiver {
    parameter: Option<Parameter>,
    setup: Vec<Statement>,
    arguments: Vec<Expression>,
    mutation: Option<ReceiverMutation>,
}

enum ReceiverMutation {
    Direct {
        ty: TypeName,
        value: Identifier,
    },
    Encoded {
        ty: TypeName,
        descriptor: Identifier,
        reader: Identifier,
        decode: Expression,
    },
}

#[derive(Clone, Copy)]
enum ReceiverOwner {
    Record(RecordId),
    CStyleEnum(EnumId),
    DataEnum(EnumId),
}

impl Function {
    pub fn from_declaration(
        declaration: &FunctionDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable(
            declaration.name(),
            declaration.symbol().name().as_str(),
            declaration.callable(),
            None,
            context,
        )
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?))
    }

    pub fn record_methods(
        owner: RecordId,
        initializers: &[InitializerDecl<Wasm32>],
        methods: &[ExportedMethodDecl<Wasm32, NativeSymbol>],
        context: &RenderContext<Wasm32>,
    ) -> Result<(Vec<MethodDeclaration>, Vec<Diagnostic>)> {
        Self::owned_methods(ReceiverOwner::Record(owner), initializers, methods, context)
    }

    pub fn c_style_enum_methods(
        owner: EnumId,
        initializers: &[InitializerDecl<Wasm32>],
        methods: &[ExportedMethodDecl<Wasm32, NativeSymbol>],
        context: &RenderContext<Wasm32>,
    ) -> Result<(Vec<MethodDeclaration>, Vec<Diagnostic>)> {
        Self::owned_methods(
            ReceiverOwner::CStyleEnum(owner),
            initializers,
            methods,
            context,
        )
    }

    pub fn data_enum_methods(
        owner: EnumId,
        initializers: &[InitializerDecl<Wasm32>],
        methods: &[ExportedMethodDecl<Wasm32, NativeSymbol>],
        context: &RenderContext<Wasm32>,
    ) -> Result<(Vec<MethodDeclaration>, Vec<Diagnostic>)> {
        Self::owned_methods(
            ReceiverOwner::DataEnum(owner),
            initializers,
            methods,
            context,
        )
    }

    pub fn from_class_initializer(
        initializer: &InitializerDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable_with_failure(
            initializer.name(),
            initializer.symbol().name().as_str(),
            initializer.callable(),
            None,
            FailurePolicy::ReturnNull,
            context,
        )
    }

    pub fn from_class_method(
        method: &ExportedMethodDecl<Wasm32, NativeSymbol>,
        class: &TypeName,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable(
            method.name(),
            method.target().name().as_str(),
            method.callable(),
            method
                .callable()
                .receiver()
                .map(|_| CallReceiver::class(class)),
            context,
        )
    }

    pub fn render_class_method(&self, static_method: bool) -> Result<MethodDeclaration> {
        #[derive(AskamaTemplate)]
        #[template(path = "target/typescript/class_method.ts", escape = "none")]
        struct ClassMethod<'function> {
            name: &'function MemberName,
            parameters: &'function [Parameter],
            returns: &'function TypeName,
            body: &'function [Statement],
            static_method: bool,
            asynchronous: bool,
        }

        Ok(MethodDeclaration::new(
            ClassMethod {
                name: &self.member,
                parameters: &self.parameters,
                returns: &self.returns,
                body: &self.body,
                static_method,
                asynchronous: self.asynchronous,
            }
            .render()?,
        ))
    }

    pub fn callback_method(
        method: &boltffi_binding::CallbackLocalMethodDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<MethodDeclaration> {
        let symbol = method
            .target()
            .segments()
            .last()
            .ok_or_else(|| Self::unsupported("callback local method target"))?;
        Self::from_callable(
            method.name(),
            symbol.as_str(),
            method.callable(),
            Some(CallReceiver::callback()),
            context,
        )?
        .render_class_method(false)
    }

    pub fn constant_accessor(
        name: &boltffi_binding::CanonicalName,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable(name, symbol.name().as_str(), callable, None, context)
    }

    pub fn render_local(&self, name: &Identifier) -> Result<String> {
        #[derive(AskamaTemplate)]
        #[template(path = "target/typescript/local_function.ts", escape = "none")]
        struct LocalFunction<'function> {
            name: &'function Identifier,
            returns: &'function TypeName,
            body: &'function [Statement],
        }

        Ok(LocalFunction {
            name,
            returns: &self.returns,
            body: &self.body,
        }
        .render()?)
    }

    pub fn return_type(&self) -> &TypeName {
        &self.returns
    }

    fn from_initializer(
        initializer: &InitializerDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable(
            initializer.name(),
            initializer.symbol().name().as_str(),
            initializer.callable(),
            None,
            context,
        )
    }

    fn from_record_method(
        method: &ExportedMethodDecl<Wasm32, NativeSymbol>,
        owner: RecordId,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable(
            method.name(),
            method.target().name().as_str(),
            method.callable(),
            method
                .callable()
                .receiver()
                .map(|receive| CallReceiver::value(ReceiverOwner::Record(owner), receive, context))
                .transpose()?,
            context,
        )
    }

    fn from_enum_method(
        method: &ExportedMethodDecl<Wasm32, NativeSymbol>,
        owner: ReceiverOwner,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable(
            method.name(),
            method.target().name().as_str(),
            method.callable(),
            method
                .callable()
                .receiver()
                .map(|receive| CallReceiver::value(owner, receive, context))
                .transpose()?,
            context,
        )
    }

    fn from_owned_initializer(
        initializer: &InitializerDecl<Wasm32>,
        owner: ReceiverOwner,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let mut function = Self::from_initializer(initializer, context)?;
        if matches!(owner, ReceiverOwner::CStyleEnum(_))
            && initializer.name() == &CanonicalName::single("new")
        {
            function.member = MemberName::parse("fromRaw")?;
        }
        Ok(function)
    }

    fn render_method(&self) -> Result<MethodDeclaration> {
        #[derive(AskamaTemplate)]
        #[template(path = "target/typescript/method.ts", escape = "none")]
        struct Method<'function> {
            name: &'function MemberName,
            parameters: &'function [Parameter],
            returns: &'function TypeName,
            body: &'function [Statement],
            asynchronous: bool,
        }

        Ok(MethodDeclaration::new(
            Method {
                name: &self.member,
                parameters: &self.parameters,
                returns: &self.returns,
                body: &self.body,
                asynchronous: self.asynchronous,
            }
            .render()?,
        ))
    }

    fn owned_methods(
        owner: ReceiverOwner,
        initializers: &[InitializerDecl<Wasm32>],
        methods: &[ExportedMethodDecl<Wasm32, NativeSymbol>],
        context: &RenderContext<Wasm32>,
    ) -> Result<(Vec<MethodDeclaration>, Vec<Diagnostic>)> {
        initializers
            .iter()
            .map(|initializer| {
                (
                    initializer.name(),
                    Self::from_owned_initializer(initializer, owner, context),
                )
            })
            .chain(methods.iter().map(|method| {
                let function = match owner {
                    ReceiverOwner::Record(owner) => {
                        Self::from_record_method(method, owner, context)
                    }
                    ReceiverOwner::CStyleEnum(_) | ReceiverOwner::DataEnum(_) => {
                        Self::from_enum_method(method, owner, context)
                    }
                };
                (method.name(), function)
            }))
            .try_fold(
                (Vec::new(), Vec::new()),
                |(mut rendered, mut diagnostics), (name, function)| match function {
                    Ok(function) => {
                        rendered.push(function.render_method()?);
                        Ok((rendered, diagnostics))
                    }
                    Err(Error::UnsupportedTarget { shape, .. })
                        if matches!(context.coverage_mode(), CoverageMode::Partial) =>
                    {
                        diagnostics.push(Diagnostic::new(format!(
                            "{}: {shape}",
                            name.as_path_string()
                        )));
                        Ok((rendered, diagnostics))
                    }
                    Err(error) => Err(error),
                },
            )
    }

    fn from_callable(
        name: &boltffi_binding::CanonicalName,
        symbol: &str,
        callable: &ExportedCallable<Wasm32>,
        receiver: Option<CallReceiver>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::from_callable_with_failure(
            name,
            symbol,
            callable,
            receiver,
            FailurePolicy::Throw,
            context,
        )
    }

    fn from_callable_with_failure(
        name: &boltffi_binding::CanonicalName,
        symbol: &str,
        callable: &ExportedCallable<Wasm32>,
        receiver: Option<CallReceiver>,
        failure_policy: FailurePolicy,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let failure = Failure::new(callable.error().channel(), failure_policy, context)?;
        let parameters = receiver
            .as_ref()
            .and_then(|receiver| receiver.parameter.clone())
            .map(Ok)
            .into_iter()
            .chain(callable.params().iter().map(|parameter| {
                let name = Name::new(parameter.name()).identifier()?;
                match parameter.payload() {
                    boltffi_binding::IncomingParam::Value(plan) => {
                        plan.render_with(&mut ParameterRenderer { name, context })
                    }
                    boltffi_binding::IncomingParam::Closure(closure) => {
                        Parameter::closure(name, closure, context)
                    }
                }
            }))
            .collect::<Result<Vec<_>>>()?;
        let returns = callable
            .returns()
            .plan()
            .render_with(&mut ReturnRenderer { context })?;
        let arguments = receiver
            .iter()
            .flat_map(|receiver| receiver.arguments.iter().cloned())
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.arguments.iter().cloned()),
            )
            .collect::<ArgumentList>();
        let symbol = Identifier::parse(symbol)?;
        let parameter_setup = parameters
            .iter()
            .flat_map(|parameter| parameter.setup.iter().cloned());
        let parameter_cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup.iter().cloned())
            .collect::<Vec<_>>();
        let (call, asynchronous) = match callable.execution() {
            ExecutionDecl::Synchronous(_) => {
                let arguments = arguments
                    .into_iter()
                    .chain(returns.arguments.iter().cloned())
                    .collect::<ArgumentList>();
                let native_call = Expression::native_call(symbol, arguments);
                let call = match receiver
                    .as_ref()
                    .and_then(|receiver| receiver.mutation.as_ref())
                {
                    Some(mutation) => mutation.render(native_call, &returns, &failure)?,
                    None => failure.render(native_call, &returns)?,
                };
                let call = returns
                    .setup
                    .iter()
                    .cloned()
                    .chain(match returns.cleanup.is_empty() {
                        true => call,
                        false => vec![Statement::try_finally(call, returns.cleanup.clone())],
                    })
                    .collect();
                (call, false)
            }
            ExecutionDecl::Asynchronous(protocol) => {
                if receiver
                    .as_ref()
                    .is_some_and(|receiver| receiver.mutation.is_some())
                {
                    return Err(Self::unsupported("asynchronous mutable receiver"));
                }
                (
                    returns.render_async(
                        Expression::native_call(symbol, arguments),
                        protocol,
                        &failure,
                    )?,
                    true,
                )
            }
            _ => return Err(Self::unsupported("unknown execution protocol")),
        };
        let receiver_setup = receiver
            .as_ref()
            .into_iter()
            .flat_map(|receiver| receiver.setup.iter().cloned());
        let body = receiver_setup
            .chain(parameter_setup)
            .chain(match parameter_cleanup.is_empty() {
                true => call,
                false => vec![Statement::try_finally(call, parameter_cleanup)],
            })
            .collect();
        let return_type = match receiver.and_then(|receiver| receiver.mutation) {
            Some(mutation) => mutation.ty(),
            None if failure.returns_null() => returns.ty.clone().nullable(),
            None => returns.ty,
        };
        let name = Name::new(name);
        Ok(Self {
            name: name.identifier()?,
            member: name.member()?,
            parameters,
            returns: return_type,
            body,
            asynchronous,
        })
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl CallReceiver {
    fn value(
        owner: ReceiverOwner,
        receive: Receive,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Parameter::receiver(owner, receive, context).map(|(parameter, mutation)| Self {
            parameter: Some(parameter),
            setup: Vec::new(),
            arguments: Vec::new(),
            mutation,
        })
    }

    fn class(class: &TypeName) -> Self {
        let disposed = Expression::property(Expression::this(), Identifier::known("_disposed"));
        let message = format!("{class} has been disposed");
        let error = Expression::construct(
            "Error",
            [Expression::string(StringLiteral::new(&message))]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        Self {
            parameter: None,
            setup: vec![Statement::throw_when(disposed, error)],
            arguments: vec![Expression::property(
                Expression::this(),
                Identifier::known("_handle"),
            )],
            mutation: None,
        }
    }

    fn callback() -> Self {
        Self {
            parameter: None,
            setup: Vec::new(),
            arguments: vec![Expression::call(
                Expression::this(),
                Identifier::known("_borrowHandle"),
                ArgumentList::default(),
            )],
            mutation: None,
        }
    }
}

impl ReceiverMutation {
    fn new(
        ty: TypeName,
        read: &boltffi_binding::ReadPlan,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let descriptor = Identifier::known("__boltffiReceiverOut");
        let reader = Identifier::known("__boltffiReceiverReader");
        Ok(Self::Encoded {
            ty,
            descriptor,
            decode: read
                .render_with(&mut Reader::new(reader.clone(), context))?
                .into_expression(),
            reader,
        })
    }

    fn ty(self) -> TypeName {
        match self {
            Self::Direct { ty, .. } | Self::Encoded { ty, .. } => ty,
        }
    }

    fn prepare(&self, parameter: &mut Parameter) {
        let Self::Encoded {
            descriptor: descriptor_name,
            ..
        } = self
        else {
            return;
        };
        let descriptor = Expression::identifier(descriptor_name.clone());
        parameter.setup.push(Statement::constant(
            descriptor_name.clone(),
            Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("allocBufDescriptor"),
                ArgumentList::default(),
            ),
        ));
        parameter.arguments.push(descriptor.clone());
        parameter
            .cleanup
            .push(Statement::expression(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("freeBuf"),
                [descriptor].into_iter().collect::<ArgumentList>(),
            )));
    }

    fn render(
        &self,
        call: Expression,
        returns: &Return,
        failure: &Failure,
    ) -> Result<Vec<Statement>> {
        if !matches!(returns.conversion, ReturnConversion::Void)
            || !matches!(failure, Failure::None)
        {
            return Err(Function::unsupported("mutable receiver with return value"));
        }
        let status = Statement::expression(Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("checkStatus"),
            [call].into_iter().collect::<ArgumentList>(),
        ));
        match self {
            Self::Direct { value, .. } => Ok(vec![
                status,
                Statement::return_value(Expression::identifier(value.clone())),
            ]),
            Self::Encoded {
                descriptor,
                reader,
                decode,
                ..
            } => Ok(vec![
                status,
                Statement::constant(
                    reader.clone(),
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("readerFromBuf"),
                        [Expression::identifier(descriptor.clone())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                ),
                Statement::return_value(decode.clone()),
            ]),
        }
    }
}

impl Failure {
    fn new(
        channel: ErrorChannel<'_, Wasm32, boltffi_binding::OutOfRust>,
        policy: FailurePolicy,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match channel {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ty,
                codec,
                shape: wasm32::BufferShape::Packed,
            } => {
                let reader = Identifier::known("__boltffiErrorReader");
                let decode = codec.render_with(&mut Reader::new(reader.clone(), context))?;
                let value = match decode.kind() {
                    Some(ReadKind::String) => FailureValue::String,
                    Some(ReadKind::Utf8String) => FailureValue::Utf8String,
                    Some(
                        ReadKind::Bytes
                        | ReadKind::Primitive(_)
                        | ReadKind::CustomPrimitive(_)
                        | ReadKind::OptionalPrimitive(_)
                        | ReadKind::ErrorRecord(_)
                        | ReadKind::ErrorEnum(_),
                    )
                    | None => FailureValue::Encoded {
                        reader,
                        decode: decode.into_expression(),
                    },
                };
                let action = match policy {
                    FailurePolicy::ReturnNull => FailureAction::ReturnNull,
                    FailurePolicy::Throw => FailureAction::Throw(match ty {
                        TypeRef::String => Exception::String,
                        TypeRef::Record(id) => context
                            .record(*id)
                            .map(|record| {
                                Exception::Typed(TypeName::named(format!(
                                    "{}Exception",
                                    Name::new(record.name()).type_name()
                                )))
                            })
                            .ok_or_else(|| {
                                Function::unsupported("error record without declaration")
                            })?,
                        TypeRef::Enum(id) => context
                            .enumeration(*id)
                            .map(|enumeration| {
                                Exception::Typed(TypeName::named(format!(
                                    "{}Exception",
                                    Name::new(enumeration.name()).type_name()
                                )))
                            })
                            .ok_or_else(|| {
                                Function::unsupported("error enum without declaration")
                            })?,
                        _ => return Err(Function::unsupported("error payload type")),
                    }),
                };
                Ok(Self::Encoded { value, action })
            }
            ErrorChannel::Status => Err(Function::unsupported("status error channel")),
            ErrorChannel::Encoded { .. } => Err(Function::unsupported("encoded error placement")),
            _ => Err(Function::unsupported("unknown error channel")),
        }
    }

    fn render(&self, call: Expression, returns: &Return) -> Result<Vec<Statement>> {
        match self {
            Self::None => Ok(returns.render(call)),
            Self::Encoded { value, action } => {
                let error = Identifier::known("__boltffiError");
                let error_value = Expression::identifier(error.clone());
                let (mut failure, value) = value.render(error_value.clone());
                match action {
                    FailureAction::Throw(exception) => {
                        failure.push(Statement::throwing(Expression::construct(
                            match exception {
                                Exception::String => TypeName::named("Error"),
                                Exception::Typed(exception) => exception.clone(),
                            },
                            [value].into_iter().collect::<ArgumentList>(),
                        )));
                    }
                    FailureAction::ReturnNull => {
                        failure.push(Statement::expression(value));
                        failure.push(Statement::return_value(Expression::null()));
                    }
                }
                Ok(
                    std::iter::once(Statement::constant(error, call.cast(TypeName::bigint())))
                        .chain(std::iter::once(Statement::when(
                            error_value.strict_not_equal(Expression::bigint(0)),
                            failure,
                        )))
                        .chain(returns.render_success()?)
                        .collect(),
                )
            }
        }
    }

    fn returns_null(&self) -> bool {
        matches!(
            self,
            Self::Encoded {
                action: FailureAction::ReturnNull,
                ..
            }
        )
    }
}

impl FailureValue {
    fn render(&self, error: Expression) -> (Vec<Statement>, Expression) {
        match self {
            Self::String => (
                Vec::new(),
                Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    Identifier::known("takePackedWireString"),
                    [error].into_iter().collect::<ArgumentList>(),
                ),
            ),
            Self::Utf8String => (
                Vec::new(),
                Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    Identifier::known("takePackedUtf8String"),
                    [error].into_iter().collect::<ArgumentList>(),
                ),
            ),
            Self::Encoded { reader, decode } => (
                vec![Statement::constant(
                    reader.clone(),
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("takePackedBuffer"),
                        [error].into_iter().collect::<ArgumentList>(),
                    ),
                )],
                decode.clone(),
            ),
        }
    }
}

impl Parameter {
    fn closure(
        name: Identifier,
        closure: &boltffi_binding::ClosureParameter<Wasm32, IntoRust>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let adapter =
            ClosureAdapter::from_closure(WasmIncomingClosure::Parameter(closure), context)?
                .ok_or_else(|| Function::unsupported("closure parameter"))?;
        let handle = Identifier::parse(format!("__boltffi_{name}_handle"))?;
        Ok(Self {
            ty: adapter.parameter_type(),
            setup: vec![Statement::constant(
                handle.clone(),
                Expression::invoke(
                    adapter.register(),
                    [Expression::identifier(name.clone())]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            )],
            arguments: vec![Expression::identifier(handle)],
            cleanup: Vec::new(),
            name,
        })
    }

    fn receiver(
        owner: ReceiverOwner,
        receive: Receive,
        context: &RenderContext<Wasm32>,
    ) -> Result<(Self, Option<ReceiverMutation>)> {
        let name = Identifier::known("self");
        match owner {
            ReceiverOwner::Record(id) => match context.record(id) {
                Some(RecordDecl::Direct(record)) => {
                    let mutation =
                        matches!(receive, Receive::ByMutRef).then(|| ReceiverMutation::Direct {
                            ty: Name::new(record.name()).type_name(),
                            value: name.clone(),
                        });
                    Self::direct_record(name, id, receive, context)
                        .map(|parameter| (parameter, mutation))
                }
                Some(RecordDecl::Encoded(record)) => Self::encoded_receiver(
                    name,
                    Name::new(record.name()).type_name(),
                    record.read(),
                    record.write(),
                    receive,
                    context,
                ),
                _ => Err(Function::unsupported("record without declaration")),
            },
            ReceiverOwner::CStyleEnum(id) | ReceiverOwner::DataEnum(id) => {
                match context.enumeration(id) {
                    Some(EnumDecl::CStyle(_)) if matches!(receive, Receive::ByMutRef) => {
                        Err(Function::unsupported("mutable direct enum receiver"))
                    }
                    Some(EnumDecl::CStyle(_)) => {
                        Self::direct_enum(name, id, context).map(|parameter| (parameter, None))
                    }
                    Some(EnumDecl::Data(enumeration)) => Self::encoded_receiver(
                        name,
                        Name::new(enumeration.name()).type_name(),
                        enumeration.read(),
                        enumeration.write(),
                        receive,
                        context,
                    ),
                    _ => Err(Function::unsupported("enum without declaration")),
                }
            }
        }
    }

    fn encoded_receiver(
        name: Identifier,
        ty: TypeName,
        read: &boltffi_binding::ReadPlan,
        write: &boltffi_binding::WritePlan,
        receive: Receive,
        context: &RenderContext<Wasm32>,
    ) -> Result<(Self, Option<ReceiverMutation>)> {
        let mut parameter = Self::encoded_type(name, ty.clone(), write, context)?;
        let mutation = match receive {
            Receive::ByMutRef => {
                let mutation = ReceiverMutation::new(ty, read, context)?;
                mutation.prepare(&mut parameter);
                Some(mutation)
            }
            _ => None,
        };
        Ok((parameter, mutation))
    }

    fn direct(name: Identifier, primitive: Primitive) -> Result<Self> {
        Ok(Self {
            ty: Type::primitive(primitive)?,
            arguments: vec![Expression::identifier(name.clone())],
            name,
            setup: Vec::new(),
            cleanup: Vec::new(),
        })
    }

    fn direct_enum(
        name: Identifier,
        id: boltffi_binding::EnumId,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let ty = context
            .enumeration(id)
            .map(|enumeration| TypeName::named(Name::new(enumeration.name()).type_name()))
            .ok_or_else(|| Function::unsupported("enum without declaration"))?;
        Ok(Self {
            ty,
            arguments: vec![Expression::identifier(name.clone())],
            name,
            setup: Vec::new(),
            cleanup: Vec::new(),
        })
    }

    fn encoded(
        name: Identifier,
        ty: &TypeRef,
        codec: &boltffi_binding::WritePlan,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Self::encoded_type(name, Type::from_ref(ty, context)?, codec, context)
    }

    fn encoded_type(
        name: Identifier,
        ty: TypeName,
        codec: &boltffi_binding::WritePlan,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let value = Expression::identifier(name.clone());
        let size = codec.size_with(&mut Sizer::new(value.clone(), context))?;
        let writer = Identifier::parse(format!("__boltffi_{name}_writer"))?;
        let writes = codec
            .render_with(&mut Writer::new(writer.clone(), value.clone(), context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        if matches!(size.kind(), Some(SizeKind::Utf8String)) && writes.is_empty() {
            let allocation = Identifier::parse(format!("__boltffi_{name}_allocation"))?;
            let allocation_value = Expression::identifier(allocation.clone());
            return Ok(Self {
                ty,
                setup: vec![Statement::constant(
                    allocation,
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("allocOwnedString"),
                        [value].into_iter().collect::<ArgumentList>(),
                    ),
                )],
                arguments: ["ptr", "len"]
                    .into_iter()
                    .map(|property| {
                        Expression::property(allocation_value.clone(), Identifier::known(property))
                    })
                    .collect(),
                cleanup: Vec::new(),
                name,
            });
        }
        let allocation_method = match (size.kind(), writes.as_slice()) {
            (Some(SizeKind::String), [write])
                if matches!(write.kind(), Some(WriteKind::String)) =>
            {
                Some(Identifier::known("allocWireString"))
            }
            (Some(SizeKind::Bytes), [write]) if matches!(write.kind(), Some(WriteKind::Bytes)) => {
                Some(Identifier::known("allocWireBytes"))
            }
            _ => None,
        };
        let size = size.into_expression();
        let writes = writes
            .into_iter()
            .map(|write| write.into_statement())
            .collect::<Vec<_>>();
        let Some(allocation_method) = allocation_method else {
            let writer_value = Expression::identifier(writer.clone());
            return Ok(Self {
                ty,
                setup: std::iter::once(Statement::constant(
                    writer.clone(),
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("allocWriter"),
                        [size].into_iter().collect::<ArgumentList>(),
                    ),
                ))
                .chain(writes)
                .collect(),
                arguments: ["ptr", "len"]
                    .into_iter()
                    .map(|property| {
                        Expression::property(writer_value.clone(), Identifier::known(property))
                    })
                    .collect(),
                cleanup: vec![Statement::expression(Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    Identifier::known("freeWriter"),
                    [writer_value].into_iter().collect::<ArgumentList>(),
                ))],
                name,
            });
        };
        let allocation = Identifier::parse(format!("__boltffi_{name}_allocation"))?;
        let allocation_value = Expression::identifier(allocation.clone());
        Ok(Self {
            ty,
            setup: vec![Statement::constant(
                allocation.clone(),
                Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    allocation_method,
                    [value].into_iter().collect::<ArgumentList>(),
                ),
            )],
            arguments: ["ptr", "len"]
                .into_iter()
                .map(|property| {
                    Expression::property(allocation_value.clone(), Identifier::known(property))
                })
                .collect(),
            cleanup: vec![Statement::expression(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("freeAlloc"),
                [allocation_value].into_iter().collect::<ArgumentList>(),
            ))],
            name,
        })
    }

    fn direct_vector(
        name: Identifier,
        element: &DirectVectorElementType,
        receive: Receive,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let vector = DirectVector::new(element, receive, context)?;
        let allocation = Identifier::parse(format!("__boltffi_{name}_allocation"))?;
        let allocation_value = Expression::identifier(allocation.clone());
        let value = Expression::identifier(name.clone());
        let mut cleanup = match vector.writeback() {
            true => vec![Statement::expression(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("copyPrimitiveBufferInto"),
                [
                    allocation_value.clone(),
                    value.clone(),
                    Expression::string(vector.element_literal()),
                ]
                .into_iter()
                .collect::<ArgumentList>(),
            ))],
            false => Vec::new(),
        };
        cleanup.push(Statement::expression(Expression::call(
            Expression::identifier(Identifier::known("_module")),
            vector.free_method(),
            [allocation_value.clone()]
                .into_iter()
                .collect::<ArgumentList>(),
        )));
        Ok(Self {
            ty: vector.parameter_type()?,
            setup: vec![Statement::constant(
                allocation.clone(),
                vector.allocation(value),
            )],
            arguments: ["ptr", "len"]
                .into_iter()
                .map(|property| {
                    Expression::property(allocation_value.clone(), Identifier::known(property))
                })
                .collect(),
            cleanup,
            name,
        })
    }

    fn scalar_option(name: Identifier, primitive: Primitive) -> Result<Self> {
        let option = ScalarOption::new(primitive)?;
        Ok(Self {
            ty: option.ty()?,
            arguments: vec![option.argument(Expression::identifier(name.clone()))],
            name,
            setup: Vec::new(),
            cleanup: Vec::new(),
        })
    }

    fn direct_record(
        name: Identifier,
        id: boltffi_binding::RecordId,
        receive: Receive,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let record = context
            .record(id)
            .ok_or_else(|| Function::unsupported("record without declaration"))?;
        let codec = Name::new(record.name()).codec_identifier()?;
        let writer = Identifier::parse(format!("__boltffi_{name}_writer"))?;
        let writer_value = Expression::identifier(writer.clone());
        let value = Expression::identifier(name.clone());
        let mut cleanup = matches!(receive, Receive::ByMutRef)
            .then(|| {
                Statement::expression(Expression::static_call(
                    "Object",
                    Identifier::known("assign"),
                    [
                        value.clone(),
                        Expression::call(
                            Expression::identifier(codec.clone()),
                            Identifier::known("decode"),
                            [Expression::call(
                                Expression::identifier(Identifier::known("_module")),
                                Identifier::known("readerFromWriter"),
                                [writer_value.clone()].into_iter().collect(),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ))
            })
            .into_iter()
            .collect::<Vec<_>>();
        cleanup.push(Statement::expression(Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("freeWriter"),
            [writer_value.clone()].into_iter().collect::<ArgumentList>(),
        )));
        Ok(Self {
            ty: Name::new(record.name()).type_name(),
            setup: vec![
                Statement::constant(
                    writer.clone(),
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("allocWriter"),
                        [Expression::call(
                            Expression::identifier(codec.clone()),
                            Identifier::known("size"),
                            [value.clone()].into_iter().collect::<ArgumentList>(),
                        )]
                        .into_iter()
                        .collect::<ArgumentList>(),
                    ),
                ),
                Statement::expression(Expression::call(
                    Expression::identifier(codec),
                    Identifier::known("encode"),
                    [writer_value.clone(), value]
                        .into_iter()
                        .collect::<ArgumentList>(),
                )),
            ],
            arguments: vec![Expression::property(
                writer_value.clone(),
                Identifier::known("ptr"),
            )],
            cleanup,
            name,
        })
    }

    fn class_handle(
        name: Identifier,
        id: ClassId,
        presence: HandlePresence,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let class = context
            .class(id)
            .map(|class| Name::new(class.name()).type_name())
            .ok_or_else(|| Function::unsupported("class without declaration"))?;
        let ty = match presence {
            HandlePresence::Required => class.clone(),
            HandlePresence::Nullable => class.clone().nullable(),
            _ => return Err(Function::unsupported("unknown class handle presence")),
        };
        Ok(Self {
            ty,
            arguments: vec![Expression::static_call(
                class,
                Identifier::known("_toHandle"),
                [Expression::identifier(name.clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )],
            name,
            setup: Vec::new(),
            cleanup: Vec::new(),
        })
    }

    fn callback_handle(
        name: Identifier,
        id: CallbackId,
        presence: HandlePresence,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let callback = context
            .callback(id)
            .ok_or_else(|| Function::unsupported("callback without declaration"))?;
        let callback_type = Name::new(callback.name()).type_name();
        let register = Identifier::parse(format!("register{callback_type}"))?;
        let value = Expression::identifier(name.clone());
        let handle = Identifier::parse(format!("__boltffi_{name}_handle"))?;
        let registered = Expression::invoke(
            register,
            [value.clone()].into_iter().collect::<ArgumentList>(),
        );
        let (ty, registered) = match presence {
            HandlePresence::Required => (callback_type, registered),
            HandlePresence::Nullable => (
                callback_type.nullable(),
                value
                    .strict_equal(Expression::null())
                    .conditional(Expression::integer(0), registered),
            ),
            _ => return Err(Function::unsupported("unknown callback handle presence")),
        };
        Ok(Self {
            name,
            ty,
            setup: vec![Statement::constant(handle.clone(), registered)],
            arguments: vec![Expression::identifier(handle)],
            cleanup: Vec::new(),
        })
    }
}

impl Return {
    fn new(ty: TypeName, conversion: ReturnConversion) -> Self {
        Self {
            ty,
            conversion,
            setup: Vec::new(),
            arguments: Vec::new(),
            cleanup: Vec::new(),
            success: Vec::new(),
        }
    }

    fn direct_record(
        id: boltffi_binding::RecordId,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let record = context
            .record(id)
            .ok_or_else(|| Function::unsupported("record without declaration"))?;
        let size = match record {
            boltffi_binding::RecordDecl::Direct(record) => record.layout().size().get(),
            _ => return Err(Function::unsupported("encoded record direct return")),
        };
        let codec = Name::new(record.name()).codec_identifier()?;
        let writer = Identifier::known("__boltffiReturnWriter");
        let writer_value = Expression::identifier(writer.clone());
        let success = vec![Statement::return_value(Expression::call(
            Expression::identifier(codec.clone()),
            Identifier::known("decode"),
            [Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("readerFromWriter"),
                [writer_value.clone()].into_iter().collect::<ArgumentList>(),
            )]
            .into_iter()
            .collect::<ArgumentList>(),
        ))];
        Ok(Self {
            ty: Name::new(record.name()).type_name(),
            conversion: ReturnConversion::DirectRecord {
                writer: writer.clone(),
                codec: codec.clone(),
            },
            setup: vec![Statement::constant(
                writer.clone(),
                Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    Identifier::known("allocWriter"),
                    [Expression::integer(size)]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            )],
            arguments: vec![Expression::property(
                writer_value.clone(),
                Identifier::known("ptr"),
            )],
            cleanup: vec![Statement::expression(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("freeWriter"),
                [writer_value].into_iter().collect::<ArgumentList>(),
            ))],
            success,
        })
    }

    fn out(ty: TypeName, size: u64, value: impl FnOnce(Expression) -> Vec<Statement>) -> Self {
        let writer = Identifier::known("__boltffiReturnWriter");
        let writer_value = Expression::identifier(writer.clone());
        let reader = Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("readerFromWriter"),
            [writer_value.clone()].into_iter().collect::<ArgumentList>(),
        );
        Self {
            ty,
            conversion: ReturnConversion::Out,
            setup: vec![Statement::constant(
                writer.clone(),
                Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    Identifier::known("allocWriter"),
                    [Expression::integer(size)]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            )],
            arguments: vec![Expression::property(
                writer_value.clone(),
                Identifier::known("ptr"),
            )],
            cleanup: vec![Statement::expression(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("freeWriter"),
                [writer_value].into_iter().collect::<ArgumentList>(),
            ))],
            success: value(reader),
        }
    }

    fn render_async(
        &self,
        start: Expression,
        protocol: &wasm32::AsyncProtocol,
        failure: &Failure,
    ) -> Result<Vec<Statement>> {
        let wasm32::AsyncProtocol::PollHandle {
            handle,
            poll_sync,
            complete,
            free,
            panic_message,
            ..
        } = protocol
        else {
            return Err(Function::unsupported("unknown asynchronous protocol"));
        };
        if !matches!(handle, wasm32::HandleCarrier::U32) {
            return Err(Function::unsupported("unknown asynchronous handle carrier"));
        }
        let future = Identifier::known("__boltffiFuture");
        let awaited = Identifier::known("__boltffiAwaitedFuture");
        let callback_handle = Identifier::known("__boltffiHandle");
        let callback_value = Expression::identifier(callback_handle.clone());
        let lifecycle = [poll_sync, panic_message, free]
            .into_iter()
            .map(|symbol| {
                Ok(Expression::parameter_lambda(
                    callback_handle.clone(),
                    Expression::native_call(
                        Identifier::parse(symbol.name().as_str())?,
                        [callback_value.clone()]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let poll = Expression::call(
            Expression::property(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("asyncManager"),
            ),
            Identifier::known("pollAsync"),
            std::iter::once(Expression::identifier(future.clone()))
                .chain(lifecycle)
                .collect::<ArgumentList>(),
        )
        .await_value();
        let complete = Identifier::parse(complete.name().as_str())?;
        let completion = self.render_async_completion(
            complete,
            Expression::identifier(awaited.clone()),
            failure,
        )?;
        let release = Statement::expression(Expression::native_call(
            Identifier::parse(free.name().as_str())?,
            [Expression::identifier(awaited.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        ));
        Ok(vec![
            Statement::constant(future, start),
            Statement::constant(awaited, poll),
            Statement::try_finally(completion, vec![release]),
        ])
    }

    fn render_async_completion(
        &self,
        complete: Identifier,
        awaited: Expression,
        failure: &Failure,
    ) -> Result<Vec<Statement>> {
        let status = Identifier::known("__boltffiStatus");
        let complete_call = Expression::native_call(
            complete,
            [awaited, Expression::identifier(status.clone())]
                .into_iter()
                .chain(self.arguments.iter().cloned())
                .collect::<ArgumentList>(),
        );
        let complete_call = Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("completeAsync"),
            [Expression::parameter_lambda(status, complete_call)]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        Ok(self
            .setup
            .iter()
            .cloned()
            .chain(match self.cleanup.is_empty() {
                true => failure.render(complete_call, self)?,
                false => vec![Statement::try_finally(
                    failure.render(complete_call, self)?,
                    self.cleanup.clone(),
                )],
            })
            .collect())
    }

    fn render(&self, call: Expression) -> Vec<Statement> {
        match &self.conversion {
            ReturnConversion::Void => vec![Statement::expression(call)],
            ReturnConversion::Direct => vec![Statement::return_value(call)],
            ReturnConversion::Boolean => vec![Statement::return_value(call.not_zero())],
            ReturnConversion::String => vec![Statement::return_value(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("takePackedWireString"),
                [call.cast(TypeName::bigint())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))],
            ReturnConversion::Utf8String => vec![Statement::return_value(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("takePackedUtf8String"),
                [call.cast(TypeName::bigint())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))],
            ReturnConversion::Bytes => vec![Statement::return_value(Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("takePackedWireBytes"),
                [call.cast(TypeName::bigint())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))],
            ReturnConversion::Encoded { reader, decode } => vec![
                Statement::constant(
                    reader.clone(),
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("takePackedBuffer"),
                        [call.cast(TypeName::bigint())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                ),
                Statement::return_value(decode.clone()),
            ],
            ReturnConversion::PackedOptional { take } => {
                vec![Statement::return_value(Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    take.clone(),
                    [call.cast(TypeName::bigint())]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ))]
            }
            ReturnConversion::DirectVector { take } => vec![
                Statement::expression(call),
                Statement::return_value(take.clone()),
            ],
            ReturnConversion::ScalarOption { unpack } => {
                vec![Statement::return_value(Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    unpack.clone(),
                    [call].into_iter().collect::<ArgumentList>(),
                ))]
            }
            ReturnConversion::DirectRecord { writer, codec } => vec![
                Statement::expression(call),
                Statement::return_value(Expression::call(
                    Expression::identifier(codec.clone()),
                    Identifier::known("decode"),
                    [Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("readerFromWriter"),
                        [Expression::identifier(writer.clone())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    )]
                    .into_iter()
                    .collect::<ArgumentList>(),
                )),
            ],
            ReturnConversion::ClassHandle { class, nullable } => match nullable {
                false => vec![Statement::return_value(Expression::static_call(
                    class,
                    Identifier::known("_fromHandle"),
                    [call].into_iter().collect::<ArgumentList>(),
                ))],
                true => {
                    let handle = Identifier::known("__boltffiHandle");
                    let value = Expression::identifier(handle.clone());
                    vec![
                        Statement::constant(handle, call),
                        Statement::return_value(
                            value
                                .clone()
                                .strict_equal(Expression::integer(0))
                                .conditional(
                                    Expression::null(),
                                    Expression::static_call(
                                        class,
                                        Identifier::known("_fromHandle"),
                                        [value].into_iter().collect::<ArgumentList>(),
                                    ),
                                ),
                        ),
                    ]
                }
            },
            ReturnConversion::CallbackHandle { wrap, nullable } => match nullable {
                false => vec![Statement::return_value(Expression::invoke(
                    wrap.clone(),
                    [call].into_iter().collect::<ArgumentList>(),
                ))],
                true => {
                    let handle = Identifier::known("__boltffiHandle");
                    let value = Expression::identifier(handle.clone());
                    vec![
                        Statement::constant(handle, call),
                        Statement::return_value(
                            value
                                .clone()
                                .strict_equal(Expression::integer(0))
                                .conditional(
                                    Expression::null(),
                                    Expression::invoke(
                                        wrap.clone(),
                                        [value].into_iter().collect::<ArgumentList>(),
                                    ),
                                ),
                        ),
                    ]
                }
            },
            ReturnConversion::Out => std::iter::once(Statement::expression(call))
                .chain(self.success.iter().cloned())
                .collect(),
        }
    }

    fn render_success(&self) -> Result<Vec<Statement>> {
        match self.conversion {
            ReturnConversion::Void => Ok(Vec::new()),
            ReturnConversion::DirectRecord { .. } | ReturnConversion::Out => {
                Ok(self.success.clone())
            }
            _ => Err(Function::unsupported("fallible success return placement")),
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Wasm32, IntoRust> for ParameterRenderer<'_> {
    type Output = Result<Parameter>;

    fn direct(&mut self, ty: &'plan DirectValueType, receive: Receive) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => {
                Parameter::direct(self.name.clone(), *primitive)
            }
            DirectValueType::Enum(id) => {
                Parameter::direct_enum(self.name.clone(), *id, self.context)
            }
            DirectValueType::Record(id) => {
                Parameter::direct_record(self.name.clone(), *id, receive, self.context)
            }
            _ => Err(Function::unsupported("unknown direct parameter")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan boltffi_binding::WritePlan,
        shape: wasm32::BufferShape,
        _receive: Receive,
    ) -> Self::Output {
        match shape {
            wasm32::BufferShape::Slice => {
                Parameter::encoded(self.name.clone(), ty, codec, self.context)
            }
            wasm32::BufferShape::Packed => Err(Function::unsupported("packed encoded parameter")),
            _ => Err(Function::unsupported("unknown encoded parameter shape")),
        }
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: wasm32::HandleCarrier,
        presence: HandlePresence,
        _receive: Receive,
    ) -> Self::Output {
        if !matches!(carrier, wasm32::HandleCarrier::U32) {
            return Err(Function::unsupported("unknown handle carrier"));
        }
        match target {
            HandleTarget::Class(id) => {
                Parameter::class_handle(self.name.clone(), *id, presence, self.context)
            }
            HandleTarget::Callback(id) => {
                Parameter::callback_handle(self.name.clone(), *id, presence, self.context)
            }
            _ => Err(Function::unsupported("handle parameter")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Parameter::scalar_option(self.name.clone(), primitive)
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        receive: Receive,
    ) -> Self::Output {
        Parameter::direct_vector(self.name.clone(), element, receive, self.context)
    }
}

impl<'plan> ReturnPlanRender<'plan, Wasm32, boltffi_binding::OutOfRust> for ReturnRenderer<'_> {
    type Output = Result<Return>;

    fn void(&mut self) -> Self::Output {
        Ok(Return::new(TypeName::void(), ReturnConversion::Void))
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match (slot, ty) {
            (ReturnValueSlot::ReturnSlot, DirectValueType::Primitive(primitive)) => {
                Ok(Return::new(
                    Type::primitive(*primitive)?,
                    match primitive {
                        Primitive::Bool => ReturnConversion::Boolean,
                        _ => ReturnConversion::Direct,
                    },
                ))
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Enum(id)) => Ok(Return::new(
                self.context
                    .enumeration(*id)
                    .map(|enumeration| Name::new(enumeration.name()).type_name())
                    .ok_or_else(|| Function::unsupported("enum without declaration"))?,
                ReturnConversion::Direct,
            )),
            (ReturnValueSlot::OutPointer, DirectValueType::Primitive(primitive)) => {
                let scalar = Scalar::new(*primitive)?;
                let read = scalar.read_method();
                Ok(Return::out(
                    scalar.ty(),
                    primitive.byte_size::<Wasm32>().get(),
                    move |reader| {
                        let value = Expression::call(reader, read, ArgumentList::default());
                        vec![Statement::return_value(value)]
                    },
                ))
            }
            (ReturnValueSlot::OutPointer, DirectValueType::Enum(id)) => {
                let enumeration = self
                    .context
                    .enumeration(*id)
                    .ok_or_else(|| Function::unsupported("enum without declaration"))?;
                let EnumDecl::CStyle(enumeration) = enumeration else {
                    return Err(Function::unsupported("direct data enum return"));
                };
                let primitive = enumeration.repr().primitive();
                let scalar = Scalar::new(primitive)?;
                let read = scalar.read_method();
                Ok(Return::out(
                    Name::new(enumeration.name()).type_name(),
                    primitive.byte_size::<Wasm32>().get(),
                    |reader| {
                        vec![Statement::return_value(Expression::call(
                            reader,
                            read,
                            ArgumentList::default(),
                        ))]
                    },
                ))
            }
            (ReturnValueSlot::OutPointer, DirectValueType::Record(id)) => {
                Return::direct_record(*id, self.context)
            }
            (ReturnValueSlot::OutPointer, _) => {
                Err(Function::unsupported("direct out-pointer return"))
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Record(_)) => {
                Err(Function::unsupported("direct record return slot"))
            }
            _ => Err(Function::unsupported("unknown direct return")),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan boltffi_binding::ReadPlan,
        shape: wasm32::BufferShape,
    ) -> Self::Output {
        if !matches!(shape, wasm32::BufferShape::Packed) {
            return Err(Function::unsupported("encoded return placement"));
        }
        let reader = Identifier::known("__boltffiReader");
        let read = codec.render_with(&mut Reader::new(reader.clone(), self.context))?;
        let kind = read.kind();
        let decode = read.into_expression();
        match (slot, kind) {
            (ReturnValueSlot::ReturnSlot, Some(ReadKind::String)) => Ok(Return::new(
                Type::from_ref(ty, self.context)?,
                ReturnConversion::String,
            )),
            (ReturnValueSlot::ReturnSlot, Some(ReadKind::Utf8String)) => Ok(Return::new(
                Type::from_ref(ty, self.context)?,
                ReturnConversion::Utf8String,
            )),
            (ReturnValueSlot::ReturnSlot, Some(ReadKind::Bytes)) => Ok(Return::new(
                Type::from_ref(ty, self.context)?,
                ReturnConversion::Bytes,
            )),
            (ReturnValueSlot::ReturnSlot, Some(ReadKind::OptionalPrimitive(primitive))) => {
                Ok(Return::new(
                    Type::from_ref(ty, self.context)?,
                    ReturnConversion::PackedOptional {
                        take: Scalar::new(primitive)?.take_optional_method(),
                    },
                ))
            }
            (
                ReturnValueSlot::ReturnSlot,
                Some(ReadKind::Primitive(_) | ReadKind::CustomPrimitive(_)) | None,
            ) => Ok(Return::new(
                Type::from_ref(ty, self.context)?,
                ReturnConversion::Encoded { reader, decode },
            )),
            (ReturnValueSlot::OutPointer, Some(ReadKind::OptionalPrimitive(primitive))) => {
                let ty = Type::from_ref(ty, self.context)?;
                let take = Scalar::new(primitive)?.take_optional_method();
                Ok(Return::out(ty, 8, move |output| {
                    let packed = Expression::call(
                        output,
                        Identifier::known("readU64"),
                        ArgumentList::default(),
                    );
                    vec![Statement::return_value(Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        take,
                        [packed].into_iter().collect::<ArgumentList>(),
                    ))]
                }))
            }
            (ReturnValueSlot::OutPointer, kind) => {
                let ty = Type::from_ref(ty, self.context)?;
                Ok(Return::out(ty, 8, move |output| {
                    let packed = Expression::call(
                        output,
                        Identifier::known("readU64"),
                        ArgumentList::default(),
                    );
                    match kind {
                        Some(ReadKind::String) => vec![Statement::return_value(Expression::call(
                            Expression::identifier(Identifier::known("_module")),
                            Identifier::known("takePackedWireString"),
                            [packed].into_iter().collect::<ArgumentList>(),
                        ))],
                        Some(ReadKind::Utf8String) => {
                            vec![Statement::return_value(Expression::call(
                                Expression::identifier(Identifier::known("_module")),
                                Identifier::known("takePackedUtf8String"),
                                [packed].into_iter().collect::<ArgumentList>(),
                            ))]
                        }
                        Some(ReadKind::Bytes) => vec![Statement::return_value(Expression::call(
                            Expression::identifier(Identifier::known("_module")),
                            Identifier::known("takePackedWireBytes"),
                            [packed].into_iter().collect::<ArgumentList>(),
                        ))],
                        Some(
                            ReadKind::Primitive(_)
                            | ReadKind::CustomPrimitive(_)
                            | ReadKind::OptionalPrimitive(_)
                            | ReadKind::ErrorRecord(_)
                            | ReadKind::ErrorEnum(_),
                        )
                        | None => vec![
                            Statement::constant(
                                reader,
                                Expression::call(
                                    Expression::identifier(Identifier::known("_module")),
                                    Identifier::known("takePackedBuffer"),
                                    [packed].into_iter().collect::<ArgumentList>(),
                                ),
                            ),
                            Statement::return_value(decode),
                        ],
                    }
                }))
            }
            _ => Err(Function::unsupported("unknown encoded return placement")),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: wasm32::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        if !matches!(carrier, wasm32::HandleCarrier::U32) {
            return Err(Function::unsupported("handle return placement"));
        }
        let nullable = match presence {
            HandlePresence::Required => false,
            HandlePresence::Nullable => true,
            _ => return Err(Function::unsupported("unknown handle presence")),
        };
        let (ty, conversion) = match target {
            HandleTarget::Class(id) => {
                let class = self
                    .context
                    .class(*id)
                    .map(|class| Name::new(class.name()).type_name())
                    .ok_or_else(|| Function::unsupported("class without declaration"))?;
                let ty = match nullable {
                    true => class.clone().nullable(),
                    false => class.clone(),
                };
                (ty, ReturnConversion::ClassHandle { class, nullable })
            }
            HandleTarget::Callback(id) => {
                let callback = self
                    .context
                    .callback(*id)
                    .map(|callback| Name::new(callback.name()).type_name())
                    .ok_or_else(|| Function::unsupported("callback without declaration"))?;
                let ty = match nullable {
                    true => callback.clone().nullable(),
                    false => callback.clone(),
                };
                (
                    ty,
                    ReturnConversion::CallbackHandle {
                        wrap: Identifier::parse(format!("wrap{callback}"))?,
                        nullable,
                    },
                )
            }
            _ => return Err(Function::unsupported("handle return")),
        };
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Return::new(ty, conversion)),
            ReturnValueSlot::OutPointer if matches!(target, HandleTarget::Class(_)) => {
                let HandleTarget::Class(id) = target else {
                    unreachable!()
                };
                let class = self
                    .context
                    .class(*id)
                    .map(|class| Name::new(class.name()).type_name())
                    .ok_or_else(|| Function::unsupported("class without declaration"))?;
                Ok(Return::out(ty, 4, move |reader| {
                    let handle = Identifier::known("__boltffiReturnHandle");
                    let handle_value = Expression::identifier(handle.clone());
                    let wrapped = Expression::static_call(
                        class,
                        Identifier::known("_fromHandle"),
                        [handle_value.clone()].into_iter().collect::<ArgumentList>(),
                    );
                    vec![
                        Statement::constant(
                            handle,
                            Expression::call(
                                reader,
                                Identifier::known("readU32"),
                                ArgumentList::default(),
                            ),
                        ),
                        Statement::return_value(match nullable {
                            true => handle_value
                                .strict_equal(Expression::integer(0))
                                .conditional(Expression::null(), wrapped),
                            false => wrapped,
                        }),
                    ]
                }))
            }
            ReturnValueSlot::OutPointer => {
                Err(Function::unsupported("callback handle out-pointer return"))
            }
            _ => Err(Function::unsupported("unknown handle return placement")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let option = ScalarOption::new(primitive)?;
        Ok(Return::new(
            option.ty()?,
            ReturnConversion::ScalarOption {
                unpack: option.return_unpack_method(),
            },
        ))
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let vector = DirectVector::new(element, Receive::ByValue, self.context)?;
        Ok(Return::new(
            vector.return_type(),
            ReturnConversion::DirectVector {
                take: vector.take(),
            },
        ))
    }

    fn closure(
        &mut self,
        _closure: &'plan boltffi_binding::ClosureReturn<Wasm32, boltffi_binding::OutOfRust>,
    ) -> Self::Output {
        Err(Function::unsupported("closure return"))
    }
}
