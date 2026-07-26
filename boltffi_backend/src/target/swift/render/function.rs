use askama::Template;

use boltffi_binding::{
    CanonicalName, ClassId, ClosureReturn, DirectValueType, DirectVectorElementType, Direction,
    EnumId, ErrorChannel, ErrorPlacement, ExecutionDecl, ExportedCallable, ExportedMethodDecl,
    FunctionDecl, HandlePresence, HandleTarget, IncomingParam, InitializerDecl, IntoRust, Native,
    NativeSymbol, OutOfRust, ParamDecl, ParamPlanRender, Primitive, ReadPlan, Receive, RecordId,
    ReturnPlanRender, ReturnValueSlot, Surface, TypeRef, WritePlan, native,
};

use crate::{
    bridge::c::{
        CBridgeContract, ClosureReturnParameter, Function as CFunction, ParameterGroup,
        ReturnChannel,
    },
    core::{
        AuxChunk, Diagnostic, Emitted, Error, HelperId, RenderContext, Result, TextChunk,
        lexical::{LexicalPlan, LocalReference, Scope, with_lexical_plan},
    },
    target::swift::{
        SwiftHost,
        c_abi::{BorrowedVector, DirectValue, DirectVector, ReturnedVector},
        codec::{
            ArgumentBuffer, OwnedBuffer, ReadExpression, Reader, ScalarOption, WriteStatement,
            Writer,
        },
        lexical::ScopeForm,
        name_style::{GeneratedLocal, Name},
        primitive::SwiftPrimitive,
        render::callback::CallbackHandle,
        render::closure::ClosureArgument,
        render::{Documentation, SwiftType},
        syntax::{
            ArgumentList, Expression, Identifier, Literal, ParameterList, Statement, Syntax,
            TypeName,
        },
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    documentation: Documentation,
    name: Identifier,
    parameters: Vec<Parameter>,
    body: String,
    returns: ReturnSignature,
    asynchronous: bool,
    requires_wire_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssociatedFunction {
    documentation: Documentation,
    is_static: bool,
    mutating: bool,
    name: Identifier,
    parameters: Vec<Parameter>,
    body: String,
    returns: ReturnSignature,
    asynchronous: bool,
    requires_wire_runtime: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AssociatedFunctions {
    functions: Vec<AssociatedFunction>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValueFunctions {
    initializers: Vec<Initializer>,
    functions: Vec<AssociatedFunction>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueType {
    Record(RecordId),
    Enum(EnumId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initializer {
    documentation: Documentation,
    signature: InitializerSignature,
    parameters: Vec<Parameter>,
    body: String,
    factory_return: TypeName,
    effect: InitializerEffect,
    requires_wire_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InitializerSignature {
    Default,
    NamedInit { label: Identifier },
    NamedFactory { name: Identifier },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitializerEffect {
    Plain,
    Throwing,
    Failable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConstructedType {
    ClassHandle,
    Record,
    Enum,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Initializers {
    initializers: Vec<Initializer>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receiver {
    kind: ReceiverKind,
    argument: Argument,
    mutable_argument: Option<Argument>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiverKind {
    Value,
    Handle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeName,
    argument: Argument,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Argument {
    Direct(Expression),
    Encoded(EncodedArgument),
    MutableEncoded(MutableEncodedArgument),
    DirectVector(BorrowedVector),
    Closure(Box<ClosureArgument>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedArgument {
    buffer: ArgumentBuffer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MutableEncodedArgument {
    target: Expression,
    input: ArgumentBuffer,
    output: OwnedBuffer,
    reader: Identifier,
    decode: Expression,
    free: Identifier,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BodyExit {
    ReturnValue,
    ThrowingReturnValue,
    ThrowingEffect,
    CompleteEffect,
}

enum ScopedInitializerValue<'plan> {
    Bound {
        source: String,
        reference: LocalReference<'plan, Syntax>,
    },
    Terminal(String),
}

impl<'plan> ScopedInitializerValue<'plan> {
    fn into_bound(self) -> Result<(String, LocalReference<'plan, Syntax>)> {
        match self {
            Self::Bound { source, reference } => Ok((source, reference)),
            Self::Terminal(_) => Err(Error::UnexpectedBindingShape {
                layer: "Swift lexical planner",
                shape: "scoped value initializer without a bound result",
            }),
        }
    }

    fn into_scope_body(
        self,
        lexical: &LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
        indent: &str,
    ) -> Result<String> {
        match self {
            Self::Bound { source, reference } => {
                let value =
                    lexical
                        .resolve(scope, &reference)
                        .ok_or(Error::UnexpectedBindingShape {
                            layer: "Swift lexical planner",
                            shape: "nested value initializer local escaped its scope",
                        })?;
                Ok([
                    source,
                    Statement::returns(Expression::identifier(value.clone())).indented(indent),
                ]
                .join("\n"))
            }
            Self::Terminal(source) => Ok(source),
        }
    }

    fn wrap(self, render: impl FnOnce(String) -> Result<String>) -> Result<Self> {
        match self {
            Self::Bound { source, reference } => Ok(Self::Bound {
                source: render(source)?,
                reference,
            }),
            Self::Terminal(source) => render(source).map(Self::Terminal),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Return {
    ty: Option<TypeName>,
    optional: bool,
    conversion: ReturnConversion,
    success: Option<SuccessSlot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReturnConversion {
    Direct,
    FromC(TypeName),
    Encoded(EncodedReturn),
    DirectVector {
        vector: ReturnedVector,
        free: Identifier,
    },
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
    Closure(Box<ReturnedClosure>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedReturn {
    buffer: OwnedBuffer,
    reader: Identifier,
    decode: Expression,
    free: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReturnedClosure {
    storage: TypeName,
    owner: TypeName,
    storage_binding: Identifier,
    owner_binding: Identifier,
    presence: HandlePresence,
    parameters: Vec<Parameter>,
    returns: Return,
    error: ErrorConversion,
    call_type: TypeName,
    public_ty: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassHandle {
    ty: TypeName,
    presence: HandlePresence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SuccessSlot {
    binding: Identifier,
    ty: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ErrorConversion {
    None,
    Encoded(EncodedError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedError {
    buffer: OwnedBuffer,
    reader: Identifier,
    decode: Expression,
    free: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invocation {
    symbol: String,
    parameters: Vec<Parameter>,
    arguments: Vec<Argument>,
    returns: Return,
    error: ErrorConversion,
    asynchronous: Option<AsyncCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReturnSignature {
    ty: Option<TypeName>,
    fallible: bool,
}

#[derive(Template)]
#[template(path = "target/swift/function.swift", escape = "none")]
struct FunctionTemplate<'a> {
    function: &'a Function,
}

#[derive(Template)]
#[template(path = "target/swift/wire.swift", escape = "none")]
struct WireTemplate;

#[derive(Template)]
#[template(path = "target/swift/async.swift", escape = "none")]
struct AsyncTemplate;

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsyncCall {
    poll: Identifier,
    complete: Identifier,
    cancel: Identifier,
    free: Identifier,
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let invocation =
            Invocation::from_callable(decl.symbol(), decl.callable(), None, bridge, context)?;
        let requires_wire_runtime = invocation.requires_wire_runtime();
        let asynchronous = invocation.asynchronous();
        let (parameters, body, returns) = invocation.into_rendered("    ")?;
        Ok(Self {
            documentation: Documentation::new(decl.meta().doc(), ""),
            name: Name::new(decl.name()).function()?,
            parameters,
            body,
            returns,
            asynchronous,
            requires_wire_runtime,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = FunctionTemplate { function: self }.render()?;
        source.push_str("\n\n");
        let emitted = Emitted::primary(source);
        let emitted = match self.requires_wire_runtime {
            true => emitted.with_aux(Self::wire_helper()?),
            false => emitted,
        };
        let emitted = match self.asynchronous {
            true => emitted.with_aux(Self::async_helper()?),
            false => emitted,
        };
        Ok(emitted)
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn parameter_list(&self) -> String {
        ParameterList::new(self.parameters.iter().map(Parameter::signature)).render("    ", "")
    }

    fn body(&self) -> &str {
        &self.body
    }

    fn returns(&self) -> &ReturnSignature {
        &self.returns
    }

    fn async_keyword(&self) -> &str {
        match self.asynchronous {
            true => " async",
            false => "",
        }
    }

    fn wire_helper() -> Result<AuxChunk> {
        let mut text = WireTemplate.render()?;
        text.push_str("\n\n");
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("swift_wire")),
            text: TextChunk::new(text),
        })
    }

    fn async_helper() -> Result<AuxChunk> {
        let mut text = AsyncTemplate.render()?;
        text.push_str("\n\n");
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("swift_async")),
            text: TextChunk::new(text),
        })
    }
}

impl AssociatedFunction {
    pub fn from_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<AssociatedFunctions> {
        methods
            .iter()
            .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
            .try_fold(
                AssociatedFunctions::default(),
                |functions, method| match Self::from_method(
                    method,
                    receiver.clone(),
                    bridge,
                    context,
                ) {
                    Ok(function) => Ok(functions.with_function(function)),
                    Err(error) => functions.with_unsupported_method(method, error),
                },
            )
    }

    pub fn from_value_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        value_type: ValueType,
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<ValueFunctions> {
        methods
            .iter()
            .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
            .try_fold(ValueFunctions::default(), |functions, method| {
                let invocation =
                    Self::method_invocation(method, receiver.clone(), bridge, context)?;
                if receiver.is_none()
                    && value_type.accepts_return(method.callable().returns().plan())
                {
                    return match Initializer::from_value_method(method, value_type, invocation) {
                        Ok(initializer) => Ok(functions.with_initializer(initializer)),
                        Err(error) => functions.with_unsupported_method(method, error),
                    };
                }
                match Self::from_parts(
                    Documentation::new(method.meta().doc(), "    "),
                    receiver.is_none(),
                    receiver.as_ref().is_some_and(|receiver| {
                        receiver.requires_mutating(method.callable().receiver())
                    }),
                    Name::new(method.name()).function()?,
                    invocation,
                ) {
                    Ok(function) => Ok(functions.with_function(function)),
                    Err(error) => functions.with_unsupported_method(method, error),
                }
            })
    }

    pub fn from_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let is_static = receiver.is_none();
        Self::from_parts(
            Documentation::new(method.meta().doc(), "    "),
            is_static,
            receiver
                .as_ref()
                .is_some_and(|receiver| receiver.requires_mutating(method.callable().receiver())),
            Name::new(method.name()).function()?,
            Self::method_invocation(method, receiver, bridge, context)?,
        )
    }

    pub fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    pub fn static_keyword(&self) -> &str {
        match self.is_static {
            true => "static ",
            false => "",
        }
    }

    pub fn mutating_keyword(&self) -> &str {
        match self.mutating {
            true => "mutating ",
            false => "",
        }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameter_list(&self) -> String {
        ParameterList::new(self.parameters.iter().map(Parameter::signature))
            .render("        ", "    ")
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn returns(&self) -> &ReturnSignature {
        &self.returns
    }

    pub fn async_keyword(&self) -> &str {
        match self.asynchronous {
            true => " async",
            false => "",
        }
    }

    pub fn requires_wire_runtime(&self) -> bool {
        self.requires_wire_runtime
    }

    pub fn requires_async_runtime(&self) -> bool {
        self.asynchronous
    }

    pub fn wire_helper() -> Result<AuxChunk> {
        Function::wire_helper()
    }

    pub fn async_helper() -> Result<AuxChunk> {
        Function::async_helper()
    }

    fn from_parts(
        documentation: Documentation,
        is_static: bool,
        mutating: bool,
        name: Identifier,
        invocation: Invocation,
    ) -> Result<Self> {
        let requires_wire_runtime = invocation.requires_wire_runtime();
        let asynchronous = invocation.asynchronous();
        let (parameters, body, returns) = invocation.into_rendered("        ")?;
        Ok(Self {
            documentation,
            is_static,
            mutating,
            name,
            parameters,
            body,
            returns,
            asynchronous,
            requires_wire_runtime,
        })
    }

    fn method_invocation(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Invocation> {
        Invocation::from_callable(
            method.target(),
            method.callable(),
            receiver,
            bridge,
            context,
        )
    }
}

impl AssociatedFunctions {
    pub fn into_parts(self) -> (Vec<AssociatedFunction>, Vec<Diagnostic>) {
        (self.functions, self.diagnostics)
    }

    fn with_function(mut self, function: AssociatedFunction) -> Self {
        self.functions.push(function);
        self
    }

    fn with_unsupported_method(
        mut self,
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        error: Error,
    ) -> Result<Self> {
        match error {
            Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape } => {
                self.diagnostics.push(Diagnostic::new(format!(
                    "method {}: {}",
                    Name::new(method.name()).function()?,
                    shape
                )));
                Ok(self)
            }
            other => Err(other),
        }
    }
}

impl ValueFunctions {
    pub fn into_parts(self) -> (Vec<Initializer>, Vec<AssociatedFunction>, Vec<Diagnostic>) {
        (self.initializers, self.functions, self.diagnostics)
    }

    fn with_initializer(mut self, initializer: Initializer) -> Self {
        self.initializers.push(initializer);
        self
    }

    fn with_function(mut self, function: AssociatedFunction) -> Self {
        self.functions.push(function);
        self
    }

    fn with_unsupported_method(
        mut self,
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        error: Error,
    ) -> Result<Self> {
        match error {
            Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape } => {
                self.diagnostics.push(Diagnostic::new(format!(
                    "method {}: {}",
                    Name::new(method.name()).function()?,
                    shape
                )));
                Ok(self)
            }
            other => Err(other),
        }
    }
}

impl ValueType {
    pub const fn record(id: RecordId) -> Self {
        Self::Record(id)
    }

    pub const fn enumeration(id: EnumId) -> Self {
        Self::Enum(id)
    }

    fn constructed_type(self) -> ConstructedType {
        match self {
            Self::Record(_) => ConstructedType::Record,
            Self::Enum(_) => ConstructedType::Enum,
        }
    }

    fn accepts_return(self, plan: &boltffi_binding::ReturnPlan<Native, OutOfRust>) -> bool {
        match plan {
            boltffi_binding::ReturnPlan::DirectViaReturnSlot { ty }
            | boltffi_binding::ReturnPlan::DirectViaOutPointer { ty } => self.accepts_direct(ty),
            boltffi_binding::ReturnPlan::EncodedViaReturnSlot { ty, .. }
            | boltffi_binding::ReturnPlan::EncodedViaOutPointer { ty, .. } => {
                self.accepts_type_ref(ty)
            }
            _ => false,
        }
    }

    fn accepts_direct(self, ty: &DirectValueType) -> bool {
        match (self, ty) {
            (Self::Record(expected), DirectValueType::Record(actual)) => expected == *actual,
            (Self::Enum(expected), DirectValueType::Enum(actual)) => expected == *actual,
            _ => false,
        }
    }

    fn accepts_type_ref(self, ty: &TypeRef) -> bool {
        match (self, ty) {
            (Self::Record(expected), TypeRef::Record(actual)) => expected == *actual,
            (Self::Enum(expected), TypeRef::Enum(actual)) => expected == *actual,
            (_, TypeRef::Optional(inner)) => self.accepts_type_ref(inner),
            _ => false,
        }
    }
}

impl Initializer {
    pub fn from_class_declarations(
        initializers: &[InitializerDecl<Native>],
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Initializers> {
        let has_default_initializer = initializers
            .iter()
            .any(|initializer| InitializerSignature::is_default_name(initializer.name()));
        initializers
            .iter()
            .try_fold(Initializers::default(), |initializers, initializer| {
                match Self::from_declaration(
                    initializer,
                    ConstructedType::ClassHandle,
                    has_default_initializer,
                    bridge,
                    context,
                ) {
                    Ok(rendered) => Ok(initializers.with_initializer(rendered)),
                    Err(error) => initializers.with_unsupported(initializer, error),
                }
            })
    }

    pub fn from_record_declarations(
        initializers: &[InitializerDecl<Native>],
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Initializers> {
        initializers
            .iter()
            .try_fold(Initializers::default(), |initializers, initializer| {
                match Self::from_declaration(
                    initializer,
                    ConstructedType::Record,
                    false,
                    bridge,
                    context,
                ) {
                    Ok(rendered) => Ok(initializers.with_initializer(rendered)),
                    Err(error) => initializers.with_unsupported(initializer, error),
                }
            })
    }

    pub fn from_enum_declarations(
        initializers: &[InitializerDecl<Native>],
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Initializers> {
        initializers
            .iter()
            .try_fold(Initializers::default(), |initializers, initializer| {
                match Self::from_declaration(
                    initializer,
                    ConstructedType::Enum,
                    false,
                    bridge,
                    context,
                ) {
                    Ok(rendered) => Ok(initializers.with_initializer(rendered)),
                    Err(error) => initializers.with_unsupported(initializer, error),
                }
            })
    }

    fn from_value_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        value_type: ValueType,
        invocation: Invocation,
    ) -> Result<Self> {
        let constructed_type = value_type.constructed_type();
        let effect = InitializerEffect::new(constructed_type, &invocation);
        let requires_wire_runtime = invocation.requires_wire_runtime();
        let factory_return = invocation.returns.factory_type(constructed_type)?;
        let signature = InitializerSignature::new(
            method.name(),
            invocation.parameters.is_empty(),
            false,
            constructed_type,
        )?;
        let (parameters, body) = match signature.factory() {
            true => invocation.into_factory_rendered("        ")?,
            false => invocation.into_value_initializer_rendered("        ")?,
        };
        Ok(Self {
            documentation: Documentation::new(method.meta().doc(), "    "),
            signature,
            parameters,
            body,
            factory_return,
            effect,
            requires_wire_runtime,
        })
    }

    fn from_declaration(
        initializer: &InitializerDecl<Native>,
        constructed_type: ConstructedType,
        has_default_initializer: bool,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let invocation = Invocation::from_callable(
            initializer.symbol(),
            initializer.callable(),
            None,
            bridge,
            context,
        )?;
        let effect = InitializerEffect::new(constructed_type, &invocation);
        let requires_wire_runtime = invocation.requires_wire_runtime();
        let factory_return = invocation.returns.factory_type(constructed_type)?;
        let signature = InitializerSignature::new(
            initializer.name(),
            invocation.parameters.is_empty(),
            has_default_initializer,
            constructed_type,
        )?;
        let (parameters, body) = match signature.factory() {
            true => invocation.into_factory_rendered("        ")?,
            false => match constructed_type {
                ConstructedType::ClassHandle => invocation.into_initializer_rendered("        ")?,
                ConstructedType::Record | ConstructedType::Enum => {
                    invocation.into_value_initializer_rendered("        ")?
                }
            },
        };
        Ok(Self {
            documentation: Documentation::new(initializer.meta().doc(), "    "),
            signature,
            parameters,
            body,
            factory_return,
            effect,
            requires_wire_runtime,
        })
    }

    pub fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    pub fn factory(&self) -> bool {
        self.signature.factory()
    }

    pub fn name(&self) -> &Identifier {
        self.signature.name()
    }

    pub fn factory_return(&self) -> &TypeName {
        &self.factory_return
    }

    pub fn parameter_list(&self) -> String {
        self.signature
            .parameter_list(&self.parameters, "        ", "    ")
    }

    pub fn failable_marker(&self) -> &str {
        self.effect.failable_marker()
    }

    pub fn throwing_keyword(&self) -> &str {
        self.effect.throwing_keyword()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn requires_wire_runtime(&self) -> bool {
        self.requires_wire_runtime
    }
}

impl InitializerSignature {
    fn new(
        name: &CanonicalName,
        no_parameters: bool,
        has_default_initializer: bool,
        constructed_type: ConstructedType,
    ) -> Result<Self> {
        match constructed_type {
            ConstructedType::ClassHandle => {
                if Self::is_default_name(name) || (!has_default_initializer && no_parameters) {
                    Ok(Self::Default)
                } else if no_parameters {
                    Ok(Self::NamedFactory {
                        name: Name::new(name).function()?,
                    })
                } else {
                    Ok(Self::NamedInit {
                        label: Name::new(name).function()?,
                    })
                }
            }
            ConstructedType::Record => {
                if Self::is_default_name(name) || no_parameters {
                    Ok(Self::NamedFactory {
                        name: Name::new(name).function()?,
                    })
                } else {
                    Ok(Self::NamedInit {
                        label: Name::new(name).function()?,
                    })
                }
            }
            ConstructedType::Enum => {
                if Self::is_default_name(name) {
                    Ok(Self::Default)
                } else if no_parameters {
                    Ok(Self::NamedFactory {
                        name: Name::new(name).function()?,
                    })
                } else {
                    Ok(Self::NamedInit {
                        label: Name::new(name).function()?,
                    })
                }
            }
        }
    }

    fn factory(&self) -> bool {
        matches!(self, Self::NamedFactory { .. })
    }

    fn name(&self) -> &Identifier {
        match self {
            Self::NamedFactory { name } => name,
            Self::Default | Self::NamedInit { .. } => unreachable!(),
        }
    }

    fn parameter_list(
        &self,
        parameters: &[Parameter],
        parameter_indent: &str,
        closing_indent: &str,
    ) -> String {
        let parameters = match self {
            Self::Default | Self::NamedFactory { .. } => {
                ParameterList::new(parameters.iter().map(Parameter::signature))
            }
            Self::NamedInit { label } => {
                ParameterList::new(parameters.split_first().into_iter().flat_map(
                    |(first, rest)| {
                        std::iter::once(format!("{label} {}: {}", first.name, first.ty))
                            .chain(rest.iter().map(Parameter::signature))
                    },
                ))
            }
        };
        parameters.render(parameter_indent, closing_indent)
    }

    fn is_default_name(name: &CanonicalName) -> bool {
        name.parts().len() == 1 && name.parts()[0].as_str() == "new"
    }
}

impl InitializerEffect {
    fn new(constructed_type: ConstructedType, invocation: &Invocation) -> Self {
        match (
            constructed_type,
            invocation.error.fallible(),
            invocation.returns.optional(),
        ) {
            (_, true, _) => Self::Throwing,
            (ConstructedType::Record | ConstructedType::Enum, false, true) => Self::Failable,
            _ => Self::Plain,
        }
    }

    fn failable_marker(self) -> &'static str {
        match self {
            Self::Failable => "?",
            Self::Plain | Self::Throwing => "",
        }
    }

    fn throwing_keyword(self) -> &'static str {
        match self {
            Self::Throwing => " throws",
            Self::Plain | Self::Failable => "",
        }
    }
}

impl Initializers {
    pub fn into_parts(self) -> (Vec<Initializer>, Vec<Diagnostic>) {
        (self.initializers, self.diagnostics)
    }

    fn with_initializer(mut self, initializer: Initializer) -> Self {
        self.initializers.push(initializer);
        self
    }

    fn with_unsupported(
        mut self,
        initializer: &InitializerDecl<Native>,
        error: Error,
    ) -> Result<Self> {
        match error {
            Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape } => {
                self.diagnostics.push(Diagnostic::new(format!(
                    "initializer {}: {}",
                    Name::new(initializer.name()).function()?,
                    shape
                )));
                Ok(self)
            }
            other => Err(other),
        }
    }
}

impl Receiver {
    pub fn direct() -> Self {
        Self {
            kind: ReceiverKind::Value,
            argument: Argument::Direct(Expression::member("self", "cValue")),
            mutable_argument: None,
        }
    }

    pub fn encoded(
        name: &CanonicalName,
        read: &ReadPlan,
        write: &WritePlan,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source = Name::new(name);
        Ok(Self {
            kind: ReceiverKind::Value,
            argument: Argument::Encoded(EncodedArgument::new(
                &source,
                write,
                Expression::new("self"),
                context,
            )?),
            mutable_argument: Some(Argument::MutableEncoded(MutableEncodedArgument::new(
                &source,
                write,
                read,
                Expression::new("self"),
                Expression::new("self"),
                bridge,
                context,
            )?)),
        })
    }

    pub fn class_handle() -> Self {
        Self {
            kind: ReceiverKind::Handle,
            argument: Argument::Direct(Expression::member("self", "handle")),
            mutable_argument: None,
        }
    }

    fn accepts(&self, receive: Receive) -> bool {
        match (receive, self.kind) {
            (Receive::ByValue | Receive::ByRef, _) => true,
            (Receive::ByMutRef, ReceiverKind::Handle) => true,
            (Receive::ByMutRef, ReceiverKind::Value) => self.mutable_argument.is_some(),
            _ => false,
        }
    }

    fn argument(self) -> Argument {
        self.argument
    }

    fn mutable_argument(self) -> Result<Argument> {
        self.mutable_argument
            .ok_or(SwiftHost::unsupported("mutable value receiver"))
    }

    fn requires_mutating(&self, receive: Option<Receive>) -> bool {
        matches!(
            (receive, self.kind),
            (Some(Receive::ByMutRef), ReceiverKind::Value)
        )
    }
}

impl Invocation {
    pub fn from_callable(
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::check_receiver(callable, receiver.as_ref())?;
        let c_function = Self::c_function(symbol, bridge)?;
        let (return_function, asynchronous) = match callable.execution() {
            ExecutionDecl::Synchronous(_) => (c_function, None),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                ..
            }) => (
                Self::c_function(complete, bridge)?,
                Some(AsyncCall::new(poll, complete, cancel, free)?),
            ),
            ExecutionDecl::Asynchronous(_) => {
                return Err(SwiftHost::unsupported("async function protocol"));
            }
            _ => return Err(SwiftHost::unsupported("unknown function execution")),
        };
        let error = ErrorConversion::from_channel(
            callable.error().channel(),
            return_function,
            bridge,
            context,
        )?;
        let parameters = callable
            .params()
            .iter()
            .map(|parameter| Parameter::from_decl(parameter, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let receiver_argument = receiver
            .map(|receiver| match callable.receiver() {
                Some(Receive::ByMutRef) if receiver.kind == ReceiverKind::Value => {
                    receiver.mutable_argument()
                }
                Some(_) => Ok(receiver.argument()),
                None => Err(SwiftHost::unsupported("method receiver mismatch")),
            })
            .transpose()?;
        let arguments = receiver_argument
            .into_iter()
            .chain(parameters.iter().map(Parameter::argument))
            .collect::<Vec<_>>();
        let returns = callable.returns().plan().render_with(&mut ReturnPlan {
            bridge,
            context,
            c_return_channel: return_function.return_channel(),
            parameter_groups: return_function.parameter_groups(),
        })?;
        Self::check_mutable_encoded_arguments(&arguments, &returns, &error, asynchronous.as_ref())?;
        Ok(Self {
            symbol: c_function.name().to_owned(),
            parameters,
            arguments,
            returns,
            error,
            asynchronous,
        })
    }

    pub fn into_rendered(self, indent: &str) -> Result<(Vec<Parameter>, String, ReturnSignature)> {
        let fallible = self.error.fallible() || self.asynchronous();
        let body = self.render_body(indent, fallible)?;
        let returns = self.returns.signature(fallible);
        Ok((self.parameters, body, returns))
    }

    fn into_initializer_rendered(self, indent: &str) -> Result<(Vec<Parameter>, String)> {
        let body = self.render_initializer_body(indent)?;
        Ok((self.parameters, body))
    }

    fn into_value_initializer_rendered(self, indent: &str) -> Result<(Vec<Parameter>, String)> {
        let body = self.render_value_initializer_body(indent)?;
        Ok((self.parameters, body))
    }

    fn into_factory_rendered(self, indent: &str) -> Result<(Vec<Parameter>, String)> {
        let body = self.render_factory_body(indent, self.error.fallible())?;
        Ok((self.parameters, body))
    }

    pub fn requires_wire_runtime(&self) -> bool {
        self.arguments.iter().any(Argument::requires_wire_runtime)
            || self.returns.requires_wire_runtime()
            || self.error.requires_wire_runtime()
    }

    pub fn asynchronous(&self) -> bool {
        self.asynchronous.is_some()
    }

    fn render_body(&self, indent: &str, fallible: bool) -> Result<String> {
        let body = match &self.asynchronous {
            Some(asynchronous) => self.render_async_body(asynchronous, indent)?,
            None => Self::render_scoped_body(
                &self.arguments,
                &self.returns,
                &self.error,
                self.call(),
                indent,
                self.returns.exit(fallible),
            )?,
        };
        Ok(body)
    }

    fn render_initializer_body(&self, indent: &str) -> Result<String> {
        if Self::has_scoped_arguments(&self.arguments) {
            let handle = GeneratedLocal::ReturnHandle.identifier()?;
            let setup = Self::render_scoped_initializer_handle(
                &self.arguments,
                &self.returns,
                &self.error,
                self.call(),
                indent,
                &handle,
            )?;
            let assign = Statement::assign(
                Expression::member("self", "handle"),
                Expression::identifier(handle),
            )
            .indented(indent);
            return Ok([setup, assign].join("\n"));
        }
        Self::render_scoped_initializer_body(
            &self.arguments,
            &self.returns,
            &self.error,
            self.call(),
            indent,
        )
    }

    fn render_value_initializer_body(&self, indent: &str) -> Result<String> {
        with_lexical_plan::<Syntax, _>(|lexical| {
            self.render_value_initializer_body_with(indent, lexical)
        })
    }

    fn render_value_initializer_body_with<'plan>(
        &self,
        indent: &str,
        lexical: &mut LexicalPlan<'plan, Syntax>,
    ) -> Result<String> {
        let scope = self.reserve_value_initializer_parameters(lexical)?;
        if Self::has_scoped_arguments(&self.arguments) {
            let (setup, reference) = Self::render_scoped_value_initializer_value(
                &self.arguments,
                &self.returns,
                &self.error,
                self.call(),
                indent,
                lexical,
                scope,
            )?
            .into_bound()?;
            let assign = Return::assign_initializer_reference(
                lexical,
                scope,
                reference,
                indent,
                self.returns.optional(),
            )?;
            return Ok([setup, assign].join("\n"));
        }
        Self::render_scoped_value_initializer_body(
            &self.arguments,
            &self.returns,
            &self.error,
            self.call(),
            indent,
            lexical,
            scope,
        )
    }

    fn reserve_value_initializer_parameters<'plan>(
        &self,
        lexical: &mut LexicalPlan<'plan, Syntax>,
    ) -> Result<Scope<'plan, Syntax>> {
        let scope = lexical.root();
        self.parameters.iter().try_for_each(|parameter| {
            lexical
                .reserve_external(scope, parameter.name.clone())
                .map(|_| ())
                .ok_or(Error::UnexpectedBindingShape {
                    layer: "Swift lexical planner",
                    shape: "duplicate value initializer parameter",
                })
        })?;
        Ok(scope)
    }

    fn render_factory_body(&self, indent: &str, fallible: bool) -> Result<String> {
        Self::render_scoped_factory_body(
            &self.arguments,
            &self.returns,
            &self.error,
            self.call(),
            indent,
            self.returns.exit(fallible),
        )
    }

    fn has_scoped_arguments(arguments: &[Argument]) -> bool {
        arguments.iter().any(Argument::uses_scope)
    }

    fn render_scoped_body(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
        exit: BodyExit,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    exit,
                )?,
                indent,
                exit,
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    exit,
                )?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap_result(
                Self::render_scoped_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    exit,
                )?,
                indent,
                exit.returns_value(),
                exit.throws(),
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_body(rest, returns, error, call, indent, exit)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_body(rest, returns, error, call, indent, exit)
            }
            None => returns.body(call, error, indent),
        }
    }

    fn render_scoped_initializer_body(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_initializer_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
                BodyExit::CompleteEffect,
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_initializer_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_initializer_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
                false,
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_initializer_body(rest, returns, error, call, indent)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_initializer_body(rest, returns, error, call, indent)
            }
            None => returns.initializer_body(call, error, indent),
        }
    }

    fn render_scoped_value_initializer_body<'plan>(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_value_initializer_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    lexical,
                    scope,
                )?,
                indent,
                BodyExit::CompleteEffect,
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_value_initializer_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    lexical,
                    scope,
                )?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_value_initializer_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    lexical,
                    scope,
                )?,
                indent,
                false,
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_value_initializer_body(
                    rest, returns, error, call, indent, lexical, scope,
                )?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => Self::render_scoped_value_initializer_body(
                rest, returns, error, call, indent, lexical, scope,
            ),
            None => returns.value_initializer_body(call, error, indent, lexical, scope),
        }
    }

    fn render_scoped_value_initializer_value<'plan>(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<ScopedInitializerValue<'plan>> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => {
                let declaration = lexical
                    .allocate(scope, &GeneratedLocal::ReturnBuffer.suffixed_stem("value"))?;
                let closure = lexical.child(scope, ScopeForm::Closure);
                let nested_indent = format!("{indent}    ");
                let body = Self::render_scoped_value_initializer_value(
                    rest,
                    returns,
                    error,
                    call,
                    &nested_indent,
                    lexical,
                    closure,
                )?
                .into_scope_body(lexical, closure, &nested_indent)?;
                let (source, reference) = lexical
                    .declare(declaration, |binding| {
                        argument.bind(binding, body, indent, error.fallible())
                    })
                    .into_parts();
                Ok(ScopedInitializerValue::Bound { source, reference })
            }
            Some((Argument::MutableEncoded(_), _)) => Err(SwiftHost::unsupported(
                "mutable encoded value initializer argument",
            )),
            Some((Argument::DirectVector(argument), rest)) => {
                let declaration = lexical
                    .allocate(scope, &GeneratedLocal::ReturnBuffer.suffixed_stem("value"))?;
                let closure = lexical.child(scope, ScopeForm::Closure);
                let nested_indent = format!("{indent}    ");
                let body = Self::render_scoped_value_initializer_value(
                    rest,
                    returns,
                    error,
                    call,
                    &nested_indent,
                    lexical,
                    closure,
                )?
                .into_scope_body(lexical, closure, &nested_indent)?;
                let (source, reference) = lexical
                    .declare(declaration, |binding| {
                        argument.wrap_binding(binding, body, indent, error.fallible())
                    })
                    .into_parts();
                Ok(ScopedInitializerValue::Bound { source, reference })
            }
            Some((Argument::Closure(argument), rest)) => {
                Self::render_scoped_value_initializer_value(
                    rest, returns, error, call, indent, lexical, scope,
                )?
                .wrap(|source| Ok(argument.wrap(source, indent)))
            }
            Some((Argument::Direct(_), rest)) => Self::render_scoped_value_initializer_value(
                rest, returns, error, call, indent, lexical, scope,
            ),
            None => returns
                .body(call, error, indent)
                .map(ScopedInitializerValue::Terminal),
        }
    }

    fn render_scoped_initializer_handle(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
        handle: &Identifier,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.bind(
                handle,
                Self::render_scoped_initializer_handle_value(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
                error.fallible(),
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_initializer_handle(rest, returns, error, call, indent, handle)?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap_binding(
                handle,
                Self::render_scoped_initializer_handle_value(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
                error.fallible(),
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_initializer_handle(rest, returns, error, call, indent, handle)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_initializer_handle(rest, returns, error, call, indent, handle)
            }
            None => Err(SwiftHost::unsupported("initializer argument scope")),
        }
    }

    fn render_scoped_initializer_handle_value(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_initializer_handle_value(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
                returns.exit(error.fallible()),
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_initializer_handle_value(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap_result(
                Self::render_scoped_initializer_handle_value(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                )?,
                indent,
                true,
                error.fallible(),
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_initializer_handle_value(rest, returns, error, call, indent)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_initializer_handle_value(rest, returns, error, call, indent)
            }
            None => returns.initializer_value_body(call, error, indent),
        }
    }

    fn render_scoped_factory_body(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        call: Expression,
        indent: &str,
        exit: BodyExit,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_factory_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    exit,
                )?,
                indent,
                exit,
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_factory_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    exit,
                )?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap_result(
                Self::render_scoped_factory_body(
                    rest,
                    returns,
                    error,
                    call,
                    &format!("{indent}    "),
                    exit,
                )?,
                indent,
                exit.returns_value(),
                exit.throws(),
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_factory_body(rest, returns, error, call, indent, exit)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_factory_body(rest, returns, error, call, indent, exit)
            }
            None => returns.factory_body(call, error, indent),
        }
    }

    fn render_async_body(&self, asynchronous: &AsyncCall, indent: &str) -> Result<String> {
        let future = GeneratedLocal::FutureHandle.identifier()?;
        let start =
            Self::render_scoped_async_start(&self.arguments, self.start_call(), indent, &future)?;
        let complete =
            asynchronous.body_from_future(&future, &self.returns, &self.error, indent)?;
        Ok([start, complete]
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn render_scoped_async_start(
        arguments: &[Argument],
        start_call: Expression,
        indent: &str,
        future: &Identifier,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.bind(
                future,
                Self::render_scoped_async_start_value(rest, start_call, &format!("{indent}    "))?,
                indent,
                false,
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_async_start(rest, start_call, indent, future)?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap_binding(
                future,
                Self::render_scoped_async_start_value(rest, start_call, &format!("{indent}    "))?,
                indent,
                false,
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_async_start(rest, start_call, indent, future)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_async_start(rest, start_call, indent, future)
            }
            None => Ok(Statement::let_value(future, start_call).indented(indent)),
        }
    }

    fn render_scoped_async_start_value(
        arguments: &[Argument],
        start_call: Expression,
        indent: &str,
    ) -> Result<String> {
        match arguments.split_first() {
            Some((Argument::Encoded(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_async_start_value(rest, start_call, &format!("{indent}    "))?,
                indent,
                BodyExit::ReturnValue,
            )),
            Some((Argument::MutableEncoded(argument), rest)) => argument.wrap(
                Self::render_scoped_async_start_value(rest, start_call, &format!("{indent}    "))?,
                indent,
            ),
            Some((Argument::DirectVector(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_async_start_value(rest, start_call, &format!("{indent}    "))?,
                indent,
                true,
            )),
            Some((Argument::Closure(argument), rest)) => Ok(argument.wrap(
                Self::render_scoped_async_start_value(rest, start_call, indent)?,
                indent,
            )),
            Some((Argument::Direct(_), rest)) => {
                Self::render_scoped_async_start_value(rest, start_call, indent)
            }
            None => Ok(Statement::returns(start_call).indented(indent)),
        }
    }

    fn call(&self) -> Expression {
        Expression::call_with_layout(
            &self.symbol,
            self.arguments
                .iter()
                .flat_map(Argument::arguments)
                .chain(self.returns.arguments())
                .collect::<ArgumentList>(),
            "    ",
            "",
        )
    }

    fn start_call(&self) -> Expression {
        Expression::call_with_layout(
            &self.symbol,
            self.arguments
                .iter()
                .flat_map(Argument::arguments)
                .collect::<ArgumentList>(),
            "    ",
            "",
        )
    }

    fn c_function<'bridge>(
        symbol: &NativeSymbol,
        bridge: &'bridge CBridgeContract,
    ) -> Result<&'bridge CFunction> {
        bridge
            .functions()
            .iter()
            .find(|function| function.name() == symbol.name().as_str())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing C function for Swift function",
            })
    }

    fn check_receiver(
        callable: &ExportedCallable<Native>,
        receiver: Option<&Receiver>,
    ) -> Result<()> {
        match (callable.receiver(), receiver) {
            (None, None) => Ok(()),
            (Some(receive), Some(receiver)) if receiver.accepts(receive) => Ok(()),
            (Some(Receive::ByMutRef), Some(receiver)) if receiver.kind == ReceiverKind::Value => {
                Err(SwiftHost::unsupported("mutable value receiver"))
            }
            _ => Err(SwiftHost::unsupported("method receiver mismatch")),
        }
    }

    fn check_mutable_encoded_arguments(
        arguments: &[Argument],
        returns: &Return,
        error: &ErrorConversion,
        asynchronous: Option<&AsyncCall>,
    ) -> Result<()> {
        if !arguments.iter().any(Argument::mutable_encoded) {
            return Ok(());
        }
        if returns.ty.is_some() {
            return Err(SwiftHost::unsupported(
                "mutable encoded parameter with return value",
            ));
        }
        if error.fallible() {
            return Err(SwiftHost::unsupported(
                "mutable encoded parameter with error channel",
            ));
        }
        if asynchronous.is_some() {
            return Err(SwiftHost::unsupported(
                "mutable encoded parameter with async execution",
            ));
        }
        Ok(())
    }
}

impl Parameter {
    fn from_decl(
        decl: &ParamDecl<Native, IntoRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source_name = Name::new(decl.name());
        let name = source_name.parameter()?;
        let mut plan = ParameterPlan {
            source_name,
            name,
            bridge,
            context,
        };
        match decl.payload() {
            IncomingParam::Value(value) => value.render_with(&mut plan),
            IncomingParam::Closure(closure) => plan.closure(closure),
        }
    }

    pub fn signature(&self) -> String {
        format!("{}: {}", self.name, self.ty)
    }

    fn argument(&self) -> Argument {
        self.argument.clone()
    }
}

impl Argument {
    fn arguments(&self) -> Vec<Expression> {
        match self {
            Self::Direct(argument) => vec![argument.clone()],
            Self::Encoded(argument) => argument.arguments(),
            Self::MutableEncoded(argument) => argument.arguments(),
            Self::DirectVector(argument) => argument.arguments(),
            Self::Closure(argument) => argument.arguments(),
        }
    }

    fn requires_wire_runtime(&self) -> bool {
        matches!(self, Self::Encoded(_) | Self::MutableEncoded(_))
    }

    fn uses_scope(&self) -> bool {
        matches!(
            self,
            Self::Encoded(_) | Self::MutableEncoded(_) | Self::DirectVector(_)
        )
    }

    fn mutable_encoded(&self) -> bool {
        matches!(self, Self::MutableEncoded(_))
    }
}

impl EncodedArgument {
    fn new(
        source_name: &Name,
        plan: &WritePlan,
        current: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let buffer = ArgumentBuffer::new(source_name)?;
        let write = plan
            .render_with(&mut Writer::new(buffer.writer().clone(), current, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        Ok(Self {
            buffer: buffer.with_statements(write),
        })
    }

    fn scalar_option(source_name: &Name, primitive: Primitive, value: Expression) -> Result<Self> {
        ScalarOption::new(primitive)
            .write(source_name, value)
            .map(|buffer| Self { buffer })
    }

    fn arguments(&self) -> Vec<Expression> {
        self.buffer.arguments()
    }

    fn wrap(&self, body: String, indent: &str, exit: BodyExit) -> String {
        let body = Statement::new(body);
        format!(
            "{}\n{}",
            self.buffer.bytes_statement().indented(indent),
            match exit {
                BodyExit::ReturnValue => self.buffer.returning_scope(body, indent, false),
                BodyExit::ThrowingReturnValue => self.buffer.returning_scope(body, indent, true),
                BodyExit::ThrowingEffect => self.buffer.effect_scope(body, indent, true),
                BodyExit::CompleteEffect => self.buffer.effect_scope(body, indent, false),
            }
        )
    }

    fn bind(&self, binding: &Identifier, body: String, indent: &str, throwing: bool) -> String {
        let body = Statement::new(body);
        format!(
            "{}\n{}",
            self.buffer.bytes_statement().indented(indent),
            self.buffer.binding_scope(binding, body, indent, throwing)
        )
    }
}

impl MutableEncodedArgument {
    fn new(
        source_name: &Name,
        write: &WritePlan,
        read: &ReadPlan,
        current: Expression,
        target: Expression,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let input = ArgumentBuffer::new(source_name)?;
        let write = write
            .render_with(&mut Writer::new(input.writer().clone(), current, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        let reader = source_name.generated("reader")?;
        let decode = read
            .render_with(&mut Reader::new(reader.clone(), context))
            .map(ReadExpression::into_expression)?;
        Ok(Self {
            target,
            input: input.with_statements(write),
            output: OwnedBuffer::new(source_name.generated("out")?),
            reader,
            decode,
            free: Identifier::parse(bridge.support().buffer_free()?.name())?,
        })
    }

    fn arguments(&self) -> Vec<Expression> {
        self.input
            .arguments()
            .into_iter()
            .chain([Expression::address(self.output.binding())])
            .collect()
    }

    fn wrap(&self, body: String, indent: &str) -> Result<String> {
        Ok([
            self.input.bytes_statement().indented(indent),
            Statement::var_value(
                self.output.binding(),
                "FfiBuf_u8",
                Expression::call("FfiBuf_u8", ArgumentList::default()),
            )
            .indented(indent),
            Statement::defer(self.output.free_call(&self.free)).indented(indent),
            self.input.effect_scope(Statement::new(body), indent, false),
            self.writeback(indent)?,
        ]
        .join("\n"))
    }

    fn writeback(&self, indent: &str) -> Result<String> {
        self.output
            .decode(&self.reader, &self.decode)
            .map(|decode| Statement::assign(&self.target, decode).indented(indent))
    }
}

impl BodyExit {
    fn returns_value(self) -> bool {
        matches!(self, Self::ReturnValue | Self::ThrowingReturnValue)
    }

    fn throws(self) -> bool {
        matches!(self, Self::ThrowingReturnValue | Self::ThrowingEffect)
    }
}

impl AsyncCall {
    fn new(
        poll: &NativeSymbol,
        complete: &NativeSymbol,
        cancel: &NativeSymbol,
        free: &NativeSymbol,
    ) -> Result<Self> {
        Ok(Self {
            poll: Identifier::parse(poll.name().as_str())?,
            complete: Identifier::parse(complete.name().as_str())?,
            cancel: Identifier::parse(cancel.name().as_str())?,
            free: Identifier::parse(free.name().as_str())?,
        })
    }

    fn body_from_future(
        &self,
        future: &Identifier,
        returns: &Return,
        error: &ErrorConversion,
        indent: &str,
    ) -> Result<String> {
        let status = GeneratedLocal::FutureStatus.identifier()?;
        let complete_call = self.complete_call(future, &status, returns);
        let complete_body =
            returns.async_body(complete_call, error, &status, &format!("{indent}    "))?;
        Ok(Statement::try_await_returning_trailing_closure(
            "boltffiAsyncCall",
            [
                Expression::labeled("futureHandle", future),
                Expression::labeled("poll", &self.poll),
                Expression::labeled("cancel", &self.cancel),
                Expression::labeled("free", &self.free),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
            [future.clone(), status],
            complete_body,
            indent,
        ))
    }

    fn complete_call(
        &self,
        future: &Identifier,
        status: &Identifier,
        returns: &Return,
    ) -> Expression {
        Expression::call(
            &self.complete,
            std::iter::once(Expression::identifier(future.clone()))
                .chain(std::iter::once(Expression::identifier(status.clone())))
                .chain(returns.arguments())
                .collect::<ArgumentList>(),
        )
    }
}

struct ParameterPlan<'context, 'bindings> {
    source_name: Name,
    name: Identifier,
    bridge: &'context CBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl ParameterPlan<'_, '_> {
    fn closure(
        &mut self,
        closure: &boltffi_binding::ClosureParameter<Native, IntoRust>,
    ) -> Result<Parameter> {
        let argument = ClosureArgument::new(
            &self.source_name,
            self.name.clone(),
            closure,
            self.bridge,
            self.context,
        )?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: argument.parameter_ty(),
            argument: Argument::Closure(Box::new(argument)),
        })
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterPlan<'_, '_> {
    type Output = Result<Parameter>;

    fn direct(&mut self, ty: &'plan DirectValueType, receive: Receive) -> Self::Output {
        if receive == Receive::ByMutRef {
            return Err(SwiftHost::unsupported("mutable direct parameter"));
        }
        let direct = DirectValue::new(ty, self.bridge, self.context)?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: direct.api_type().clone(),
            argument: Argument::Direct(direct.c_value(Expression::identifier(self.name.clone()))),
        })
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        if shape != native::BufferShape::Slice {
            return Err(SwiftHost::unsupported("encoded parameter shape"));
        }
        if receive == Receive::ByMutRef {
            let read = codec.read_plan();
            return Ok(Parameter {
                name: self.name.clone(),
                ty: TypeName::new(format!("inout {}", SwiftType::type_ref(ty, self.context)?)),
                argument: Argument::MutableEncoded(MutableEncodedArgument::new(
                    &self.source_name,
                    codec,
                    &read,
                    Expression::identifier(self.name.clone()),
                    Expression::identifier(self.name.clone()),
                    self.bridge,
                    self.context,
                )?),
            });
        }
        Ok(Parameter {
            name: self.name.clone(),
            ty: SwiftType::type_ref(ty, self.context)?,
            argument: Argument::Encoded(EncodedArgument::new(
                &self.source_name,
                codec,
                Expression::identifier(self.name.clone()),
                self.context,
            )?),
        })
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(*class, presence, self.context)?;
                Ok(Parameter {
                    name: self.name.clone(),
                    ty: handle.api_type(),
                    argument: Argument::Direct(
                        handle.parameter_argument(Expression::identifier(self.name.clone())),
                    ),
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(Parameter {
                    name: self.name.clone(),
                    ty: handle.api_type(),
                    argument: Argument::Direct(
                        handle.c_handle(Expression::identifier(self.name.clone())),
                    ),
                })
            }
            HandleTarget::Stream(_) => Err(SwiftHost::unsupported("stream handle parameter")),
            _ => Err(SwiftHost::unsupported("unknown handle parameter")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(Parameter {
            name: self.name.clone(),
            ty: ScalarOption::new(primitive).ty()?,
            argument: Argument::Encoded(EncodedArgument::scalar_option(
                &self.source_name,
                primitive,
                Expression::identifier(self.name.clone()),
            )?),
        })
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        receive: Receive,
    ) -> Self::Output {
        let vector = DirectVector::from_element(element, self.bridge, self.context)?;
        let ty = match receive {
            Receive::ByMutRef => TypeName::new(format!("inout {}", vector.ty())),
            _ => vector.ty().clone(),
        };
        Ok(Parameter {
            name: self.name.clone(),
            ty,
            argument: Argument::DirectVector(vector.borrowed(
                &self.source_name,
                self.name.clone(),
                receive,
            )?),
        })
    }
}

impl Return {
    fn signature(&self, fallible: bool) -> ReturnSignature {
        ReturnSignature {
            ty: self.ty.clone(),
            fallible,
        }
    }

    fn exit(&self, fallible: bool) -> BodyExit {
        match (self.ty.is_some(), fallible) {
            (true, true) => BodyExit::ThrowingReturnValue,
            (true, false) => BodyExit::ReturnValue,
            (false, true) => BodyExit::ThrowingEffect,
            (false, false) => BodyExit::CompleteEffect,
        }
    }

    fn requires_wire_runtime(&self) -> bool {
        match &self.conversion {
            ReturnConversion::Encoded(_) => true,
            ReturnConversion::Closure(closure) => closure.requires_wire_runtime(),
            _ => false,
        }
    }

    fn optional(&self) -> bool {
        self.optional
    }

    fn factory_type(&self, constructed_type: ConstructedType) -> Result<TypeName> {
        match constructed_type {
            ConstructedType::ClassHandle => Ok(TypeName::new("Self")),
            ConstructedType::Record | ConstructedType::Enum => self
                .ty
                .clone()
                .ok_or(SwiftHost::unsupported("void value initializer")),
        }
    }

    fn arguments(&self) -> impl Iterator<Item = Expression> + '_ {
        let success = self.success.iter().map(SuccessSlot::argument);
        let closure = match &self.conversion {
            ReturnConversion::Closure(closure) => Some(closure.argument()),
            _ => None,
        };
        success.chain(closure)
    }

    fn body(&self, call: Expression, error: &ErrorConversion, indent: &str) -> Result<String> {
        if let ReturnConversion::Closure(closure) = &self.conversion {
            return closure.body(call, error, indent);
        }
        let setup = self
            .success
            .as_ref()
            .map(|success| success.statement().indented(indent));
        let error = error.body(call.clone(), indent)?;
        let success = self.success.as_ref().map(SuccessSlot::expression);
        let result = match (success, error.consumes_call()) {
            (Some(value), _) => Some(self.body_for_success(value, indent)?),
            (None, false) => Some(self.body_for_value(call, indent)?),
            (None, true) => None,
        };
        Ok([setup, Some(error.text), result]
            .into_iter()
            .flatten()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn async_body(
        &self,
        call: Expression,
        error: &ErrorConversion,
        status: &Identifier,
        indent: &str,
    ) -> Result<String> {
        let setup = self
            .success
            .as_ref()
            .map(|success| success.statement().indented(indent));
        let error = error.body(call.clone(), indent)?;
        let complete = if error.consumes_call() {
            None
        } else if self.ty.is_some() || self.success.is_some() {
            let binding = self.async_complete_binding()?;
            Some((
                Some(binding.clone()),
                Statement::let_value(&binding, call).indented(indent),
            ))
        } else {
            Some((None, Statement::expression(call).indented(indent)))
        };
        let status = Self::async_status_guard(status, indent)?;
        let success = self.success.as_ref().map(SuccessSlot::expression);
        let value = match (&self.ty, success, complete.as_ref()) {
            (_, Some(value), _) => Some(value),
            (Some(_), None, Some((Some(binding), _))) => {
                Some(Expression::identifier(binding.clone()))
            }
            _ => None,
        };
        let result = value
            .map(|value| self.body_for_success(value, indent))
            .transpose()?;
        Ok([
            setup,
            Some(error.text),
            complete.map(|(_, statement)| statement),
            Some(status),
            result,
        ]
        .into_iter()
        .flatten()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
    }

    fn async_status_guard(status: &Identifier, indent: &str) -> Result<String> {
        let code = GeneratedLocal::FutureStatus.suffixed("code")?;
        let pointee_code = Expression::member(
            Expression::member(Expression::forced(status), "pointee"),
            "code",
        );
        Ok([
            Statement::let_value(&code, pointee_code).indented(indent),
            Statement::guard_else(
                Expression::equal(&code, Expression::literal(Literal::integer(0))),
                [Statement::throwing(Expression::call(
                    "FfiError",
                    [Expression::labeled(
                        "message",
                        Expression::literal(Literal::interpolated(
                            "FFI failed in async completion with code ",
                            Expression::identifier(code),
                            "",
                        )),
                    )]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ))],
            )
            .indented(indent),
        ]
        .join("\n"))
    }

    fn async_complete_binding(&self) -> Result<Identifier> {
        if self.success.is_some() {
            return GeneratedLocal::ReturnBuffer.suffixed("complete");
        }
        match &self.conversion {
            ReturnConversion::Encoded(encoded) => Ok(encoded.buffer.binding().clone()),
            _ => GeneratedLocal::ReturnBuffer.identifier(),
        }
    }

    fn body_for_value(&self, value: Expression, indent: &str) -> Result<String> {
        match &self.conversion {
            ReturnConversion::Encoded(encoded) => encoded.body(value, indent),
            ReturnConversion::DirectVector { vector, free } => vector.body(value, indent, free),
            ReturnConversion::ClassHandle(handle) => handle.body(value, indent),
            ReturnConversion::CallbackHandle(handle) => handle.body(value, indent),
            ReturnConversion::Closure(_) => Err(SwiftHost::unsupported("closure value body")),
            ReturnConversion::Direct | ReturnConversion::FromC(_) => {
                Ok(self.statement(value).indented(indent))
            }
        }
    }

    fn body_for_success(&self, value: Expression, indent: &str) -> Result<String> {
        match &self.conversion {
            ReturnConversion::Encoded(encoded)
                if value == Expression::identifier(encoded.buffer.binding().clone()) =>
            {
                encoded.body_from_buffer(indent)
            }
            _ => self.body_for_value(value, indent),
        }
    }

    fn initializer_body(
        &self,
        call: Expression,
        error: &ErrorConversion,
        indent: &str,
    ) -> Result<String> {
        let ReturnConversion::ClassHandle(_) = &self.conversion else {
            return Err(SwiftHost::unsupported("class initializer return"));
        };
        let setup = self
            .success
            .as_ref()
            .map(|success| success.statement().indented(indent));
        let error = error.body(call.clone(), indent)?;
        let value = match (
            self.success.as_ref().map(SuccessSlot::expression),
            error.consumes_call(),
        ) {
            (Some(value), _) => value,
            (None, false) => call,
            (None, true) => return Err(SwiftHost::unsupported("class initializer result")),
        };
        let assign =
            Statement::assign(Expression::member("self", "handle"), value).indented(indent);
        Ok([setup, Some(error.text), Some(assign)]
            .into_iter()
            .flatten()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn initializer_value_body(
        &self,
        call: Expression,
        error: &ErrorConversion,
        indent: &str,
    ) -> Result<String> {
        let ReturnConversion::ClassHandle(_) = &self.conversion else {
            return Err(SwiftHost::unsupported("class initializer return"));
        };
        let setup = self
            .success
            .as_ref()
            .map(|success| success.statement().indented(indent));
        let error = error.body(call.clone(), indent)?;
        let value = match (
            self.success.as_ref().map(SuccessSlot::expression),
            error.consumes_call(),
        ) {
            (Some(value), _) => value,
            (None, false) => call,
            (None, true) => return Err(SwiftHost::unsupported("class initializer result")),
        };
        let result = Statement::returns(value).indented(indent);
        Ok([setup, Some(error.text), Some(result)]
            .into_iter()
            .flatten()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn value_initializer_body<'plan>(
        &self,
        call: Expression,
        error: &ErrorConversion,
        indent: &str,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<String> {
        let setup = self
            .success
            .as_ref()
            .map(|success| success.statement().indented(indent));
        let error = error.body(call.clone(), indent)?;
        let success = self.success.as_ref().map(SuccessSlot::expression);
        let result = match (success, error.consumes_call()) {
            (Some(value), _) => {
                Some(self.value_initializer_body_for_value(value, indent, lexical, scope)?)
            }
            (None, false) => {
                Some(self.value_initializer_body_for_value(call, indent, lexical, scope)?)
            }
            (None, true) => None,
        };
        Ok([setup, Some(error.text), result]
            .into_iter()
            .flatten()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn value_initializer_body_for_value<'plan>(
        &self,
        value: Expression,
        indent: &str,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<String> {
        match &self.conversion {
            ReturnConversion::Encoded(encoded)
                if value == Expression::identifier(encoded.buffer.binding().clone()) =>
            {
                encoded.value_initializer_body_from_buffer(indent, self.optional, lexical, scope)
            }
            ReturnConversion::Encoded(encoded) => {
                encoded.value_initializer_body(value, indent, self.optional, lexical, scope)
            }
            ReturnConversion::DirectVector { .. }
            | ReturnConversion::ClassHandle(_)
            | ReturnConversion::CallbackHandle(_)
            | ReturnConversion::Closure(_) => {
                Err(SwiftHost::unsupported("value initializer return"))
            }
            ReturnConversion::Direct | ReturnConversion::FromC(_) => {
                Self::assign_initializer_value(
                    self.expression(value),
                    indent,
                    self.optional,
                    lexical,
                    scope,
                )
            }
        }
    }

    fn assign_initializer_value<'plan>(
        value: Expression,
        indent: &str,
        optional: bool,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<String> {
        if !optional {
            return Ok(Statement::assign("self", value).indented(indent));
        }
        let declaration =
            lexical.allocate(scope, &GeneratedLocal::ReturnBuffer.suffixed_stem("value"))?;
        let (statement, reference) = lexical
            .declare(declaration, |binding| {
                Statement::let_value(binding, value).indented(indent)
            })
            .into_parts();
        Ok([
            statement,
            Self::assign_initializer_reference(lexical, scope, reference, indent, optional)?,
        ]
        .join("\n"))
    }

    fn assign_initializer_reference<'plan>(
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
        reference: LocalReference<'plan, Syntax>,
        indent: &str,
        optional: bool,
    ) -> Result<String> {
        let source =
            lexical
                .resolve(scope, &reference)
                .cloned()
                .ok_or(Error::UnexpectedBindingShape {
                    layer: "Swift lexical planner",
                    shape: "invisible value initializer local",
                })?;
        if !optional {
            return Ok(Statement::assign("self", source).indented(indent));
        }
        let continuation = lexical.child(scope, ScopeForm::GuardContinuation);
        let declaration =
            lexical
                .shadow(continuation, &reference)
                .ok_or(Error::UnexpectedBindingShape {
                    layer: "Swift lexical planner",
                    shape: "invalid value initializer guard binding",
                })?;
        let (guard, guarded) = lexical
            .declare(declaration, |binding| {
                format!("{indent}guard let {binding} = {source} else {{ return nil }}")
            })
            .into_parts();
        let value =
            lexical
                .resolve(continuation, &guarded)
                .ok_or(Error::UnexpectedBindingShape {
                    layer: "Swift lexical planner",
                    shape: "invisible guarded value initializer local",
                })?;
        Ok([guard, Statement::assign("self", value).indented(indent)].join("\n"))
    }

    fn factory_body(
        &self,
        call: Expression,
        error: &ErrorConversion,
        indent: &str,
    ) -> Result<String> {
        let setup = self
            .success
            .as_ref()
            .map(|success| success.statement().indented(indent));
        let error = error.body(call.clone(), indent)?;
        let success = self.success.as_ref().map(SuccessSlot::expression);
        let result = match (success, error.consumes_call()) {
            (Some(value), _) => Some(self.factory_body_for_value(value, indent)?),
            (None, false) => Some(self.factory_body_for_value(call, indent)?),
            (None, true) => None,
        };
        Ok([setup, Some(error.text), result]
            .into_iter()
            .flatten()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn factory_body_for_value(&self, value: Expression, indent: &str) -> Result<String> {
        match &self.conversion {
            ReturnConversion::ClassHandle(handle) => handle.factory_body(value, indent),
            _ => self.body_for_value(value, indent),
        }
    }

    fn statement(&self, call: Expression) -> Statement {
        match &self.ty {
            Some(_) => Statement::returns(self.expression(call)),
            None => Statement::expression(call),
        }
    }

    fn expression(&self, call: Expression) -> Expression {
        match &self.conversion {
            ReturnConversion::Direct => call,
            ReturnConversion::FromC(ty) => Expression::call(
                ty,
                [Expression::labeled("fromC", call)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
            ReturnConversion::Encoded(_) => call,
            ReturnConversion::DirectVector { .. } => call,
            ReturnConversion::ClassHandle(handle) => handle.wrap(call),
            ReturnConversion::CallbackHandle(handle) => handle.wrap(call),
            ReturnConversion::Closure(_) => call,
        }
    }
}

impl ClassHandle {
    fn new(id: ClassId, presence: HandlePresence, context: &RenderContext<Native>) -> Result<Self> {
        Ok(Self {
            ty: SwiftType::class(id, context)?,
            presence,
        })
    }

    fn api_type(&self) -> TypeName {
        match self.presence {
            HandlePresence::Required => self.ty.clone(),
            HandlePresence::Nullable => self.ty.clone().optional(),
            _ => self.ty.clone(),
        }
    }

    fn parameter_argument(&self, value: Expression) -> Expression {
        match self.presence {
            HandlePresence::Required => Expression::member(value, "handle"),
            HandlePresence::Nullable => Expression::nil_coalescing(
                Expression::member(Expression::new(format!("{value}?")), "handle"),
                Self::empty(),
            ),
            _ => value,
        }
    }

    fn wrap(&self, handle: Expression) -> Expression {
        let wrapped = Expression::call(
            &self.ty,
            [Expression::labeled("handle", handle.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        match self.presence {
            HandlePresence::Required => wrapped,
            HandlePresence::Nullable => Expression::conditional(
                Expression::equal(&handle, Self::empty()),
                Expression::nil(),
                wrapped,
            ),
            _ => wrapped,
        }
    }

    fn body(&self, handle: Expression, indent: &str) -> Result<String> {
        match self.presence {
            HandlePresence::Required => Ok(Statement::returns(self.wrap(handle)).indented(indent)),
            HandlePresence::Nullable => {
                let binding = GeneratedLocal::ReturnHandle.identifier()?;
                let value = Expression::identifier(binding.clone());
                Ok([
                    Statement::let_value(&binding, handle).indented(indent),
                    Statement::returns(self.wrap(value)).indented(indent),
                ]
                .join("\n"))
            }
            _ => Ok(Statement::returns(self.wrap(handle)).indented(indent)),
        }
    }

    fn factory_body(&self, handle: Expression, indent: &str) -> Result<String> {
        if self.presence != HandlePresence::Required {
            return Err(SwiftHost::unsupported("nullable class initializer"));
        }
        Ok(Statement::returns(Expression::call(
            "Self",
            [Expression::labeled("handle", handle)]
                .into_iter()
                .collect::<ArgumentList>(),
        ))
        .indented(indent))
    }

    fn empty() -> Expression {
        Expression::new("0")
    }
}

impl EncodedReturn {
    fn new(decode: Expression, bridge: &CBridgeContract) -> Result<Self> {
        Ok(Self {
            buffer: OwnedBuffer::new(GeneratedLocal::ReturnBuffer.identifier()?),
            reader: GeneratedLocal::WireReader.identifier()?,
            decode,
            free: Identifier::parse(bridge.support().buffer_free()?.name())?,
        })
    }

    fn body(&self, call: Expression, indent: &str) -> Result<String> {
        Ok([
            Statement::let_value(self.buffer.binding(), call).indented(indent),
            self.body_from_buffer(indent)?,
        ]
        .join("\n"))
    }

    fn value_initializer_body<'plan>(
        &self,
        call: Expression,
        indent: &str,
        optional: bool,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<String> {
        Ok([
            Statement::let_value(self.buffer.binding(), call).indented(indent),
            self.value_initializer_body_from_buffer(indent, optional, lexical, scope)?,
        ]
        .join("\n"))
    }

    fn value_initializer_body_from_buffer<'plan>(
        &self,
        indent: &str,
        optional: bool,
        lexical: &mut LexicalPlan<'plan, Syntax>,
        scope: Scope<'plan, Syntax>,
    ) -> Result<String> {
        let decode_call = self.buffer.decode(&self.reader, &self.decode)?;
        Ok([
            Statement::defer(Expression::call(
                &self.free,
                [Expression::identifier(self.buffer.binding().clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))
            .indented(indent),
            Return::assign_initializer_value(decode_call, indent, optional, lexical, scope)?,
        ]
        .join("\n"))
    }

    fn body_from_buffer(&self, indent: &str) -> Result<String> {
        let decode_call = self.buffer.decode(&self.reader, &self.decode)?;
        Ok([
            Statement::defer(Expression::call(
                &self.free,
                [Expression::identifier(self.buffer.binding().clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))
            .indented(indent),
            Statement::returns(decode_call).indented(indent),
        ]
        .join("\n"))
    }
}

impl ReturnedClosure {
    fn new(
        closure: &ClosureReturn<Native, OutOfRust>,
        returned: &ClosureReturnParameter,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let parameters = closure
            .invoke()
            .params()
            .iter()
            .map(|parameter| Parameter::from_decl(parameter, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let error = ErrorConversion::from_closure_channel(
            closure.invoke().error().channel(),
            bridge,
            context,
        )?;
        let c_return_channel = match error.fallible() {
            true => ReturnChannel::EncodedError,
            false => ReturnChannel::Value,
        };
        let returns = closure
            .invoke()
            .returns()
            .plan()
            .render_with(&mut ReturnPlan {
                bridge,
                context,
                c_return_channel,
                parameter_groups: returned.parameter_groups(),
            })?;
        let storage = TypeName::new(format!(
            "BoltFFIReturnedClosure{}",
            closure.signature().as_str()
        ));
        let owner = TypeName::new(format!("{storage}Owner"));
        let public_ty = Self::public_type(closure.presence(), &parameters, &returns, &error);
        Ok(Self {
            storage,
            owner,
            storage_binding: GeneratedLocal::ReturnBuffer.identifier()?,
            owner_binding: GeneratedLocal::ReturnBuffer.suffixed("owner")?,
            presence: closure.presence(),
            parameters,
            returns,
            error,
            call_type: SwiftType::c_type(returned.call_type())?,
            public_ty,
        })
    }

    fn public_type(
        presence: HandlePresence,
        parameters: &[Parameter],
        returns: &Return,
        error: &ErrorConversion,
    ) -> TypeName {
        let ty = match error.fallible() {
            true => TypeName::throwing_closure(
                parameters.iter().map(|parameter| parameter.ty.clone()),
                returns.ty.clone(),
            ),
            false => TypeName::closure(
                parameters.iter().map(|parameter| parameter.ty.clone()),
                returns.ty.clone(),
            ),
        };
        match presence {
            HandlePresence::Nullable => ty.optional(),
            _ => ty,
        }
    }

    fn argument(&self) -> Expression {
        Expression::address(&self.storage_binding)
    }

    fn body(
        &self,
        call: Expression,
        function_error: &ErrorConversion,
        indent: &str,
    ) -> Result<String> {
        let error = function_error.body(call.clone(), indent)?;
        let status = match error.consumes_call() {
            true => None,
            false => Some(self.status_check(call, indent)?),
        };
        let owner = Statement::let_value(
            &self.owner_binding,
            Expression::call(
                &self.owner,
                [Expression::identifier(self.storage_binding.clone())]
                    .into_iter()
                    .collect(),
            ),
        )
        .indented(indent);
        Ok([
            self.storage_declaration(indent)?,
            self.owner_declaration(indent)?,
            Statement::var_value(
                &self.storage_binding,
                &self.storage,
                Expression::call(&self.storage, ArgumentList::default()),
            )
            .indented(indent),
            error.text,
            status.unwrap_or_default(),
            self.nullable_return(indent),
            owner,
            self.closure_statement(indent)?,
        ]
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
    }

    fn requires_wire_runtime(&self) -> bool {
        self.parameters
            .iter()
            .any(|parameter| parameter.argument.requires_wire_runtime())
            || self.returns.requires_wire_runtime()
            || self.error.requires_wire_runtime()
    }

    fn storage_declaration(&self, indent: &str) -> Result<String> {
        let invoke = Identifier::parse("invoke")?;
        let context = Identifier::parse("context")?;
        let release = Identifier::parse("release")?;
        let raw_pointer = TypeName::new("UnsafeMutableRawPointer");
        let release_type = TypeName::new("@convention(c) (UnsafeMutableRawPointer?) -> Void")
            .optional_function_pointer();
        Ok(Statement::structure(
            &self.storage,
            [
                Statement::var_nil(invoke, self.call_type.clone().optional_function_pointer()),
                Statement::var_nil(context, raw_pointer.optional()),
                Statement::var_nil(release, release_type),
            ],
        )
        .indented(indent))
    }

    fn owner_declaration(&self, indent: &str) -> Result<String> {
        let storage = Identifier::parse("storage")?;
        let release = Identifier::parse("release")?;
        let context = Identifier::parse("context")?;
        Ok(Statement::final_class(
            &self.owner,
            [
                Statement::stored_var(&storage, &self.storage),
                Statement::initializer(
                    &storage,
                    &self.storage,
                    [Statement::assign(
                        Expression::member("self", &storage),
                        Expression::identifier(storage.clone()),
                    )],
                ),
                Statement::deinitializer([Statement::expression(Expression::optional_call(
                    Expression::member(&storage, release),
                    [Expression::member(&storage, context)]
                        .into_iter()
                        .collect(),
                ))]),
            ],
        )
        .indented(indent))
    }

    fn status_check(&self, call: Expression, indent: &str) -> Result<String> {
        let status = GeneratedLocal::ClosureStatus.identifier()?;
        let code = Expression::member(&status, "code");
        Ok([
            Statement::let_value(&status, call).indented(indent),
            Statement::guard_else(
                Expression::equal(&code, Expression::literal(Literal::integer(0))),
                [Statement::fatal_error(Literal::interpolated(
                    "returned closure registration failed with code ",
                    code,
                    "",
                ))],
            )
            .indented(indent),
        ]
        .join("\n"))
    }

    fn nullable_return(&self, indent: &str) -> String {
        match self.presence {
            HandlePresence::Nullable => Statement::guard_else(
                Expression::not_equal(
                    Expression::member(&self.storage_binding, "invoke"),
                    Expression::nil(),
                ),
                [Statement::returns(Expression::nil())],
            )
            .indented(indent),
            _ => String::new(),
        }
    }

    fn closure_statement(&self, indent: &str) -> Result<String> {
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        let body = self.invoke_body(&format!("{indent}    "))?;
        Ok(Statement::returning_closure(parameters, body, indent))
    }

    fn invoke_body(&self, indent: &str) -> Result<String> {
        let invoke = GeneratedLocal::ClosureInvoke.identifier()?;
        let storage = Expression::member(&self.owner_binding, "storage");
        let call = Expression::call(
            Expression::identifier(invoke.clone()),
            std::iter::once(Expression::member(&storage, "context"))
                .chain(
                    self.parameters
                        .iter()
                        .map(Parameter::argument)
                        .flat_map(|argument| argument.arguments()),
                )
                .chain(self.returns.arguments())
                .collect(),
        );
        Ok([
            Statement::guard_let(
                &invoke,
                Expression::member(&storage, "invoke"),
                [Statement::fatal_error(Literal::string(
                    "returned closure was released",
                ))],
            )
            .indented(indent),
            Invocation::render_scoped_body(
                &self.arguments(),
                &self.returns,
                &self.error,
                call,
                indent,
                self.returns.exit(self.error.fallible()),
            )?,
        ]
        .join("\n"))
    }

    fn arguments(&self) -> Vec<Argument> {
        self.parameters.iter().map(Parameter::argument).collect()
    }
}

impl SuccessSlot {
    fn new(ty: TypeName) -> Result<Self> {
        Ok(Self {
            binding: GeneratedLocal::ReturnBuffer.identifier()?,
            ty,
        })
    }

    fn statement(&self) -> Statement {
        Statement::var_value(
            &self.binding,
            &self.ty,
            Expression::call(&self.ty, ArgumentList::default()),
        )
    }

    fn argument(&self) -> Expression {
        Expression::address(&self.binding)
    }

    fn expression(&self) -> Expression {
        Expression::identifier(self.binding.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ErrorBody {
    text: String,
    consumes_call: bool,
}

impl ErrorBody {
    fn empty() -> Self {
        Self {
            text: String::new(),
            consumes_call: false,
        }
    }

    fn consumes_call(&self) -> bool {
        self.consumes_call
    }
}

impl ReturnSignature {
    pub fn signature(&self) -> String {
        match (&self.ty, self.fallible) {
            (Some(ty), true) => format!(" throws -> {ty}"),
            (Some(ty), false) => format!(" -> {ty}"),
            (None, true) => " throws".to_owned(),
            (None, false) => String::new(),
        }
    }

    pub fn type_name(&self) -> Option<&TypeName> {
        self.ty.as_ref()
    }

    pub fn fallible(&self) -> bool {
        self.fallible
    }
}

impl ErrorConversion {
    fn from_channel(
        channel: ErrorChannel<'_, Native, OutOfRust>,
        function: &CFunction,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match channel {
            ErrorChannel::None => {
                if function.return_channel() == ReturnChannel::EncodedError {
                    Err(Error::BrokenBridgeContract {
                        bridge: SwiftHost::TARGET,
                        invariant: "C return channel carries an error for an infallible callable",
                    })
                } else {
                    Ok(Self::None)
                }
            }
            ErrorChannel::Status => Err(SwiftHost::unsupported("status error channel")),
            ErrorChannel::Encoded {
                placement,
                ty,
                codec,
                shape,
            } => {
                if placement != ErrorPlacement::ReturnSlot {
                    return Err(SwiftHost::unsupported("error out pointer"));
                }
                if function.return_channel() != ReturnChannel::EncodedError {
                    return Err(Error::BrokenBridgeContract {
                        bridge: SwiftHost::TARGET,
                        invariant: "encoded error does not use the C return slot",
                    });
                }
                let reader = GeneratedLocal::ErrorReader.identifier()?;
                Self::encoded(ty, codec, shape, reader, bridge, context)
            }
            _ => Err(SwiftHost::unsupported("unknown error channel")),
        }
    }

    fn from_closure_channel(
        channel: ErrorChannel<'_, Native, OutOfRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match channel {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Status => Err(SwiftHost::unsupported("status error channel")),
            ErrorChannel::Encoded {
                placement,
                ty,
                codec,
                shape,
            } => {
                if placement != ErrorPlacement::ReturnSlot {
                    return Err(SwiftHost::unsupported("error out pointer"));
                }
                let reader = GeneratedLocal::ErrorReader.identifier()?;
                Self::encoded(ty, codec, shape, reader, bridge, context)
            }
            _ => Err(SwiftHost::unsupported("unknown error channel")),
        }
    }

    fn encoded(
        ty: &TypeRef,
        codec: &ReadPlan,
        shape: native::BufferShape,
        reader: Identifier,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if shape != native::BufferShape::Buffer {
            return Err(SwiftHost::unsupported("encoded error shape"));
        }
        let decode = codec
            .render_with(&mut Reader::new(reader.clone(), context))
            .map(ReadExpression::into_expression)?;
        Ok(Self::Encoded(EncodedError::new(
            ty, decode, reader, bridge,
        )?))
    }

    fn fallible(&self) -> bool {
        !matches!(self, Self::None)
    }

    fn requires_wire_runtime(&self) -> bool {
        matches!(self, Self::Encoded(_))
    }

    fn body(&self, call: Expression, indent: &str) -> Result<ErrorBody> {
        match self {
            Self::None => Ok(ErrorBody::empty()),
            Self::Encoded(encoded) => encoded.body(call, indent),
        }
    }
}

impl EncodedError {
    fn new(
        ty: &TypeRef,
        decode: Expression,
        reader: Identifier,
        bridge: &CBridgeContract,
    ) -> Result<Self> {
        Ok(Self {
            buffer: OwnedBuffer::new(GeneratedLocal::ErrorBuffer.identifier()?),
            reader,
            decode: Self::throw_expression(ty, decode)?,
            free: Identifier::parse(bridge.support().buffer_free()?.name())?,
        })
    }

    fn body(&self, call: Expression, indent: &str) -> Result<ErrorBody> {
        let decode = self.buffer.decode(&self.reader, &self.decode)?;
        let block = Statement::if_then(
            self.buffer.is_present(),
            [
                Statement::defer(self.buffer.free_call(&self.free)),
                Statement::throwing(decode),
            ],
        );
        Ok(ErrorBody {
            text: [
                Statement::let_value(self.buffer.binding(), call).indented(indent),
                block.indented(indent),
            ]
            .join("\n"),
            consumes_call: true,
        })
    }

    fn throw_expression(ty: &TypeRef, decode: Expression) -> Result<Expression> {
        match ty {
            TypeRef::String => Ok(Expression::call(
                "FfiError",
                [Expression::labeled("message", decode)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )),
            TypeRef::Record(_) | TypeRef::Enum(_) => Ok(decode),
            _ => Err(SwiftHost::unsupported("encoded error type")),
        }
    }
}

struct ReturnPlan<'context, 'bindings> {
    bridge: &'context CBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
    c_return_channel: ReturnChannel,
    parameter_groups: &'context [ParameterGroup],
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for ReturnPlan<'_, '_> {
    type Output = Result<Return>;

    fn void(&mut self) -> Self::Output {
        Ok(Return {
            ty: None,
            optional: false,
            conversion: ReturnConversion::Direct,
            success: None,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        let direct = DirectValue::new(ty, self.bridge, self.context)?;
        let conversion = match direct.converts_from_c() {
            true => ReturnConversion::FromC(direct.api_type().clone()),
            false => ReturnConversion::Direct,
        };
        Ok(Return {
            ty: Some(direct.api_type().clone()),
            optional: false,
            conversion,
            success: self.success_slot(slot, direct.storage_type().clone())?,
        })
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        if shape != native::BufferShape::Buffer {
            return Err(SwiftHost::unsupported("encoded return shape"));
        }
        let reader = GeneratedLocal::WireReader.identifier()?;
        let decode = codec
            .render_with(&mut Reader::new(reader.clone(), self.context))
            .map(ReadExpression::into_expression)?;
        Ok(Return {
            ty: Some(SwiftType::type_ref(ty, self.context)?),
            optional: matches!(ty, TypeRef::Optional(_)),
            conversion: ReturnConversion::Encoded(EncodedReturn::new(decode, self.bridge)?),
            success: self.success_slot(slot, TypeName::new("FfiBuf_u8"))?,
        })
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(*class, presence, self.context)?;
                Ok(Return {
                    ty: Some(handle.api_type()),
                    optional: presence == HandlePresence::Nullable,
                    conversion: ReturnConversion::ClassHandle(handle),
                    success: self.success_slot(slot, SwiftType::handle_carrier(carrier)?)?,
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::from_rust_handle(*callback, presence, self.context)?;
                Ok(Return {
                    ty: Some(handle.api_type()),
                    optional: presence == HandlePresence::Nullable,
                    conversion: ReturnConversion::CallbackHandle(handle),
                    success: self.success_slot(slot, TypeName::new("BoltFFICallbackHandle"))?,
                })
            }
            HandleTarget::Stream(_) => Err(SwiftHost::unsupported("stream handle return")),
            _ => Err(SwiftHost::unsupported("unknown handle return")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let reader = GeneratedLocal::WireReader.identifier()?;
        Ok(Return {
            ty: Some(SwiftPrimitive::new(primitive).api_type()?.optional()),
            optional: true,
            conversion: ReturnConversion::Encoded(EncodedReturn::new(
                ScalarOption::new(primitive).read(reader)?,
                self.bridge,
            )?),
            success: None,
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let vector = DirectVector::from_element(element, self.bridge, self.context)?.returned();
        Ok(Return {
            ty: Some(vector.ty()),
            optional: false,
            conversion: ReturnConversion::DirectVector {
                vector,
                free: Identifier::parse(self.bridge.support().buffer_free()?.name())?,
            },
            success: None,
        })
    }

    fn closure(&mut self, closure: &'plan ClosureReturn<Native, OutOfRust>) -> Self::Output {
        let returned = self
            .parameter_groups
            .iter()
            .find_map(|group| match group {
                ParameterGroup::ClosureReturn(returned) => Some(returned),
                _ => None,
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing C closure return parameter for Swift closure return",
            })?;
        let closure = ReturnedClosure::new(closure, returned, self.bridge, self.context)?;
        Ok(Return {
            ty: Some(closure.public_ty.clone()),
            optional: closure.presence == HandlePresence::Nullable,
            conversion: ReturnConversion::Closure(Box::new(closure)),
            success: None,
        })
    }
}

impl ReturnPlan<'_, '_> {
    fn success_slot(&self, slot: ReturnValueSlot, ty: TypeName) -> Result<Option<SuccessSlot>> {
        match (slot, self.c_return_channel) {
            (ReturnValueSlot::ReturnSlot, ReturnChannel::Value) => Ok(None),
            (ReturnValueSlot::OutPointer, ReturnChannel::EncodedError) => {
                SuccessSlot::new(ty).map(Some)
            }
            (ReturnValueSlot::OutPointer, ReturnChannel::Value) => {
                Err(SwiftHost::unsupported("out pointer return"))
            }
            (ReturnValueSlot::ReturnSlot, ReturnChannel::EncodedError) => {
                Err(Error::BrokenBridgeContract {
                    bridge: SwiftHost::TARGET,
                    invariant: "error return channel without success out pointer",
                })
            }
            _ => Err(SwiftHost::unsupported("unknown return slot")),
        }
    }
}
