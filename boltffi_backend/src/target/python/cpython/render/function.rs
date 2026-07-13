use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClosureReturn, DirectValueType, DirectVectorElementType, ErrorChannel, ErrorPlacement,
    ExecutionDecl, ExportedCallable, FunctionDecl, HandlePresence, HandleTarget, IncomingParam,
    IntoRust, Native, NativeSymbol, OutOfRust, ParamPlanRender, Primitive, ReadPlan, Receive,
    RecordId, ReturnPlan, ReturnPlanRender, ReturnValueSlot, TypeRef, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        python_cext::{
            ExtensionMethod, LoadedFunction, MethodFlags, MethodName, PythonCExtBridgeContract,
        },
    },
    core::{Diagnostic, Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{argument, direct_vector, primitive, result},
        name_style::Name,
        render::NativeFutureMethods,
        syntax::Identifier as PythonIdentifier,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/function.c", escape = "none")]
struct SyncTemplate {
    python_name: PythonIdentifier,
    wrapper: Identifier,
    storage: Identifier,
    params: Vec<argument::Conversion>,
    call_args: Vec<c::Expression>,
    returns: result::Conversion,
    mutation: Option<argument::MutationOutput>,
    fallible: Option<FallibleResult>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/async_function.c", escape = "none")]
struct AsyncTemplate {
    python_name: PythonIdentifier,
    start_wrapper: Identifier,
    start_storage: Identifier,
    poll_python_name: PythonIdentifier,
    poll_wrapper: Identifier,
    poll_storage: Identifier,
    complete_wrapper: Identifier,
    complete_storage: Identifier,
    panic_message_wrapper: Identifier,
    panic_storage: Identifier,
    cancel_wrapper: Identifier,
    cancel_storage: Identifier,
    free_wrapper: Identifier,
    free_storage: Identifier,
    params: Vec<argument::Conversion>,
    call_args: Vec<c::Expression>,
    complete_call_args: Vec<c::Expression>,
    returns: result::Conversion,
    fallible: Option<FallibleResult>,
}

pub struct Function {
    body: Body,
}

struct SyncFunction {
    pub python_name: PythonIdentifier,
    pub wrapper: Identifier,
    pub storage: Identifier,
    pub params: Vec<argument::Conversion>,
    pub call_args: Vec<c::Expression>,
    pub returns: result::Conversion,
    mutation: Option<argument::MutationOutput>,
    fallible: Option<FallibleResult>,
    methods: Vec<ExtensionMethod>,
}

struct AsyncFunction {
    future_methods: NativeFutureMethods,
    start_wrapper: Identifier,
    start_storage: Identifier,
    poll_wrapper: Identifier,
    poll_storage: Identifier,
    complete_wrapper: Identifier,
    complete_storage: Identifier,
    panic_message_wrapper: Identifier,
    panic_storage: Identifier,
    cancel_wrapper: Identifier,
    cancel_storage: Identifier,
    free_wrapper: Identifier,
    free_storage: Identifier,
    params: Vec<argument::Conversion>,
    call_args: Vec<c::Expression>,
    complete_call_args: Vec<c::Expression>,
    returns: result::Conversion,
    fallible: Option<FallibleResult>,
    methods: Vec<ExtensionMethod>,
}

struct FutureExtensionMethods {
    start: Identifier,
    poll: Identifier,
    complete: Identifier,
    cancel: Identifier,
    free: Identifier,
    panic_message: Identifier,
}

impl FutureExtensionMethods {
    fn new(
        start: Identifier,
        poll: Identifier,
        complete: Identifier,
        cancel: Identifier,
        free: Identifier,
        panic_message: Identifier,
    ) -> Self {
        Self {
            start,
            poll,
            complete,
            cancel,
            free,
            panic_message,
        }
    }

    fn bind(self, names: &NativeFutureMethods) -> Result<Vec<ExtensionMethod>> {
        [
            ExtensionMethod::new(
                MethodName::parse(names.start().as_str())?,
                self.start,
                MethodFlags::FastCall,
            ),
            ExtensionMethod::new(
                MethodName::parse(names.poll().as_str())?,
                self.poll,
                MethodFlags::FastCall,
            ),
            ExtensionMethod::new(
                MethodName::parse(names.complete().as_str())?,
                self.complete,
                MethodFlags::OneObject,
            ),
            ExtensionMethod::new(
                MethodName::parse(names.panic_message().as_str())?,
                self.panic_message,
                MethodFlags::OneObject,
            ),
            ExtensionMethod::new(
                MethodName::parse(names.cancel().as_str())?,
                self.cancel,
                MethodFlags::OneObject,
            ),
            ExtensionMethod::new(
                MethodName::parse(names.free().as_str())?,
                self.free,
                MethodFlags::OneObject,
            ),
        ]
        .into_iter()
        .collect()
    }
}

enum Body {
    Sync(Box<SyncFunction>),
    Async(Box<AsyncFunction>),
    Skipped(SkippedFunction),
}

impl Function {
    pub fn from_declaration(
        declaration: &FunctionDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_export(
            Name::new(declaration.name()).function()?,
            declaration.symbol(),
            declaration.callable(),
            Vec::new(),
            bridge,
            context,
        )
    }

    pub fn from_export(
        python_name: PythonIdentifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if let Some(unsupported) = UnsupportedCallable::from_parts(callable, &receiver_args) {
            return Ok(Self {
                body: Body::Skipped(SkippedFunction::new(python_name, unsupported)),
            });
        }
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => SyncFunction::from_export(
                python_name,
                symbol,
                callable,
                receiver_args,
                bridge,
                context,
            )
            .map(Box::new)
            .map(Body::Sync)
            .map(|body| Self { body }),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => AsyncFunction::from_export(
                python_name,
                AsyncSymbols {
                    start: symbol.clone(),
                    poll: poll.clone(),
                    complete: complete.clone(),
                    cancel: cancel.clone(),
                    free: free.clone(),
                    panic_message: panic_message.clone(),
                },
                callable,
                receiver_args,
                bridge,
                context,
            )
            .map(Box::new)
            .map(Body::Async)
            .map(|body| Self { body }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown function execution",
            }),
        }
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        self.body.methods().iter()
    }

    pub fn render(self) -> Result<Emitted> {
        match self.body {
            Body::Sync(function) => function.render(),
            Body::Async(function) => function.render(),
            Body::Skipped(function) => Ok(function.render()),
        }
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        match &self.body {
            Body::Sync(function) => function.primitives(),
            Body::Async(function) => function.primitives(),
            Body::Skipped(_) => Vec::new(),
        }
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> {
        match &self.body {
            Body::Sync(function) => function.wire_primitives().collect::<Vec<_>>(),
            Body::Async(function) => function.wire_primitives().collect::<Vec<_>>(),
            Body::Skipped(_) => Vec::new(),
        }
        .into_iter()
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> {
        match &self.body {
            Body::Sync(function) => function.direct_vector_elements().collect::<Vec<_>>(),
            Body::Async(function) => function.direct_vector_elements().collect::<Vec<_>>(),
            Body::Skipped(_) => Vec::new(),
        }
        .into_iter()
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.owned_buffers().next()
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> {
        match &self.body {
            Body::Sync(function) => function.owned_buffers().collect::<Vec<_>>(),
            Body::Async(function) => function.owned_buffers().collect::<Vec<_>>(),
            Body::Skipped(_) => Vec::new(),
        }
        .into_iter()
    }

    pub fn has_string_argument(&self) -> bool {
        match &self.body {
            Body::Sync(function) => function.has_string_argument(),
            Body::Async(function) => function.has_string_argument(),
            Body::Skipped(_) => false,
        }
    }

    pub fn has_bytes_argument(&self) -> bool {
        match &self.body {
            Body::Sync(function) => function.has_bytes_argument(),
            Body::Async(function) => function.has_bytes_argument(),
            Body::Skipped(_) => false,
        }
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        match &self.body {
            Body::Sync(function) => function.has_raw_wire_argument(),
            Body::Async(function) => function.has_raw_wire_argument(),
            Body::Skipped(_) => false,
        }
    }

    pub fn uses_async_protocol(&self) -> bool {
        matches!(self.body, Body::Async(_))
    }

    pub fn can_render(callable: &ExportedCallable<Native>) -> bool {
        UnsupportedCallable::from_callable(callable).is_none()
    }

    fn wrapper_symbol(symbol: &NativeSymbol) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi_python_callable_wrapper_{}",
            symbol.name().as_str()
        ))
    }
}

