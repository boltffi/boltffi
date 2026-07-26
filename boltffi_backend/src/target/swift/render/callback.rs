use std::collections::BTreeSet;

use askama::Template;
use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, CallbackProtocolIntrospect, ClosureParameter,
    ClosureReturn, DirectValueType, DirectVectorElementType, Direction, ErrorChannel,
    ErrorPlacement, ExecutionDecl, ExportedCallable, HandlePresence, HandleTarget,
    ImportedCallable, ImportedMethodDecl, IncomingParam, IntoRust, Native, OutOfRust,
    OutgoingParam, ParamDecl, ParamPlanRender, Primitive, Receive, ReturnPlanRender,
    ReturnValueSlot, Surface, TypeRef, VTableSlot, native,
};

use crate::{
    bridge::c::{CBridgeContract, CallbackSlot, ParameterGroup, Type as CType},
    core::{Emitted, Error, RenderContext, Result},
    target::swift::{
        SwiftHost,
        c_abi::{BorrowedVector, CopiedVector, DirectValue, DirectVector, ReturnedVector},
        codec::{
            ArgumentBuffer, OwnedBuffer, ReadExpression, Reader, ScalarOption, WriteStatement,
            Writer,
        },
        name_style::{GeneratedLocal, Name},
        render::{Documentation, SwiftType, function::AssociatedFunction},
        syntax::{ArgumentList, Expression, Identifier, ParameterList, Statement, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/callback.swift", escape = "none")]
struct CallbackTemplate<'callback> {
    callback: &'callback Callback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Callback {
    documentation: Documentation,
    name: TypeName,
    wrapper: TypeName,
    proxy: TypeName,
    bridge: TypeName,
    bridgeable: TypeName,
    vtable: Identifier,
    vtable_type: TypeName,
    register: Identifier,
    create_handle: Identifier,
    methods: Vec<Method>,
    requires_wire_runtime: bool,
    proxy_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackHandle {
    ty: TypeName,
    bridge: TypeName,
    presence: CallbackPresence,
    optional_callback: Identifier,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallbackPresence {
    Required,
    Nullable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Method {
    name: Identifier,
    slot: Identifier,
    wrapper: TypeName,
    vtable: TypeName,
    execution: MethodExecution,
    return_bindings: Vec<Identifier>,
    parameters: Vec<Parameter>,
    returns: Return,
    error: CallbackError,
    requires_wire_runtime: bool,
    proxy_body: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MethodExecution {
    Synchronous,
    Asynchronous(AsyncCompletion),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsyncCompletion {
    callback: Identifier,
    context: Identifier,
    payload: Option<CType>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Parameter {
    name: Identifier,
    ty: TypeName,
    bindings: Vec<Identifier>,
    setup: Vec<Statement>,
    argument: Expression,
    proxy_arguments: Vec<Expression>,
    proxy_scopes: Vec<ProxyArgumentScope>,
    requires_wire_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Return {
    ty: Option<TypeName>,
    conversion: ReturnConversion,
    result: Identifier,
    success_out: Option<Identifier>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReturnConversion {
    Void,
    Direct(DirectReturn),
    Encoded(EncodedReturn),
    DirectVector(DirectVectorReturn),
    CallbackHandle(CallbackHandle),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectReturn {
    value: DirectValue,
    encoded_completion: Option<EncodedDirectCompletion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedDirectCompletion {
    buffer: ArgumentBuffer,
    copy: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectVectorReturn {
    copied: CopiedVector,
    returned: ReturnedVector,
    free: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedReturn {
    result: Identifier,
    buffer: ArgumentBuffer,
    copy: Identifier,
    proxy: EncodedProxyReturn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedProxyReturn {
    buffer: OwnedBuffer,
    decode: Expression,
    free: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedProxyArgument {
    buffer: ArgumentBuffer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProxyArgumentScope {
    Encoded(EncodedProxyArgument),
    DirectVector(BorrowedVector),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CallbackError {
    None,
    Encoded(EncodedCallbackError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedCallbackError {
    buffer: ArgumentBuffer,
    copy: Identifier,
}

struct ParameterPlan<'context, 'bindings> {
    source_name: Name,
    name: Identifier,
    group: &'context ParameterGroup,
    slot: &'context CallbackSlot,
    bridge: &'context CBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

struct ReturnPlan<'context, 'bindings> {
    bridge: &'context CBridgeContract,
    slot: &'context CallbackSlot,
    fallible: bool,
    asynchronous: bool,
    context: &'context RenderContext<'bindings, Native>,
}

struct ProxyParameterSupport<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
}

struct ProxyReturnSupport<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ProxyRequirements {
    callbacks: BTreeSet<CallbackId>,
}

impl Callback {
    pub fn from_declaration(
        declaration: &CallbackDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_callback =
            bridge
                .source_callback(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: SwiftHost::TARGET,
                    invariant: "missing C callback protocol for Swift callback",
                })?;
        let source_methods = declaration.protocol().vtable().methods();
        if source_methods.len() != c_callback.methods().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "callback method count does not match C callback protocol",
            });
        }
        let name = Name::new(declaration.name());
        let type_name = name.type_name();
        let wrapper = TypeName::new(format!("{type_name}Wrapper"));
        let vtable_type = TypeName::new(c_callback.vtable().name());
        let proxy_required = ProxyRequirements::from_context(context).contains(declaration.id());
        let methods = source_methods
            .iter()
            .zip(c_callback.methods())
            .map(|(source, slot)| {
                Method::from_declaration(
                    source,
                    slot,
                    wrapper.clone(),
                    vtable_type.clone(),
                    bridge,
                    context,
                    proxy_required,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let requires_wire_runtime = methods.iter().any(Method::requires_wire_runtime);
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), ""),
            wrapper: wrapper.clone(),
            proxy: TypeName::new(format!("{type_name}Proxy")),
            bridge: TypeName::new(format!("{type_name}Bridge")),
            bridgeable: TypeName::new(format!("{type_name}Bridgeable")),
            vtable: name.generated("vtable")?,
            vtable_type,
            register: Identifier::parse(c_callback.register().name())?,
            create_handle: Identifier::parse(c_callback.create_handle().name())?,
            methods,
            requires_wire_runtime,
            proxy_required,
            name: type_name,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = CallbackTemplate { callback: self }.render()?;
        source.push_str("\n\n");
        let emitted = Emitted::primary(source);
        let emitted = match self.requires_wire_runtime {
            true => emitted.with_aux(AssociatedFunction::wire_helper()?),
            false => emitted,
        };
        Ok(emitted)
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &TypeName {
        &self.name
    }

    fn wrapper(&self) -> &TypeName {
        &self.wrapper
    }

    fn proxy(&self) -> &TypeName {
        &self.proxy
    }

    fn bridge(&self) -> &TypeName {
        &self.bridge
    }

    fn bridgeable(&self) -> &TypeName {
        &self.bridgeable
    }

    fn vtable(&self) -> &Identifier {
        &self.vtable
    }

    fn vtable_type(&self) -> &TypeName {
        &self.vtable_type
    }

    fn register(&self) -> &Identifier {
        &self.register
    }

    fn create_handle(&self) -> &Identifier {
        &self.create_handle
    }

    fn methods(&self) -> &[Method] {
        &self.methods
    }

    fn proxy_required(&self) -> bool {
        self.proxy_required
    }
}

impl CallbackHandle {
    pub fn new(
        callback: CallbackId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let ty = SwiftType::callback(callback, context)?;
        let presence = CallbackPresence::new(presence)?;
        Ok(Self {
            bridge: TypeName::new(format!("{ty}Bridge")),
            ty,
            presence,
            optional_callback: Identifier::parse("boltffiCallback")?,
        })
    }

    pub fn from_rust_handle(
        callback: CallbackId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::validate_proxy(callback, context)?;
        Self::new(callback, presence, context)
    }

    pub fn api_type(&self) -> TypeName {
        match self.presence {
            CallbackPresence::Required => self.ty.clone(),
            CallbackPresence::Nullable => self.ty.clone().optional(),
        }
    }

    pub fn c_handle(&self, value: Expression) -> Expression {
        match self.presence {
            CallbackPresence::Required | CallbackPresence::Nullable => self.create(value),
        }
    }

    pub fn wrap(&self, handle: Expression) -> Expression {
        let wrapped = Expression::call(
            Expression::member(&self.bridge, "wrap"),
            [handle.clone()].into_iter().collect::<ArgumentList>(),
        );
        match self.presence {
            CallbackPresence::Required => wrapped,
            CallbackPresence::Nullable => Expression::conditional(
                Expression::equal(Expression::member(&handle, "handle"), "0"),
                Expression::nil(),
                wrapped,
            ),
        }
    }

    pub fn body(&self, handle: Expression, indent: &str) -> Result<String> {
        match self.presence {
            CallbackPresence::Required => {
                Ok(Statement::returns(self.wrap(handle)).indented(indent))
            }
            CallbackPresence::Nullable => {
                let binding = GeneratedLocal::ReturnHandle.identifier()?;
                let value = Expression::identifier(binding.clone());
                Ok([
                    Statement::let_value(&binding, handle).indented(indent),
                    Statement::returns(self.wrap(value)).indented(indent),
                ]
                .join("\n"))
            }
        }
    }

    fn create(&self, value: Expression) -> Expression {
        match self.presence {
            CallbackPresence::Required => Expression::call(
                Expression::member(&self.bridge, "create"),
                [value].into_iter().collect::<ArgumentList>(),
            ),
            CallbackPresence::Nullable => {
                let callback = self.optional_callback.clone();
                Expression::nil_coalescing(
                    Expression::map(
                        value,
                        callback.clone(),
                        Expression::call(
                            Expression::member(&self.bridge, "create"),
                            [Expression::identifier(callback)]
                                .into_iter()
                                .collect::<ArgumentList>(),
                        ),
                    ),
                    Self::empty(),
                )
            }
        }
    }

    fn empty() -> Expression {
        Expression::call(
            "BoltFFICallbackHandle",
            [
                Expression::labeled("handle", "0"),
                Expression::labeled("vtable", Expression::nil()),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )
    }

    fn validate_proxy(callback: CallbackId, context: &RenderContext<Native>) -> Result<()> {
        let declaration = context
            .callback(callback)
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing callback type for Swift callback handle",
            })?;
        declaration
            .protocol()
            .vtable()
            .methods()
            .iter()
            .try_for_each(|method| Method::validate_proxy(method, context))
    }
}

impl CallbackPresence {
    fn new(presence: HandlePresence) -> Result<Self> {
        match presence {
            HandlePresence::Required => Ok(Self::Required),
            HandlePresence::Nullable => Ok(Self::Nullable),
            _ => Err(SwiftHost::unsupported("unknown callback handle presence")),
        }
    }
}

impl ProxyRequirements {
    fn from_context(context: &RenderContext<Native>) -> Self {
        let mut requirements = Self::default();
        requirements.collect(context.bindings());
        requirements.collect_required_proxy_returns(context);
        requirements
    }

    fn contains(&self, callback: CallbackId) -> bool {
        self.callbacks.contains(&callback)
    }

    fn collect(&mut self, bindings: &Bindings<Native>) {
        bindings.decls().iter().for_each(|declaration| {
            declaration
                .exported_callables()
                .for_each(|callable| self.collect_exported_callable(callable));
            declaration
                .imported_callables()
                .for_each(|callable| self.collect_imported_callable_params(callable));
        });
    }

    fn collect_required_proxy_returns(&mut self, context: &RenderContext<Native>) {
        let mut collected = None;
        while collected != Some(self.callbacks.len()) {
            collected = Some(self.callbacks.len());
            self.callbacks
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|callback| context.callback(callback))
                .flat_map(|callback| callback.protocol().method_callables())
                .for_each(|callable| self.collect_imported_callable_return(callable));
        }
    }

    fn collect_exported_callable(&mut self, callable: &ExportedCallable<Native>) {
        callable.params().iter().for_each(|parameter| {
            if let IncomingParam::Closure(closure) = parameter.payload() {
                self.collect_incoming_closure(closure);
            }
        });
        callable.returns().plan().render_with(self);
    }

    fn collect_imported_callable_params(&mut self, callable: &ImportedCallable<Native>) {
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                OutgoingParam::Value(plan) => plan.render_with(self),
                OutgoingParam::Closure(closure) => self.collect_outgoing_closure(closure),
            });
    }

    fn collect_imported_callable_return(&mut self, callable: &ImportedCallable<Native>) {
        callable.returns().plan().render_with(self);
    }

    fn collect_incoming_closure(&mut self, closure: &ClosureParameter<Native, IntoRust>) {
        self.collect_imported_callable_params(closure.invoke());
        self.collect_imported_callable_return(closure.invoke());
    }

    fn collect_outgoing_closure(&mut self, closure: &ClosureParameter<Native, OutOfRust>) {
        self.collect_exported_callable(closure.invoke());
    }
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ProxyRequirements {
    type Output = ();

    fn direct(&mut self, _: &'plan DirectValueType, _: ()) -> Self::Output {}

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan <OutOfRust as Direction>::Codec,
        _: <Native as Surface>::BufferShape,
        _: (),
    ) -> Self::Output {
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        if let HandleTarget::Callback(callback) = target {
            self.callbacks.insert(*callback);
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType, _: ()) -> Self::Output {}
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for ProxyRequirements {
    type Output = ();

    fn void(&mut self) -> Self::Output {}

    fn direct(&mut self, _: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {}

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan <OutOfRust as Direction>::Codec,
        _: <Native as Surface>::BufferShape,
    ) -> Self::Output {
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        if let HandleTarget::Callback(callback) = target {
            self.callbacks.insert(*callback);
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {}

    fn closure(&mut self, closure: &'plan ClosureReturn<Native, OutOfRust>) -> Self::Output {
        self.collect_exported_callable(closure.invoke());
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ProxyRequirements {
    type Output = ();

    fn void(&mut self) -> Self::Output {}

    fn direct(&mut self, _: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {}

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan <IntoRust as Direction>::Codec,
        _: <Native as Surface>::BufferShape,
    ) -> Self::Output {
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        if let HandleTarget::Callback(callback) = target {
            self.callbacks.insert(*callback);
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {}

    fn closure(&mut self, closure: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        self.collect_imported_callable_params(closure.invoke());
        self.collect_imported_callable_return(closure.invoke());
    }
}

impl Method {
    fn from_declaration(
        source: &ImportedMethodDecl<Native, VTableSlot>,
        slot: &CallbackSlot,
        wrapper: TypeName,
        vtable: TypeName,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
        proxy_required: bool,
    ) -> Result<Self> {
        let execution = MethodExecution::from_declaration(source, slot)?;
        let error =
            CallbackError::from_channel(source.callable().error().channel(), bridge, context)?;
        let parameter_groups = slot.source_parameter_groups();
        if source.callable().params().len() != parameter_groups.len() {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "callback method parameter count does not match C callback slot",
            });
        }
        let parameters = source
            .callable()
            .params()
            .iter()
            .zip(parameter_groups)
            .map(|(parameter, group)| {
                Parameter::from_declaration(parameter, group, slot, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let returns = source
            .callable()
            .returns()
            .plan()
            .render_with(&mut ReturnPlan {
                bridge,
                slot,
                fallible: error.fallible(),
                asynchronous: execution.asynchronous(),
                context,
            })?;
        let requires_wire_runtime = parameters.iter().any(Parameter::requires_wire_runtime)
            || returns.requires_wire_runtime()
            || error.requires_wire_runtime();
        let mut method = Self {
            name: Name::new(source.name()).function()?,
            slot: Identifier::parse(slot.name().as_str())?,
            wrapper,
            vtable,
            execution,
            return_bindings: returns.bindings(),
            parameters,
            returns,
            error,
            requires_wire_runtime,
            proxy_body: String::new(),
        };
        if proxy_required {
            Self::validate_proxy(source, context)?;
            method.proxy_body = method.render_proxy_body()?;
        }
        Ok(method)
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn slot(&self) -> &Identifier {
        &self.slot
    }

    fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    fn parameter_list(&self) -> String {
        ParameterList::new(self.parameters.iter().map(Parameter::signature))
            .render("        ", "    ")
    }

    fn return_bindings(&self) -> &[Identifier] {
        &self.return_bindings
    }

    fn completion_bindings(&self) -> Vec<Identifier> {
        self.execution.bindings()
    }

    fn return_signature(&self) -> String {
        self.returns
            .signature(self.error.fallible(), self.execution.asynchronous())
    }

    fn requires_wire_runtime(&self) -> bool {
        self.requires_wire_runtime
    }

    fn proxy_body(&self) -> &str {
        &self.proxy_body
    }

    fn render_proxy_body(&self) -> Result<String> {
        let invocation = self.proxy_invocation();
        let statement = self.returns.proxy_statement(invocation)?;
        let body = self
            .parameters
            .iter()
            .flat_map(Parameter::proxy_scopes)
            .rev()
            .fold(statement.to_string(), |body, scope| {
                scope.wrap(body, self.returns.returns_value())
            });
        Ok([
            format!(
                "guard let vtable = handle.vtable?.assumingMemoryBound(to: {}.self),\n      let invoke = vtable.pointee.{} else {{\n    fatalError(\"missing callback vtable entry\")\n}}",
                self.vtable,
                self.slot
            ),
            body,
        ]
        .join("\n")
        .lines()
        .map(|line| format!("        {line}"))
        .collect::<Vec<_>>()
        .join("\n"))
    }

    fn validate_proxy(
        source: &ImportedMethodDecl<Native, VTableSlot>,
        context: &RenderContext<Native>,
    ) -> Result<()> {
        if matches!(
            source.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Err(SwiftHost::unsupported("async callback proxy method"));
        }
        if !matches!(source.callable().error().channel(), ErrorChannel::None) {
            return Err(SwiftHost::unsupported("fallible callback proxy method"));
        }
        source
            .callable()
            .params()
            .iter()
            .try_for_each(|parameter| {
                parameter
                    .payload()
                    .as_value()
                    .ok_or(SwiftHost::unsupported("callback proxy closure parameter"))?
                    .render_with(&mut ProxyParameterSupport { context })
            })?;
        source
            .callable()
            .returns()
            .plan()
            .render_with(&mut ProxyReturnSupport { context })
    }

    fn proxy_invocation(&self) -> Expression {
        Expression::call(
            "invoke",
            std::iter::once(Expression::member("handle", "handle"))
                .chain(self.parameters.iter().flat_map(Parameter::proxy_arguments))
                .collect::<ArgumentList>(),
        )
    }

    fn body(&self) -> String {
        let call = Expression::call(
            Expression::member("wrapper.impl", &self.name),
            self.parameters
                .iter()
                .map(Parameter::call_argument)
                .collect::<ArgumentList>(),
        );
        let invalid_handle = self.execution.invalid_handle_statement(&self.returns);
        let body = [
            format!("            guard handle != 0 else {{ {invalid_handle} }}"),
            format!(
                "            let wrapper = Unmanaged<{}>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()",
                self.wrapper
            ),
            self.parameters
                .iter()
                .flat_map(Parameter::setup)
                .map(|statement| statement.indented("            "))
                .collect::<Vec<_>>()
                .join("\n"),
            self.execution
                .statement(call, &self.returns, &self.error)
                .indented("            "),
        ];
        body.into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Parameter {
    fn from_declaration(
        declaration: &ParamDecl<Native, OutOfRust>,
        group: &ParameterGroup,
        slot: &CallbackSlot,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source_name = Name::new(declaration.name());
        let name = source_name.parameter()?;
        declaration
            .payload()
            .as_value()
            .ok_or(SwiftHost::unsupported("callback closure parameter"))?
            .render_with(&mut ParameterPlan {
                source_name,
                name,
                group,
                slot,
                bridge,
                context,
            })
    }

    fn signature(&self) -> String {
        format!("{}: {}", self.name, self.ty)
    }

    fn bindings(&self) -> &[Identifier] {
        &self.bindings
    }

    fn setup(&self) -> Vec<Statement> {
        self.setup.clone()
    }

    fn call_argument(&self) -> Expression {
        Expression::labeled(&self.name, &self.argument)
    }

    fn requires_wire_runtime(&self) -> bool {
        self.requires_wire_runtime
    }

    fn proxy_arguments(&self) -> Vec<Expression> {
        self.proxy_arguments.clone()
    }

    fn proxy_scopes(&self) -> &[ProxyArgumentScope] {
        &self.proxy_scopes
    }
}

impl MethodExecution {
    fn from_declaration(
        source: &ImportedMethodDecl<Native, VTableSlot>,
        slot: &CallbackSlot,
    ) -> Result<Self> {
        match source.callable().execution() {
            ExecutionDecl::Synchronous(_) => Ok(Self::Synchronous),
            ExecutionDecl::Asynchronous(_) => {
                AsyncCompletion::from_slot(slot).map(Self::Asynchronous)
            }
            _ => Err(SwiftHost::unsupported("unknown callback execution")),
        }
    }

    fn asynchronous(&self) -> bool {
        matches!(self, Self::Asynchronous(_))
    }

    fn bindings(&self) -> Vec<Identifier> {
        match self {
            Self::Synchronous => Vec::new(),
            Self::Asynchronous(completion) => completion.bindings(),
        }
    }

    fn invalid_handle_statement(&self, returns: &Return) -> Statement {
        match self {
            Self::Synchronous => Statement::expression("fatalError(\"invalid callback handle\")"),
            Self::Asynchronous(completion) => {
                Statement::new(format!("{}; return", completion.failure_statement(returns)))
            }
        }
    }

    fn statement(&self, call: Expression, returns: &Return, error: &CallbackError) -> Statement {
        match self {
            Self::Synchronous => error.statement(call, returns),
            Self::Asynchronous(completion) => Statement::new(format!(
                "_Concurrency.Task {{\n{}\n}}",
                error
                    .async_statement(call, returns, completion)
                    .indented("    ")
            )),
        }
    }
}

impl AsyncCompletion {
    fn from_slot(slot: &CallbackSlot) -> Result<Self> {
        let mut completions = slot
            .parameter_groups()
            .iter()
            .filter_map(|group| match group {
                ParameterGroup::CallbackCompletion(completion) => Some(completion),
                _ => None,
            });
        let completion = completions.next().ok_or(Error::BrokenBridgeContract {
            bridge: SwiftHost::TARGET,
            invariant: "async callback method has no completion parameter group",
        })?;
        if completions.next().is_some() {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "async callback method has more than one completion parameter group",
            });
        }
        Ok(Self {
            callback: Identifier::parse("completion")?,
            context: Identifier::parse("completionContext")?,
            payload: completion.payload().cloned(),
        })
    }

    fn bindings(&self) -> Vec<Identifier> {
        vec![self.callback.clone(), self.context.clone()]
    }

    fn success_statement(&self, payload: Option<Expression>) -> Statement {
        Statement::expression(self.call(Self::success_status(), payload))
    }

    fn failure_statement(&self, returns: &Return) -> Statement {
        Statement::expression(self.call(Self::failure_status(), returns.default_payload(self)))
    }

    fn call(&self, status: Expression, payload: Option<Expression>) -> Expression {
        Expression::optional_call(
            &self.callback,
            std::iter::once(Expression::identifier(self.context.clone()))
                .chain(std::iter::once(status))
                .chain(payload)
                .collect::<ArgumentList>(),
        )
    }

    fn success_status() -> Expression {
        Self::status(0)
    }

    fn failure_status() -> Expression {
        Self::status(1)
    }

    fn status(code: i32) -> Expression {
        Expression::call(
            "FfiStatus",
            [Expression::labeled("code", code)]
                .into_iter()
                .collect::<ArgumentList>(),
        )
    }

    fn expects_buffer_payload(&self) -> bool {
        matches!(self.payload, Some(CType::Buffer))
    }
}

impl Return {
    fn signature(&self, fallible: bool, asynchronous: bool) -> String {
        match (&self.ty, fallible, asynchronous) {
            (Some(ty), true, true) => format!(" async throws -> {ty}"),
            (Some(ty), false, true) => format!(" async -> {ty}"),
            (Some(ty), true, false) => format!(" throws -> {ty}"),
            (Some(ty), false, false) => format!(" -> {ty}"),
            (None, true, true) => " async throws".to_owned(),
            (None, false, true) => " async".to_owned(),
            (None, true, false) => " throws".to_owned(),
            (None, false, false) => String::new(),
        }
    }

    fn statement(&self, call: Expression) -> Statement {
        match &self.conversion {
            ReturnConversion::Void => Statement::expression(call),
            ReturnConversion::Direct(direct) => Statement::returns(direct.c_value(call)),
            ReturnConversion::Encoded(encoded) => encoded.statement(call),
            ReturnConversion::DirectVector(vector) => vector.statement(call),
            ReturnConversion::CallbackHandle(handle) => Statement::returns(handle.create(call)),
        }
    }

    fn requires_wire_runtime(&self) -> bool {
        match &self.conversion {
            ReturnConversion::Direct(direct) => direct.requires_wire_runtime(),
            ReturnConversion::Encoded(_) => true,
            _ => false,
        }
    }

    fn returns_value(&self) -> bool {
        self.ty.is_some()
    }

    fn proxy_statement(&self, call: Expression) -> Result<Statement> {
        match &self.conversion {
            ReturnConversion::Void => Ok(Statement::expression(call)),
            ReturnConversion::Direct(direct) => Ok(Statement::returns(direct.swift_value(call))),
            ReturnConversion::Encoded(encoded) => Ok(encoded.proxy_statement(call)),
            ReturnConversion::DirectVector(vector) => vector.proxy_statement(call),
            ReturnConversion::CallbackHandle(handle) => Ok(Statement::returns(handle.wrap(call))),
        }
    }

    fn bindings(&self) -> Vec<Identifier> {
        self.success_out.iter().cloned().collect()
    }

    fn success_statement(&self, call: Expression) -> Statement {
        let Some(success_out) = &self.success_out else {
            return Statement::new(
                [
                    Statement::expression(call).to_string(),
                    Statement::returns(Self::empty_error()).to_string(),
                ]
                .join("\n"),
            );
        };
        match &self.conversion {
            ReturnConversion::Void => Statement::returns(Self::empty_error()),
            ReturnConversion::Direct(direct) => self.direct_success(
                call,
                success_out,
                direct.c_value(Expression::identifier(self.result.clone())),
            ),
            ReturnConversion::Encoded(encoded) => encoded.success_statement(call, success_out),
            ReturnConversion::DirectVector(vector) => {
                vector.success_statement(call, success_out, Self::empty_error())
            }
            ReturnConversion::CallbackHandle(handle) => {
                let value = handle.create(Expression::identifier(self.result.clone()));
                self.direct_success(call, success_out, value)
            }
        }
    }

    fn completion_success_statement(
        &self,
        call: Expression,
        completion: &AsyncCompletion,
    ) -> Statement {
        match &self.conversion {
            ReturnConversion::Void => Statement::new(
                [
                    Statement::expression(call).to_string(),
                    completion
                        .success_statement(
                            completion.expects_buffer_payload().then(Self::empty_error),
                        )
                        .to_string(),
                ]
                .join("\n"),
            ),
            ReturnConversion::Direct(direct) => {
                direct.completion_statement(call, &self.result, completion)
            }
            ReturnConversion::Encoded(encoded) => encoded.completion_statement(call, completion),
            ReturnConversion::DirectVector(vector) => {
                vector.consume_statement(call, |value| completion.success_statement(Some(value)))
            }
            ReturnConversion::CallbackHandle(handle) => self.direct_completion(
                call,
                completion,
                handle.create(Expression::identifier(self.result.clone())),
            ),
        }
    }

    fn direct_completion(
        &self,
        call: Expression,
        completion: &AsyncCompletion,
        value: Expression,
    ) -> Statement {
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                completion.success_statement(Some(value)).to_string(),
            ]
            .join("\n"),
        )
    }

    fn default_payload(&self, completion: &AsyncCompletion) -> Option<Expression> {
        if completion.expects_buffer_payload() {
            return Some(Self::empty_error());
        }
        match &self.conversion {
            ReturnConversion::Void => None,
            ReturnConversion::Direct(direct) => Some(direct.default_storage_value()),
            ReturnConversion::Encoded(_) | ReturnConversion::DirectVector(_) => {
                Some(Self::empty_error())
            }
            ReturnConversion::CallbackHandle(_) => Some(CallbackHandle::empty()),
        }
    }

    fn direct_success(
        &self,
        call: Expression,
        success_out: &Identifier,
        value: Expression,
    ) -> Statement {
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                Statement::assign(
                    Expression::optional_chain_member(success_out, "pointee"),
                    value,
                )
                .to_string(),
                Statement::returns(Self::empty_error()).to_string(),
            ]
            .join("\n"),
        )
    }

    fn empty_error() -> Expression {
        Expression::call("FfiBuf_u8", ArgumentList::default())
    }
}

impl EncodedProxyArgument {
    fn new(
        source_name: &Name,
        value: Identifier,
        codec: &<OutOfRust as Direction>::Codec,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let buffer = ArgumentBuffer::new(source_name)?;
        let write = codec
            .write_self_value()
            .render_with(&mut Writer::new(
                buffer.writer().clone(),
                Expression::identifier(value),
                context,
            ))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        Ok(Self {
            buffer: buffer.with_statements(write),
        })
    }

    fn scalar_option(source_name: &Name, value: Identifier, primitive: Primitive) -> Result<Self> {
        Ok(Self {
            buffer: ScalarOption::new(primitive)
                .write(source_name, Expression::identifier(value))?,
        })
    }

    fn arguments(&self) -> Vec<Expression> {
        self.buffer.arguments()
    }

    fn wrap(&self, body: String, returns_value: bool) -> String {
        let body = Statement::new(Statement::new(body).indented("    "));
        [
            self.buffer.bytes_statement().to_string(),
            match returns_value {
                true => self.buffer.returning_scope(body, "", false),
                false => self.buffer.effect_scope(body, "", false),
            },
        ]
        .join("\n")
    }
}

impl ProxyArgumentScope {
    fn wrap(&self, body: String, returns_value: bool) -> String {
        match self {
            Self::Encoded(encoded) => encoded.wrap(body, returns_value),
            Self::DirectVector(vector) => {
                vector.wrap(Statement::new(body).indented("    "), "", returns_value)
            }
        }
    }
}

impl CallbackError {
    fn from_channel(
        channel: ErrorChannel<'_, Native, IntoRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match channel {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ty,
                codec,
                shape: native::BufferShape::Buffer,
            } => EncodedCallbackError::new(ty, codec, bridge, context).map(Self::Encoded),
            ErrorChannel::Encoded { .. } => {
                Err(SwiftHost::unsupported("callback encoded error channel"))
            }
            ErrorChannel::Status => Err(SwiftHost::unsupported("callback status error channel")),
            _ => Err(SwiftHost::unsupported("unknown callback error channel")),
        }
    }

    fn fallible(&self) -> bool {
        !matches!(self, Self::None)
    }

    fn requires_wire_runtime(&self) -> bool {
        matches!(self, Self::Encoded(_))
    }

    fn statement(&self, call: Expression, returns: &Return) -> Statement {
        match self {
            Self::None => returns.statement(call),
            Self::Encoded(error) => error.statement(Expression::trying(call), returns),
        }
    }

    fn async_statement(
        &self,
        call: Expression,
        returns: &Return,
        completion: &AsyncCompletion,
    ) -> Statement {
        match self {
            Self::None => {
                returns.completion_success_statement(Expression::awaiting(call), completion)
            }
            Self::Encoded(error) => error.async_statement(
                Expression::trying(Expression::awaiting(call)),
                returns,
                completion,
            ),
        }
    }
}

impl EncodedCallbackError {
    fn new(
        ty: &TypeRef,
        codec: &<IntoRust as Direction>::Codec,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let buffer = Self::buffer()?;
        let write = codec
            .render_with(&mut Writer::new(
                buffer.writer().clone(),
                Self::error_value(ty, context)?,
                context,
            ))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        Ok(Self {
            buffer: buffer.with_statements(write),
            copy: Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
        })
    }

    fn statement(&self, call: Expression, returns: &Return) -> Statement {
        Statement::new(format!(
            "do {{\n{}\n}} catch {{\n{}\n}}",
            returns.success_statement(call).indented("    "),
            self.catch_statement().indented("    ")
        ))
    }

    fn catch_statement(&self) -> Statement {
        Statement::new(
            [
                self.buffer.bytes_statement().to_string(),
                self.buffer.returning_scope(
                    Statement::new(Statement::expression(self.copy_expression()).indented("    ")),
                    "",
                    false,
                ),
            ]
            .join("\n"),
        )
    }

    fn async_statement(
        &self,
        call: Expression,
        returns: &Return,
        completion: &AsyncCompletion,
    ) -> Statement {
        Statement::new(format!(
            "do {{\n{}\n}} catch {{\n{}\n}}",
            returns
                .completion_success_statement(call, completion)
                .indented("    "),
            self.completion_statement(completion).indented("    ")
        ))
    }

    fn completion_statement(&self, completion: &AsyncCompletion) -> Statement {
        Statement::new(
            [
                self.buffer.bytes_statement().to_string(),
                self.buffer.unsafe_buffer_scope(
                    Statement::new(
                        Statement::expression(completion.call(
                            AsyncCompletion::failure_status(),
                            Some(self.copy_expression()),
                        ))
                        .indented("    "),
                    ),
                    "",
                ),
            ]
            .join("\n"),
        )
    }

    fn copy_expression(&self) -> Expression {
        self.buffer.copy_expression(&self.copy)
    }

    fn buffer() -> Result<ArgumentBuffer> {
        Ok(ArgumentBuffer::from_parts(
            GeneratedLocal::ErrorBuffer.suffixed("bytes")?,
            GeneratedLocal::ErrorBuffer.suffixed("buffer")?,
            GeneratedLocal::ErrorBuffer.suffixed("writer")?,
        ))
    }

    fn error_value(ty: &TypeRef, context: &RenderContext<Native>) -> Result<Expression> {
        match ty {
            TypeRef::String => Ok(Expression::nil_coalescing(
                Expression::optional_chain_member(
                    Expression::new("(error as? FfiError)"),
                    "message",
                ),
                Expression::call(
                    "String",
                    [Expression::labeled("describing", "error")]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            )),
            TypeRef::Record(_) | TypeRef::Enum(_) => Ok(Expression::forced(Expression::new(
                format!("(error as? {})", SwiftType::type_ref(ty, context)?),
            ))),
            _ => Err(SwiftHost::unsupported("callback encoded error type")),
        }
    }
}

impl DirectReturn {
    fn new(
        value: DirectValue,
        result: &Identifier,
        bridge: &CBridgeContract,
        encoded_completion: bool,
    ) -> Result<Self> {
        let encoded_completion = match encoded_completion {
            true => Some(EncodedDirectCompletion::new(&value, result, bridge)?),
            false => None,
        };
        Ok(Self {
            value,
            encoded_completion,
        })
    }

    fn c_value(&self, value: Expression) -> Expression {
        self.value.c_value(value)
    }

    fn swift_value(&self, value: Expression) -> Expression {
        self.value.swift_value(value)
    }

    fn default_storage_value(&self) -> Expression {
        self.value.default_storage_value()
    }

    fn requires_wire_runtime(&self) -> bool {
        self.encoded_completion.is_some()
    }

    fn completion_statement(
        &self,
        call: Expression,
        result: &Identifier,
        completion: &AsyncCompletion,
    ) -> Statement {
        match (
            &self.encoded_completion,
            completion.expects_buffer_payload(),
        ) {
            (Some(encoded), true) => encoded.statement(call, result, completion),
            _ => Statement::new(
                [
                    Statement::let_value(result, call).to_string(),
                    completion
                        .success_statement(Some(
                            self.c_value(Expression::identifier(result.clone())),
                        ))
                        .to_string(),
                ]
                .join("\n"),
            ),
        }
    }
}

impl EncodedDirectCompletion {
    fn new(value: &DirectValue, result: &Identifier, bridge: &CBridgeContract) -> Result<Self> {
        let buffer = EncodedReturn::buffer()?;
        let write = value.write_statement(
            buffer.writer().clone(),
            Expression::identifier(result.clone()),
        )?;
        Ok(Self {
            buffer: buffer.with_statement(write),
            copy: Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
        })
    }

    fn statement(
        &self,
        call: Expression,
        result: &Identifier,
        completion: &AsyncCompletion,
    ) -> Statement {
        Statement::new(
            [
                Statement::let_value(result, call).to_string(),
                self.buffer.bytes_statement().to_string(),
                self.buffer.unsafe_buffer_scope(
                    Statement::new(
                        completion
                            .success_statement(Some(self.copy_expression()))
                            .indented("    "),
                    ),
                    "",
                ),
            ]
            .join("\n"),
        )
    }

    fn copy_expression(&self) -> Expression {
        self.buffer.copy_expression(&self.copy)
    }
}

impl EncodedReturn {
    fn new(
        codec: &<IntoRust as Direction>::Codec,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let result = GeneratedLocal::ReturnBuffer.identifier()?;
        let buffer = Self::buffer()?;
        let write = codec
            .render_with(&mut Writer::new(
                buffer.writer().clone(),
                Expression::identifier(result.clone()),
                context,
            ))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        Ok(Self {
            result,
            buffer: buffer.with_statements(write),
            copy: Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
            proxy: EncodedProxyReturn::new(codec, bridge, context)?,
        })
    }

    fn scalar_option(primitive: Primitive, bridge: &CBridgeContract) -> Result<Self> {
        let result = GeneratedLocal::ReturnBuffer.identifier()?;
        let buffer = Self::buffer()?;
        let write = vec![ScalarOption::new(primitive).write_statement(
            buffer.writer().clone(),
            Expression::identifier(result.clone()),
        )?];
        Ok(Self {
            result,
            buffer: buffer.with_statements(write),
            copy: Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
            proxy: EncodedProxyReturn::scalar_option(primitive, bridge)?,
        })
    }

    fn proxy_statement(&self, call: Expression) -> Statement {
        self.proxy.statement(call)
    }

    fn statement(&self, call: Expression) -> Statement {
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.buffer.bytes_statement().to_string(),
                self.buffer.returning_scope(
                    Statement::new(Statement::expression(self.copy_expression()).indented("    ")),
                    "",
                    false,
                ),
            ]
            .join("\n"),
        )
    }

    fn success_statement(&self, call: Expression, success_out: &Identifier) -> Statement {
        let store = Statement::assign(
            Expression::optional_chain_member(success_out, "pointee"),
            self.copy_expression(),
        );
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.buffer.bytes_statement().to_string(),
                self.buffer
                    .unsafe_buffer_scope(Statement::new(store.indented("    ")), ""),
                Statement::returns(Return::empty_error()).to_string(),
            ]
            .join("\n"),
        )
    }

    fn completion_statement(&self, call: Expression, completion: &AsyncCompletion) -> Statement {
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.buffer.bytes_statement().to_string(),
                self.buffer.unsafe_buffer_scope(
                    Statement::new(
                        completion
                            .success_statement(Some(self.copy_expression()))
                            .indented("    "),
                    ),
                    "",
                ),
            ]
            .join("\n"),
        )
    }

    fn copy_expression(&self) -> Expression {
        self.buffer.copy_expression(&self.copy)
    }

    fn buffer() -> Result<ArgumentBuffer> {
        Ok(ArgumentBuffer::from_parts(
            GeneratedLocal::ReturnBuffer.suffixed("bytes")?,
            GeneratedLocal::ReturnBuffer.suffixed("buffer")?,
            GeneratedLocal::ReturnBuffer.suffixed("writer")?,
        ))
    }
}

impl DirectVectorReturn {
    fn new(vector: &DirectVector, bridge: &CBridgeContract) -> Result<Self> {
        Ok(Self {
            copied: vector.copied(
                GeneratedLocal::ReturnBuffer.identifier()?,
                Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
            )?,
            returned: vector.returned(),
            free: Identifier::parse(bridge.support().buffer_free()?.name())?,
        })
    }

    fn statement(&self, call: Expression) -> Statement {
        self.copied.statement(call)
    }

    fn success_statement(
        &self,
        call: Expression,
        success_out: &Identifier,
        empty_error: Expression,
    ) -> Statement {
        self.copied
            .success_statement(call, success_out, empty_error)
    }

    fn consume_statement<F>(&self, call: Expression, consume: F) -> Statement
    where
        F: FnOnce(Expression) -> Statement,
    {
        self.copied.consume_statement(call, consume)
    }

    fn proxy_statement(&self, call: Expression) -> Result<Statement> {
        self.returned.body(call, "", &self.free).map(Statement::new)
    }
}

impl EncodedProxyReturn {
    fn new(
        codec: &<IntoRust as Direction>::Codec,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let buffer = OwnedBuffer::new(GeneratedLocal::ReturnBuffer.suffixed("encoded")?);
        let reader = GeneratedLocal::WireReader.identifier()?;
        let decode = codec
            .read_plan()
            .render_with(&mut Reader::new(reader.clone(), context))
            .map(ReadExpression::into_expression)?;
        Ok(Self {
            decode: buffer.decode(&reader, &decode)?,
            buffer,
            free: Identifier::parse(bridge.support().buffer_free()?.name())?,
        })
    }

    fn scalar_option(primitive: Primitive, bridge: &CBridgeContract) -> Result<Self> {
        let buffer = OwnedBuffer::new(GeneratedLocal::ReturnBuffer.suffixed("encoded")?);
        let reader = GeneratedLocal::WireReader.identifier()?;
        let decode = ScalarOption::new(primitive).read(reader.clone())?;
        Ok(Self {
            decode: buffer.decode(&reader, &decode)?,
            buffer,
            free: Identifier::parse(bridge.support().buffer_free()?.name())?,
        })
    }

    fn statement(&self, call: Expression) -> Statement {
        Statement::new(
            [
                Statement::let_value(self.buffer.binding(), call).to_string(),
                Statement::defer(self.buffer.free_call(&self.free)).to_string(),
                Statement::returns(&self.decode).to_string(),
            ]
            .join("\n"),
        )
    }
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ParameterPlan<'_, '_> {
    type Output = Result<Parameter>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ()) -> Self::Output {
        let binding = self.value_binding()?;
        let direct = DirectValue::new(ty, self.bridge, self.context)?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: direct.api_type().clone(),
            bindings: vec![binding.clone()],
            setup: Vec::new(),
            argument: direct.swift_value(Expression::identifier(binding)),
            proxy_arguments: vec![direct.c_value(Expression::identifier(self.name.clone()))],
            proxy_scopes: Vec::new(),
            requires_wire_runtime: false,
        })
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
        _: (),
    ) -> Self::Output {
        if shape != native::BufferShape::Slice {
            return Err(SwiftHost::unsupported("encoded callback parameter shape"));
        }
        let ParameterGroup::ByteSlice(slice) = self.group else {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "encoded callback parameter does not use a byte-slice C group",
            });
        };
        let pointer = Identifier::escape(self.slot.parameter(slice.pointer()).name())?;
        let length = Identifier::escape(self.slot.parameter(slice.length()).name())?;
        let reader = self.source_name.generated("reader")?;
        let decode = codec
            .render_with(&mut Reader::new(reader.clone(), self.context))
            .map(ReadExpression::into_expression)?;
        let proxy =
            EncodedProxyArgument::new(&self.source_name, self.name.clone(), codec, self.context)?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: SwiftType::type_ref(ty, self.context)?,
            bindings: vec![pointer.clone(), length.clone()],
            setup: vec![Statement::var_value(
                &reader,
                "WireReader",
                Expression::call(
                    "WireReader",
                    [
                        Expression::labeled("ptr", Expression::forced(&pointer)),
                        Expression::labeled(
                            "len",
                            Expression::call(
                                TypeName::int(),
                                [Expression::identifier(length)]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )],
            argument: decode,
            proxy_arguments: proxy.arguments(),
            proxy_scopes: vec![ProxyArgumentScope::Encoded(proxy)],
            requires_wire_runtime: true,
        })
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _: (),
    ) -> Self::Output {
        let HandleTarget::Callback(callback) = target else {
            return Err(SwiftHost::unsupported("unknown handle callback parameter"));
        };
        let binding = self.value_binding()?;
        let handle = CallbackHandle::from_rust_handle(*callback, presence, self.context)?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: handle.api_type(),
            bindings: vec![binding.clone()],
            setup: Vec::new(),
            argument: handle.wrap(Expression::identifier(binding)),
            proxy_arguments: vec![handle.create(Expression::identifier(self.name.clone()))],
            proxy_scopes: Vec::new(),
            requires_wire_runtime: false,
        })
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let ParameterGroup::ByteSlice(slice) = self.group else {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "optional scalar callback parameter does not use a byte-slice C group",
            });
        };
        let pointer = Identifier::escape(self.slot.parameter(slice.pointer()).name())?;
        let length = Identifier::escape(self.slot.parameter(slice.length()).name())?;
        let reader = self.source_name.generated("reader")?;
        let proxy =
            EncodedProxyArgument::scalar_option(&self.source_name, self.name.clone(), primitive)?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: ScalarOption::new(primitive).ty()?,
            bindings: vec![pointer.clone(), length.clone()],
            setup: vec![Statement::var_value(
                &reader,
                "WireReader",
                Expression::call(
                    "WireReader",
                    [
                        Expression::labeled("ptr", Expression::forced(&pointer)),
                        Expression::labeled(
                            "len",
                            Expression::call(
                                TypeName::int(),
                                [Expression::identifier(length)]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )],
            argument: ScalarOption::new(primitive).read(reader)?,
            proxy_arguments: proxy.arguments(),
            proxy_scopes: vec![ProxyArgumentScope::Encoded(proxy)],
            requires_wire_runtime: true,
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType, _: ()) -> Self::Output {
        let ParameterGroup::DirectVector(group) = self.group else {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "direct-vector callback parameter does not use a direct-vector C group",
            });
        };
        let vector = DirectVector::from_element(element, self.bridge, self.context)?;
        let pointer = Identifier::escape(self.slot.parameter(group.pointer()).name())?;
        let length = Identifier::escape(self.slot.parameter(group.length()).name())?;
        let received = vector.received(&self.source_name, pointer.clone(), length.clone())?;
        let proxy = vector.borrowed(&self.source_name, self.name.clone(), Receive::ByValue)?;
        Ok(Parameter {
            name: self.name.clone(),
            ty: vector.ty().clone(),
            bindings: vec![pointer, length],
            setup: received.setup(),
            argument: received.value(),
            proxy_arguments: proxy.arguments(),
            proxy_scopes: vec![ProxyArgumentScope::DirectVector(proxy)],
            requires_wire_runtime: false,
        })
    }
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ProxyParameterSupport<'_, '_> {
    type Output = Result<()>;

    fn direct(&mut self, _: &'plan DirectValueType, _: ()) -> Self::Output {
        Ok(())
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan <OutOfRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
        _: (),
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => Ok(()),
            _ => Err(SwiftHost::unsupported(
                "callback proxy encoded parameter shape",
            )),
        }
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        match target {
            HandleTarget::Callback(callback) => {
                CallbackHandle::validate_proxy(*callback, self.context)
            }
            _ => Err(SwiftHost::unsupported("callback proxy handle parameter")),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType, _: ()) -> Self::Output {
        Ok(())
    }
}

impl ParameterPlan<'_, '_> {
    fn value_binding(&self) -> Result<Identifier> {
        match self.group {
            ParameterGroup::Value(index) => Identifier::escape(self.slot.parameter(*index).name()),
            _ => Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "direct callback parameter does not use a value C group",
            }),
        }
    }
}

impl ReturnPlan<'_, '_> {
    fn success_out(&self, slot: ReturnValueSlot) -> Result<Option<Identifier>> {
        match (self.fallible, self.asynchronous, slot) {
            (false, _, ReturnValueSlot::ReturnSlot) => Ok(None),
            (false, _, ReturnValueSlot::OutPointer) => {
                Err(SwiftHost::unsupported("callback out pointer return"))
            }
            (true, true, ReturnValueSlot::OutPointer) => Ok(None),
            (true, false, ReturnValueSlot::OutPointer) => self.success_out_parameter().map(Some),
            (true, _, ReturnValueSlot::ReturnSlot) => Err(SwiftHost::unsupported(
                "fallible callback return slot success",
            )),
            _ => Err(SwiftHost::unsupported("unknown callback return slot")),
        }
    }

    fn success_out_parameter(&self) -> Result<Identifier> {
        match self.slot.return_parameter_groups() {
            [ParameterGroup::SuccessOut(index)] => {
                Identifier::escape(self.slot.parameter(*index).name())
            }
            _ => Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "fallible callback success does not use one success out parameter",
            }),
        }
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ReturnPlan<'_, '_> {
    type Output = Result<Return>;

    fn void(&mut self) -> Self::Output {
        Ok(Return {
            ty: None,
            conversion: ReturnConversion::Void,
            result: GeneratedLocal::ReturnBuffer.identifier()?,
            success_out: None,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        let success_out = self.success_out(slot)?;
        let direct = DirectValue::new(ty, self.bridge, self.context)?;
        let api_type = direct.api_type().clone();
        let result = GeneratedLocal::ReturnBuffer.identifier()?;
        let conversion = ReturnConversion::Direct(DirectReturn::new(
            direct,
            &result,
            self.bridge,
            self.fallible && self.asynchronous,
        )?);
        Ok(Return {
            ty: Some(api_type),
            conversion,
            result,
            success_out,
        })
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        let success_out = self.success_out(slot)?;
        if shape != native::BufferShape::Buffer {
            return Err(SwiftHost::unsupported("encoded callback return shape"));
        }
        Ok(Return {
            ty: Some(SwiftType::type_ref(ty, self.context)?),
            conversion: ReturnConversion::Encoded(EncodedReturn::new(
                codec,
                self.bridge,
                self.context,
            )?),
            result: GeneratedLocal::ReturnBuffer.identifier()?,
            success_out,
        })
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        match target {
            HandleTarget::Callback(callback) => {
                let success_out = self.success_out(slot)?;
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(Return {
                    ty: Some(handle.api_type()),
                    conversion: ReturnConversion::CallbackHandle(handle),
                    result: GeneratedLocal::ReturnBuffer.identifier()?,
                    success_out,
                })
            }
            _ => Err(SwiftHost::unsupported("unknown callback handle return")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(Return {
            ty: Some(ScalarOption::new(primitive).ty()?),
            conversion: ReturnConversion::Encoded(EncodedReturn::scalar_option(
                primitive,
                self.bridge,
            )?),
            result: GeneratedLocal::ReturnBuffer.identifier()?,
            success_out: None,
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        if self.fallible {
            return Err(SwiftHost::unsupported(
                "fallible direct vector callback return",
            ));
        }
        let vector = DirectVector::from_element(element, self.bridge, self.context)?;
        Ok(Return {
            ty: Some(vector.ty().clone()),
            result: GeneratedLocal::ReturnBuffer.identifier()?,
            conversion: ReturnConversion::DirectVector(DirectVectorReturn::new(
                &vector,
                self.bridge,
            )?),
            success_out: None,
        })
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(SwiftHost::unsupported("closure callback return"))
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ProxyReturnSupport<'_, '_> {
    type Output = Result<()>;

    fn void(&mut self) -> Self::Output {
        Ok(())
    }

    fn direct(&mut self, _: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {
        Ok(())
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan <IntoRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::ReturnSlot, native::BufferShape::Buffer) => Ok(()),
            _ => Err(SwiftHost::unsupported("callback proxy encoded return")),
        }
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match target {
            HandleTarget::Callback(callback) => {
                CallbackHandle::validate_proxy(*callback, self.context)
            }
            _ => Err(SwiftHost::unsupported("callback proxy handle return")),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(SwiftHost::unsupported("callback proxy closure return"))
    }
}