impl Body {
    fn methods(&self) -> &[ExtensionMethod] {
        match self {
            Self::Sync(function) => &function.methods,
            Self::Async(function) => &function.methods,
            Self::Skipped(function) => &function.methods,
        }
    }
}

struct SkippedFunction {
    python_name: PythonIdentifier,
    shape: &'static str,
    methods: Vec<ExtensionMethod>,
}

impl SkippedFunction {
    fn new(python_name: PythonIdentifier, unsupported: UnsupportedCallable) -> Self {
        Self {
            python_name,
            shape: unsupported.shape(),
            methods: Vec::new(),
        }
    }

    fn render(self) -> Emitted {
        Emitted::diagnostic(Diagnostic::new(format!(
            "unsupported callable {}: {}",
            self.python_name, self.shape
        )))
    }
}

#[derive(Clone, Copy)]
struct UnsupportedCallable {
    shape: &'static str,
}

impl UnsupportedCallable {
    fn from_parts(
        callable: &ExportedCallable<Native>,
        receiver_args: &[argument::Conversion],
    ) -> Option<Self> {
        if matches!(callable.execution(), ExecutionDecl::Asynchronous(_))
            && receiver_args.iter().any(argument::Conversion::has_mutation)
        {
            return Some(Self {
                shape: "async mutable encoded receiver",
            });
        }

        Self::from_callable(callable)
    }

    fn from_callable(callable: &ExportedCallable<Native>) -> Option<Self> {
        let mutation_count = callable
            .params()
            .iter()
            .filter(|parameter| match parameter.payload() {
                IncomingParam::Value(plan) => plan.render_with(&mut MutableEncodedParameter),
                IncomingParam::Closure(_) => false,
            })
            .count();
        if mutation_count == 0 {
            return None;
        }
        if matches!(callable.execution(), ExecutionDecl::Asynchronous(_)) {
            return Some(Self {
                shape: "async mutable encoded parameter",
            });
        }
        if mutation_count > 1 {
            return Some(Self {
                shape: "multiple mutable encoded parameters",
            });
        }
        if !matches!(callable.error().channel(), ErrorChannel::None)
            || !matches!(callable.returns().plan(), ReturnPlan::Void)
        {
            return Some(Self {
                shape: "mutable encoded parameter with non-void return",
            });
        }
        None
    }

    fn shape(self) -> &'static str {
        self.shape
    }
}

struct MutableEncodedParameter;

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for MutableEncodedParameter {
    type Output = bool;

    fn direct(&mut self, _: &DirectValueType, _: Receive) -> Self::Output {
        false
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        _: &WritePlan,
        _: native::BufferShape,
        _: boltffi_binding::EncodedParamTransport,
        receive: Receive,
    ) -> Self::Output {
        receive == Receive::ByMutRef
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        false
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        false
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType, _: Receive) -> Self::Output {
        false
    }
}

impl SyncFunction {
    fn from_export(
        python_name: PythonIdentifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let loaded = bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "function without C bridge symbol",
            })?;
        let wrapper = Function::wrapper_symbol(symbol)?;
        let method = ExtensionMethod::new(
            MethodName::parse(python_name.as_str())?,
            wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let value_args = callable
            .params()
            .iter()
            .enumerate()
            .try_fold(
                (
                    receiver_args
                        .iter()
                        .map(argument::Conversion::c_arity)
                        .sum::<usize>(),
                    Vec::new(),
                ),
                |(c_offset, mut conversions), (offset, parameter)| {
                    let index = receiver_args.len() + offset;
                    let conversion = argument::Conversion::from_parameter(
                        symbol.name().as_str(),
                        index,
                        parameter,
                        &loaded.function().params()[c_offset..],
                        bridge,
                        context,
                    )?;
                    let c_offset = c_offset + conversion.c_arity();
                    conversions.push(conversion);
                    Ok::<_, Error>((c_offset, conversions))
                },
            )?
            .1;
        let params = receiver_args
            .into_iter()
            .chain(value_args)
            .collect::<Vec<_>>();
        let base_call_args = Self::call_args(&params)?;
        let fallible = FallibleResult::new(callable, loaded, base_call_args.len())?;
        let call_args = base_call_args
            .into_iter()
            .chain(fallible.iter().filter_map(FallibleResult::success_argument))
            .collect();
        let returns = result::Conversion::from_plan(callable.returns().plan(), bridge, context)?;
        let mutation = Self::mutation(&params, &returns, fallible.is_some())?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().clone(),
            params,
            call_args,
            returns,
            mutation,
            fallible,
            methods: vec![method],
        })
    }

    fn render(self) -> Result<Emitted> {
        let source = SyncTemplate {
            python_name: self.python_name,
            wrapper: self.wrapper,
            storage: self.storage,
            params: self.params,
            call_args: self.call_args,
            returns: self.returns,
            mutation: self.mutation,
            fallible: self.fallible,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    fn mutation(
        params: &[argument::Conversion],
        returns: &result::Conversion,
        fallible: bool,
    ) -> Result<Option<argument::MutationOutput>> {
        let mut mutations = params.iter().filter_map(argument::Conversion::mutation);
        let mutation = mutations.next();
        if mutations.next().is_some() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "multiple mutable encoded parameters",
            });
        }
        if mutation.is_some() && (fallible || !returns.is_void()) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "mutable encoded parameter with non-void return",
            });
        }
        Ok(mutation)
    }

    fn call_args(params: &[argument::Conversion]) -> Result<Vec<c::Expression>> {
        params
            .iter()
            .try_fold(Vec::new(), |mut arguments, parameter| {
                arguments.extend(parameter.call_args()?);
                Ok(arguments)
            })
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        let params = self
            .params
            .iter()
            .filter_map(argument::Conversion::primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_primitives),
            )
            .collect::<Vec<_>>();
        params
            .into_iter()
            .chain(self.returns.primitive())
            .chain(self.fallible.iter().filter_map(FallibleResult::primitive))
            .collect()
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::wire_primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_wire_primitives),
            )
    }

    fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::direct_vector_element)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_direct_vector_elements),
            )
            .chain(self.returns.direct_vector_element())
            .chain(
                self.fallible
                    .iter()
                    .filter_map(FallibleResult::direct_vector_element),
            )
    }

    fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::mutation_owned_buffer)
            .chain(self.returns.owned_buffer())
            .chain(
                self.fallible
                    .iter()
                    .filter_map(FallibleResult::owned_buffer),
            )
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_string)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_string_argument())
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_bytes)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_bytes_argument())
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_raw_wire)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_raw_wire_argument())
    }
}

#[derive(Clone)]
struct AsyncSymbols {
    start: NativeSymbol,
    poll: NativeSymbol,
    complete: NativeSymbol,
    cancel: NativeSymbol,
    free: NativeSymbol,
    panic_message: NativeSymbol,
}

impl AsyncFunction {
    fn from_export(
        python_name: PythonIdentifier,
        symbols: AsyncSymbols,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let start = Self::loaded(&symbols.start, bridge, "async start symbol")?;
        let poll = Self::loaded(&symbols.poll, bridge, "async poll symbol")?;
        let complete = Self::loaded(&symbols.complete, bridge, "async complete symbol")?;
        let cancel = Self::loaded(&symbols.cancel, bridge, "async cancel symbol")?;
        let free = Self::loaded(&symbols.free, bridge, "async free symbol")?;
        let panic_message = Self::loaded(&symbols.panic_message, bridge, "async panic symbol")?;
        let future_methods = NativeFutureMethods::new(python_name)?;
        let start_wrapper = Function::wrapper_symbol(&symbols.start)?;
        let poll_wrapper = Function::wrapper_symbol(&symbols.poll)?;
        let complete_wrapper = Function::wrapper_symbol(&symbols.complete)?;
        let panic_message_wrapper = Function::wrapper_symbol(&symbols.panic_message)?;
        let cancel_wrapper = Function::wrapper_symbol(&symbols.cancel)?;
        let free_wrapper = Function::wrapper_symbol(&symbols.free)?;
        let value_args = callable
            .params()
            .iter()
            .enumerate()
            .try_fold(
                (
                    receiver_args
                        .iter()
                        .map(argument::Conversion::c_arity)
                        .sum::<usize>(),
                    Vec::new(),
                ),
                |(c_offset, mut conversions), (offset, parameter)| {
                    let index = receiver_args.len() + offset;
                    let conversion = argument::Conversion::from_parameter(
                        symbols.start.name().as_str(),
                        index,
                        parameter,
                        &start.function().params()[c_offset..],
                        bridge,
                        context,
                    )?;
                    let c_offset = c_offset + conversion.c_arity();
                    conversions.push(conversion);
                    Ok::<_, Error>((c_offset, conversions))
                },
            )?
            .1;
        let params = receiver_args
            .into_iter()
            .chain(value_args)
            .collect::<Vec<_>>();
        let call_args = SyncFunction::call_args(&params)?;
        let fallible = FallibleResult::new(callable, complete, 2)?;
        let complete_call_args = fallible
            .iter()
            .filter_map(FallibleResult::success_argument)
            .collect::<Vec<_>>();
        let returns = result::Conversion::from_plan(callable.returns().plan(), bridge, context)?;
        let methods = FutureExtensionMethods::new(
            start_wrapper.clone(),
            poll_wrapper.clone(),
            complete_wrapper.clone(),
            cancel_wrapper.clone(),
            free_wrapper.clone(),
            panic_message_wrapper.clone(),
        )
        .bind(&future_methods)?;
        Ok(Self {
            future_methods,
            start_wrapper,
            start_storage: start.storage_name().clone(),
            poll_wrapper,
            poll_storage: poll.storage_name().clone(),
            complete_wrapper,
            complete_storage: complete.storage_name().clone(),
            panic_message_wrapper,
            panic_storage: panic_message.storage_name().clone(),
            cancel_wrapper,
            cancel_storage: cancel.storage_name().clone(),
            free_wrapper,
            free_storage: free.storage_name().clone(),
            params,
            call_args,
            complete_call_args,
            returns,
            fallible,
            methods,
        })
    }

    fn render(self) -> Result<Emitted> {
        let source = AsyncTemplate {
            python_name: self.future_methods.start().clone(),
            start_wrapper: self.start_wrapper,
            start_storage: self.start_storage,
            poll_python_name: self.future_methods.poll().clone(),
            poll_wrapper: self.poll_wrapper,
            poll_storage: self.poll_storage,
            complete_wrapper: self.complete_wrapper,
            complete_storage: self.complete_storage,
            panic_message_wrapper: self.panic_message_wrapper,
            panic_storage: self.panic_storage,
            cancel_wrapper: self.cancel_wrapper,
            cancel_storage: self.cancel_storage,
            free_wrapper: self.free_wrapper,
            free_storage: self.free_storage,
            params: self.params,
            call_args: self.call_args,
            complete_call_args: self.complete_call_args,
            returns: self.returns,
            fallible: self.fallible,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        self.params
            .iter()
            .filter_map(argument::Conversion::primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_primitives),
            )
            .chain(self.returns.primitive())
            .chain(self.fallible.iter().filter_map(FallibleResult::primitive))
            .collect()
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::wire_primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_wire_primitives),
            )
    }

    fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::direct_vector_element)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_direct_vector_elements),
            )
            .chain(self.returns.direct_vector_element())
            .chain(
                self.fallible
                    .iter()
                    .filter_map(FallibleResult::direct_vector_element),
            )
    }

    fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::mutation_owned_buffer)
            .chain(self.returns.owned_buffer())
            .chain(
                self.fallible
                    .iter()
                    .filter_map(FallibleResult::owned_buffer),
            )
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_string)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_string_argument())
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_bytes)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_bytes_argument())
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_raw_wire)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_raw_wire_argument())
    }

    fn loaded<'bridge>(
        symbol: &NativeSymbol,
        bridge: &'bridge PythonCExtBridgeContract,
        shape: &'static str,
    ) -> Result<&'bridge LoadedFunction> {
        bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape,
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleResult {
    success_declaration: Option<c::Statement>,
    success_argument: Option<c::Expression>,
    success_value: Option<c::Expression>,
    error_type: c::TypeFragment,
    error_value: c::Identifier,
    error: result::Conversion,
}

impl FallibleResult {
    fn new(
        callable: &ExportedCallable<Native>,
        loaded: &LoadedFunction,
        argument_count: usize,
    ) -> Result<Option<Self>> {
        match callable.error().channel() {
            ErrorChannel::None => Ok(None),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                shape: native::BufferShape::Buffer,
                ..
            } => Self::encoded(callable.returns().plan(), loaded, argument_count).map(Some),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ..
            } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error buffer shape",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible function",
            }),
        }
    }

    fn success_argument(&self) -> Option<c::Expression> {
        self.success_argument.clone()
    }

    fn success_value(&self) -> &c::Expression {
        self.success_value.as_ref().expect("fallible success value")
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.error.primitive()
    }

    fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        self.error.direct_vector_element()
    }

    fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.error.owned_buffer()
    }

    fn encoded(
        success: &ReturnPlan<Native, OutOfRust>,
        loaded: &LoadedFunction,
        argument_count: usize,
    ) -> Result<Self> {
        if !matches!(loaded.function().returns(), c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error return",
            });
        }
        let error = result::Conversion::from_owned_buffer(result::OwnedBuffer::RawWire)?;
        let success = FallibleSuccess::new(loaded.function(), argument_count).render(success)?;
        Ok(Self {
            success_declaration: success.declaration,
            success_argument: success.argument,
            success_value: success.value,
            error_type: TypeFragment::anonymous(loaded.function().returns())?,
            error_value: c::Identifier::parse("return_error")?,
            error,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleSuccessBinding {
    declaration: Option<c::Statement>,
    argument: Option<c::Expression>,
    value: Option<c::Expression>,
}

impl FallibleSuccessBinding {
    fn empty() -> Self {
        Self {
            declaration: None,
            argument: None,
            value: None,
        }
    }

    fn out_pointer(ty: &c::Type) -> Result<Self> {
        let value = c::Identifier::parse("return_success")?;
        Ok(Self {
            declaration: Some(TypeFragment::declaration(ty, value.as_str())?),
            argument: Some(c::Expression::address_of(c::Expression::identifier(
                value.clone(),
            ))),
            value: Some(c::Expression::identifier(value)),
        })
    }
}

struct FallibleSuccess {
    out_parameter: Option<c::Type>,
}

impl FallibleSuccess {
    fn new(function: &c::Function, argument_count: usize) -> Self {
        Self {
            out_parameter: function
                .params()
                .get(argument_count)
                .map(|parameter| parameter.ty().clone()),
        }
    }

    fn render(mut self, plan: &ReturnPlan<Native, OutOfRust>) -> Result<FallibleSuccessBinding> {
        plan.render_with(&mut self)
    }

    fn out_pointer(&self, slot: ReturnValueSlot) -> Result<FallibleSuccessBinding> {
        match slot {
            ReturnValueSlot::OutPointer => {
                let Some(c::Type::MutPointer(success_type)) = &self.out_parameter else {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "fallible success parameter",
                    });
                };
                FallibleSuccessBinding::out_pointer(success_type.as_ref())
            }
            ReturnValueSlot::ReturnSlot => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible success return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible success return",
            }),
        }
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for FallibleSuccess {
    type Output = Result<FallibleSuccessBinding>;

    fn void(&mut self) -> Self::Output {
        Ok(FallibleSuccessBinding::empty())
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &DirectValueType) -> Self::Output {
        self.out_pointer(slot)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &TypeRef,
        _: &ReadPlan,
        _: native::BufferShape,
    ) -> Self::Output {
        self.out_pointer(slot)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        self.out_pointer(slot)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible success return",
        })
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible success return",
        })
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible success return",
        })
    }

    fn native_opaque_record(&mut self, _: RecordId) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible native opaque record return",
        })
    }
}
