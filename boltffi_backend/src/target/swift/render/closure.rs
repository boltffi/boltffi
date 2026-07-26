use boltffi_binding::{
    CallableDecl, ClosureParameter as BindingClosureParameter,
    ClosureReturn as BindingClosureReturn, DirectValueType, DirectVectorElementType, Direction,
    ErrorChannel, ErrorPlacement, ForeignBody, HandlePresence, HandleTarget, IntoRust, Native,
    OutOfRust, ParamDecl, ParamPlanRender, Primitive, ReturnPlanRender, ReturnValueSlot, Surface,
    TypeRef, native,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{RenderContext, Result},
    target::swift::{
        SwiftHost,
        c_abi::{CopiedVector, DirectValue, DirectVector},
        codec::{ArgumentBuffer, Reader, ScalarOption, WriteStatement, Writer},
        name_style::{GeneratedLocal, Name},
        render::{SwiftType, callback::CallbackHandle},
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosureArgument {
    source: Identifier,
    ty: TypeName,
    box_binding: Identifier,
    context: Identifier,
    call: Identifier,
    release: Identifier,
    parameters: Vec<ClosureParameter>,
    returns: ClosureReturn,
    error: ClosureError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClosureParameter {
    bindings: Vec<ClosureParameterBinding>,
    public_ty: TypeName,
    setup: Vec<Statement>,
    argument: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClosureParameterBinding {
    name: Identifier,
    ty: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClosureReturn {
    c_ty: TypeName,
    public_ty: Option<TypeName>,
    conversion: ClosureReturnConversion,
    success: Option<ClosureSuccess>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClosureSuccess {
    binding: Identifier,
    value: Identifier,
    ty: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClosureReturnConversion {
    Void,
    Direct,
    FromC,
    Encoded(EncodedClosureReturn),
    DirectVector(CopiedVector),
    CallbackHandle(CallbackHandle),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedClosureReturn {
    result: Identifier,
    buffer: ArgumentBuffer,
    copy: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClosureError {
    None,
    Encoded(Box<EncodedClosureError>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedClosureError {
    public_ty: TypeName,
    success: Identifier,
    value: Identifier,
    buffer: ArgumentBuffer,
    copy: Identifier,
}

struct ClosureParameterType<'context> {
    binding: Identifier,
    bridge: &'context CBridgeContract,
    context: &'context RenderContext<'context, Native>,
}

struct ClosureReturnType<'context> {
    bridge: &'context CBridgeContract,
    context: &'context RenderContext<'context, Native>,
    fallible: bool,
}

impl ClosureArgument {
    pub fn new(
        source_name: &Name,
        source: Identifier,
        closure: &BindingClosureParameter<Native, IntoRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if closure.presence() != HandlePresence::Required {
            return Err(SwiftHost::unsupported("nullable closure parameter"));
        }
        let callable = closure.invoke();
        let error = ClosureError::from_channel(callable.error().channel(), bridge, context)?;
        let parameters = callable
            .params()
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                ClosureParameter::from_declaration(index, parameter, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let returns = ClosureReturn::from_callable(callable, error.fallible(), bridge, context)?;
        let ty = Self::closure_type(&parameters, &returns, &error);
        Ok(Self {
            source,
            ty,
            box_binding: source_name.generated("box")?,
            context: source_name.generated("context")?,
            call: source_name.generated("call")?,
            release: source_name.generated("release")?,
            parameters,
            returns,
            error,
        })
    }

    pub fn parameter_ty(&self) -> TypeName {
        self.ty.clone().escaping()
    }

    pub fn arguments(&self) -> Vec<Expression> {
        vec![
            Expression::identifier(self.call.clone()),
            Expression::identifier(self.context.clone()),
            Expression::identifier(self.release.clone()),
        ]
    }

    pub fn wrap(&self, body: String, indent: &str) -> String {
        [
            Statement::let_value(
                &self.box_binding,
                Expression::call(
                    self.box_type(),
                    [Expression::identifier(self.source.clone())]
                        .into_iter()
                        .collect(),
                ),
            )
            .indented(indent),
            Statement::let_value(
                &self.context,
                Expression::call(
                    Expression::member(
                        Expression::call(
                            Expression::member("Unmanaged", "passRetained"),
                            [Expression::identifier(self.box_binding.clone())]
                                .into_iter()
                                .collect(),
                        ),
                        "toOpaque",
                    ),
                    ArgumentList::default(),
                ),
            )
            .indented(indent),
            self.call_statement(indent),
            self.release_statement(indent),
            body,
        ]
        .join("\n")
    }

    fn box_type(&self) -> TypeName {
        TypeName::new(format!("BoltFFIClosureBox<{}>", self.ty))
    }

    fn closure_type(
        parameters: &[ClosureParameter],
        returns: &ClosureReturn,
        error: &ClosureError,
    ) -> TypeName {
        match error.public_type() {
            Some(error) => TypeName::closure(
                parameters.iter().map(ClosureParameter::public_ty),
                Some(TypeName::result(returns.public_ty(), error.clone())),
            ),
            None => TypeName::closure(
                parameters.iter().map(ClosureParameter::public_ty),
                returns.public_ty.clone(),
            ),
        }
    }

    fn call_statement(&self, indent: &str) -> String {
        let bindings = std::iter::once("context".to_owned())
            .chain(
                self.parameters
                    .iter()
                    .flat_map(ClosureParameter::bindings)
                    .map(ToString::to_string),
            )
            .chain(self.returns.bindings().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join(", ");
        [
            format!(
                "{indent}let {}: @convention(c) ({}) -> {} = {{ {} in",
                self.call,
                self.c_parameter_types(),
                self.returns.c_ty,
                bindings
            ),
            self.call_body().indented(&format!("{indent}    ")),
            format!("{indent}}}"),
        ]
        .join("\n")
    }

    fn c_parameter_types(&self) -> String {
        std::iter::once(TypeName::new("UnsafeMutableRawPointer?"))
            .chain(self.parameters.iter().flat_map(ClosureParameter::c_types))
            .chain(self.returns.c_types())
            .map(|ty| ty.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn call_body(&self) -> Statement {
        let implementation = Expression::member(
            Expression::call(
                Expression::member(
                    Expression::call(
                        Expression::member(
                            Expression::new(format!("Unmanaged<{}>", self.box_type())),
                            "fromOpaque",
                        ),
                        [Expression::forced("context")].into_iter().collect(),
                    ),
                    "takeUnretainedValue",
                ),
                ArgumentList::default(),
            ),
            "invoke",
        );
        let call = Expression::call(
            implementation,
            self.parameters
                .iter()
                .map(ClosureParameter::argument)
                .collect(),
        );
        Statement::new(
            self.parameters
                .iter()
                .flat_map(ClosureParameter::setup)
                .chain(std::iter::once(self.error.statement(call, &self.returns)))
                .map(|statement| statement.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn release_statement(&self, indent: &str) -> String {
        [
            format!(
                "{indent}let {}: @convention(c) (UnsafeMutableRawPointer?) -> Void = {{ context in",
                self.release
            ),
            format!("{indent}    guard let context = context else {{ return }}"),
            format!(
                "{indent}    Unmanaged<{}>.fromOpaque(context).release()",
                self.box_type()
            ),
            format!("{indent}}}"),
        ]
        .join("\n")
    }
}

impl ClosureParameter {
    fn from_declaration(
        index: usize,
        declaration: &ParamDecl<Native, OutOfRust>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        declaration
            .payload()
            .as_value()
            .ok_or(SwiftHost::unsupported("closure nested closure parameter"))?
            .render_with(&mut ClosureParameterType {
                binding: Identifier::parse(format!("arg{index}"))?,
                bridge,
                context,
            })
    }

    fn public_ty(&self) -> TypeName {
        self.public_ty.clone()
    }

    fn bindings(&self) -> impl Iterator<Item = &Identifier> {
        self.bindings.iter().map(|binding| &binding.name)
    }

    fn c_types(&self) -> impl Iterator<Item = TypeName> + '_ {
        self.bindings.iter().map(|binding| binding.ty.clone())
    }

    fn setup(&self) -> Vec<Statement> {
        self.setup.clone()
    }

    fn argument(&self) -> Expression {
        self.argument.clone()
    }
}

impl ClosureParameterBinding {
    fn new(name: Identifier, ty: TypeName) -> Self {
        Self { name, ty }
    }
}

impl ClosureReturn {
    fn from_callable(
        callable: &CallableDecl<Native, ForeignBody>,
        fallible: bool,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        callable
            .returns()
            .plan()
            .render_with(&mut ClosureReturnType {
                bridge,
                context,
                fallible,
            })
    }

    fn statement(&self, call: Expression) -> Statement {
        self.success_statement(call)
    }

    fn success_statement(&self, call: Expression) -> Statement {
        let Some(success) = &self.success else {
            return match &self.conversion {
                ClosureReturnConversion::Void => Statement::expression(call),
                ClosureReturnConversion::Direct => Statement::returns(call),
                ClosureReturnConversion::FromC => {
                    Statement::returns(Expression::member(call, "cValue"))
                }
                ClosureReturnConversion::Encoded(encoded) => encoded.statement(call),
                ClosureReturnConversion::DirectVector(vector) => vector.statement(call),
                ClosureReturnConversion::CallbackHandle(handle) => {
                    Statement::returns(handle.c_handle(call))
                }
            };
        };
        match &self.conversion {
            ClosureReturnConversion::Void => Statement::new(
                [
                    Statement::expression(call).to_string(),
                    Statement::returns(Self::empty_error()).to_string(),
                ]
                .join("\n"),
            ),
            ClosureReturnConversion::Direct => self.success_out_statement(
                call,
                success,
                Expression::identifier(success.value.clone()),
            ),
            ClosureReturnConversion::FromC => self.success_out_statement(
                call,
                success,
                Expression::member(Expression::identifier(success.value.clone()), "cValue"),
            ),
            ClosureReturnConversion::Encoded(encoded) => encoded.success_statement(call, success),
            ClosureReturnConversion::DirectVector(vector) => {
                vector.success_statement(call, &success.binding, Self::empty_error())
            }
            ClosureReturnConversion::CallbackHandle(handle) => self.success_out_statement(
                call,
                success,
                handle.c_handle(Expression::identifier(success.value.clone())),
            ),
        }
    }

    fn fallible_success_statement(&self, call: Expression) -> Statement {
        match &self.conversion {
            ClosureReturnConversion::Void => Statement::returns(Self::empty_error()),
            _ => self.success_statement(call),
        }
    }

    fn public_ty(&self) -> TypeName {
        self.public_ty.clone().unwrap_or_else(TypeName::void)
    }

    fn c_types(&self) -> impl Iterator<Item = TypeName> + '_ {
        self.success
            .iter()
            .map(|success| success.ty.clone().mutable_pointer())
    }

    fn bindings(&self) -> impl Iterator<Item = &Identifier> {
        self.success.iter().map(|success| &success.binding)
    }

    fn success_out_statement(
        &self,
        call: Expression,
        success: &ClosureSuccess,
        value: Expression,
    ) -> Statement {
        Statement::new(
            [
                Statement::let_value(success.value(), call).to_string(),
                Statement::assign(
                    Expression::optional_chain_member(&success.binding, "pointee"),
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

impl ClosureSuccess {
    fn new(ty: TypeName) -> Result<Self> {
        Ok(Self {
            binding: Identifier::parse("return_out")?,
            value: Identifier::parse("boltffiClosureSuccess")?,
            ty,
        })
    }

    fn value(&self) -> &Identifier {
        &self.value
    }
}

impl ClosureError {
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
            } => EncodedClosureError::new(ty, codec, bridge, context)
                .map(Box::new)
                .map(Self::Encoded),
            ErrorChannel::Encoded { .. } => {
                Err(SwiftHost::unsupported("closure encoded error channel"))
            }
            ErrorChannel::Status => Err(SwiftHost::unsupported("closure status error channel")),
            _ => Err(SwiftHost::unsupported("unknown closure error channel")),
        }
    }

    fn fallible(&self) -> bool {
        !matches!(self, Self::None)
    }

    fn public_type(&self) -> Option<&TypeName> {
        match self {
            Self::None => None,
            Self::Encoded(error) => Some(error.public_type()),
        }
    }

    fn statement(&self, call: Expression, returns: &ClosureReturn) -> Statement {
        match self {
            Self::None => returns.statement(call),
            Self::Encoded(error) => error.statement(call, returns),
        }
    }
}

impl EncodedClosureError {
    fn new(
        ty: &TypeRef,
        codec: &<IntoRust as Direction>::Codec,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let buffer = ArgumentBuffer::from_parts(
            Identifier::parse("boltffiClosureErrorBytes")?,
            Identifier::parse("boltffiClosureErrorBuffer")?,
            Identifier::parse("boltffiClosureErrorWriter")?,
        );
        let value = Identifier::parse("boltffiClosureError")?;
        let write = codec
            .render_with(&mut Writer::new(
                buffer.writer().clone(),
                Self::error_value(ty, &value)?,
                context,
            ))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        Ok(Self {
            public_ty: SwiftType::type_ref(ty, context)?,
            success: Identifier::parse("boltffiClosureSuccess")?,
            value,
            buffer: buffer.with_statements(write),
            copy: Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
        })
    }

    fn public_type(&self) -> &TypeName {
        &self.public_ty
    }

    fn statement(&self, call: Expression, returns: &ClosureReturn) -> Statement {
        Statement::new(format!(
            "switch {call} {{\ncase .success(let {}):\n{}\ncase .failure(let {}):\n{}\n}}",
            self.success_binding(returns),
            returns
                .fallible_success_statement(self.success_expression(returns))
                .indented("    "),
            self.value,
            self.failure_statement().indented("    ")
        ))
    }

    fn failure_statement(&self) -> Statement {
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

    fn success_binding(&self, returns: &ClosureReturn) -> String {
        match returns.public_ty.is_some() {
            true => self.success.to_string(),
            false => "_".to_owned(),
        }
    }

    fn success_expression(&self, returns: &ClosureReturn) -> Expression {
        match returns.public_ty.is_some() {
            true => Expression::identifier(self.success.clone()),
            false => Expression::new("()"),
        }
    }

    fn copy_expression(&self) -> Expression {
        self.buffer.copy_expression(&self.copy)
    }

    fn error_value(ty: &TypeRef, value: &Identifier) -> Result<Expression> {
        match ty {
            TypeRef::String => Ok(Expression::member(value, "message")),
            TypeRef::Record(_) | TypeRef::Enum(_) => Ok(Expression::identifier(value.clone())),
            _ => Err(SwiftHost::unsupported("closure encoded error type")),
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ClosureParameterType<'_> {
    type Output = Result<ClosureParameter>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ()) -> Self::Output {
        let direct = DirectValue::new(ty, self.bridge, self.context)?;
        Ok(ClosureParameter {
            bindings: vec![ClosureParameterBinding::new(
                self.binding.clone(),
                direct.storage_type().clone(),
            )],
            public_ty: direct.api_type().clone(),
            setup: Vec::new(),
            argument: direct.swift_value(Expression::identifier(self.binding.clone())),
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
            return Err(SwiftHost::unsupported("encoded closure parameter shape"));
        }
        let pointer = self.suffixed("ptr")?;
        let length = self.suffixed("len")?;
        let reader = self.suffixed("reader")?;
        let decode = codec.render_with(&mut Reader::new(reader.clone(), self.context))?;
        let decode = decode.into_expression();
        Ok(ClosureParameter {
            bindings: vec![
                ClosureParameterBinding::new(
                    pointer.clone(),
                    TypeName::new("UnsafePointer<UInt8>?"),
                ),
                ClosureParameterBinding::new(length.clone(), TypeName::uint()),
            ],
            public_ty: SwiftType::type_ref(ty, self.context)?,
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
                                [Expression::identifier(length)].into_iter().collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            )],
            argument: decode,
        })
    }

    fn handle(
        &mut self,
        _: &'plan HandleTarget,
        _: <Native as Surface>::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        Err(SwiftHost::unsupported("handle closure parameter"))
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let pointer = self.suffixed("ptr")?;
        let length = self.suffixed("len")?;
        let reader = self.suffixed("reader")?;
        Ok(ClosureParameter {
            bindings: vec![
                ClosureParameterBinding::new(
                    pointer.clone(),
                    TypeName::new("UnsafePointer<UInt8>?"),
                ),
                ClosureParameterBinding::new(length.clone(), TypeName::uint()),
            ],
            public_ty: ScalarOption::new(primitive).ty()?,
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
                                [Expression::identifier(length)].into_iter().collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            )],
            argument: ScalarOption::new(primitive).read(reader)?,
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType, _: ()) -> Self::Output {
        let vector = DirectVector::from_element(element, self.bridge, self.context)?;
        let pointer = self.suffixed("ptr")?;
        let length = self.suffixed("len")?;
        let received = vector.received_with(
            pointer.clone(),
            length.clone(),
            self.suffixed("count")?,
            self.suffixed("value")?,
            self.suffixed("raw")?,
        )?;
        Ok(ClosureParameter {
            bindings: vec![
                ClosureParameterBinding::new(pointer, vector.pointer_ty()),
                ClosureParameterBinding::new(length, TypeName::uint()),
            ],
            public_ty: vector.ty().clone(),
            setup: received.setup(),
            argument: received.value(),
        })
    }
}

impl ClosureParameterType<'_> {
    fn suffixed(&self, suffix: &str) -> Result<Identifier> {
        Identifier::parse(format!("{}_{}", self.binding.as_str(), suffix))
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ClosureReturnType<'_> {
    type Output = Result<ClosureReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(ClosureReturn {
            c_ty: self.c_return_type(TypeName::void()),
            public_ty: None,
            conversion: ClosureReturnConversion::Void,
            success: None,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        let direct = DirectValue::new(ty, self.bridge, self.context)?;
        let conversion = match direct.converts_from_c() {
            true => ClosureReturnConversion::FromC,
            false => ClosureReturnConversion::Direct,
        };
        Ok(ClosureReturn {
            c_ty: self.c_return_type(direct.storage_type().clone()),
            public_ty: Some(direct.api_type().clone()),
            conversion,
            success: self.success(slot, direct.storage_type().clone())?,
        })
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot && !self.fallible {
            return Err(SwiftHost::unsupported("closure encoded out-pointer return"));
        }
        if shape != native::BufferShape::Buffer {
            return Err(SwiftHost::unsupported("encoded closure return shape"));
        }
        Ok(ClosureReturn {
            c_ty: self.c_return_type(TypeName::new("FfiBuf_u8")),
            public_ty: Some(SwiftType::type_ref(ty, self.context)?),
            conversion: ClosureReturnConversion::Encoded(EncodedClosureReturn::new(
                codec,
                self.bridge,
                self.context,
            )?),
            success: self.success(slot, TypeName::new("FfiBuf_u8"))?,
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
            HandleTarget::Callback(callback)
                if carrier == native::HandleCarrier::CallbackHandle =>
            {
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(ClosureReturn {
                    c_ty: self.c_return_type(TypeName::new("BoltFFICallbackHandle")),
                    public_ty: Some(handle.api_type()),
                    conversion: ClosureReturnConversion::CallbackHandle(handle),
                    success: self.success(slot, TypeName::new("BoltFFICallbackHandle"))?,
                })
            }
            HandleTarget::Callback(_) => {
                Err(SwiftHost::unsupported("callback closure return carrier"))
            }
            _ => Err(SwiftHost::unsupported("handle closure return")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(ClosureReturn {
            c_ty: self.c_return_type(TypeName::new("FfiBuf_u8")),
            public_ty: Some(ScalarOption::new(primitive).ty()?),
            conversion: ClosureReturnConversion::Encoded(EncodedClosureReturn::scalar_option(
                primitive,
                self.bridge,
            )?),
            success: None,
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        if self.fallible {
            return Err(SwiftHost::unsupported(
                "fallible direct-vector closure return",
            ));
        }
        let vector = DirectVector::from_element(element, self.bridge, self.context)?;
        Ok(ClosureReturn {
            c_ty: self.c_return_type(TypeName::new("FfiBuf_u8")),
            public_ty: Some(vector.ty().clone()),
            conversion: ClosureReturnConversion::DirectVector(vector.copied(
                GeneratedLocal::ReturnBuffer.identifier()?,
                Identifier::parse(self.bridge.support().buffer_from_bytes()?.name())?,
            )?),
            success: None,
        })
    }

    fn closure(&mut self, _: &'plan BindingClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(SwiftHost::unsupported("closure return from closure"))
    }
}

impl EncodedClosureReturn {
    fn new(
        codec: &<IntoRust as Direction>::Codec,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let result = Identifier::parse("boltffiClosureResult")?;
        let buffer = ArgumentBuffer::from_parts(
            Identifier::parse("boltffiClosureResultBytes")?,
            Identifier::parse("boltffiClosureResultBuffer")?,
            Identifier::parse("boltffiClosureResultWriter")?,
        );
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
        })
    }

    fn scalar_option(primitive: Primitive, bridge: &CBridgeContract) -> Result<Self> {
        let result = Identifier::parse("boltffiClosureResult")?;
        let buffer = Self::buffer()?;
        let write = vec![ScalarOption::new(primitive).write_statement(
            buffer.writer().clone(),
            Expression::identifier(result.clone()),
        )?];
        Ok(Self {
            result,
            buffer: buffer.with_statements(write),
            copy: Identifier::parse(bridge.support().buffer_from_bytes()?.name())?,
        })
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

    fn success_statement(&self, call: Expression, success: &ClosureSuccess) -> Statement {
        let store = Statement::assign(
            Expression::optional_chain_member(&success.binding, "pointee"),
            self.copy_expression(),
        );
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.buffer.bytes_statement().to_string(),
                self.buffer
                    .unsafe_buffer_scope(Statement::new(store.indented("    ")), ""),
                Statement::returns(ClosureReturn::empty_error()).to_string(),
            ]
            .join("\n"),
        )
    }

    fn copy_expression(&self) -> Expression {
        self.buffer.copy_expression(&self.copy)
    }

    fn buffer() -> Result<ArgumentBuffer> {
        Ok(ArgumentBuffer::from_parts(
            Identifier::parse("boltffiClosureResultBytes")?,
            Identifier::parse("boltffiClosureResultBuffer")?,
            Identifier::parse("boltffiClosureResultWriter")?,
        ))
    }
}

impl ClosureReturnType<'_> {
    fn c_return_type(&self, success: TypeName) -> TypeName {
        match self.fallible {
            true => TypeName::new("FfiBuf_u8"),
            false => success,
        }
    }

    fn success(&self, slot: ReturnValueSlot, ty: TypeName) -> Result<Option<ClosureSuccess>> {
        match (self.fallible, slot) {
            (false, ReturnValueSlot::ReturnSlot) => Ok(None),
            (true, ReturnValueSlot::OutPointer) => ClosureSuccess::new(ty).map(Some),
            (true, ReturnValueSlot::ReturnSlot) => {
                Err(SwiftHost::unsupported("fallible closure return slot"))
            }
            (false, ReturnValueSlot::OutPointer) => {
                Err(SwiftHost::unsupported("closure out-pointer return"))
            }
            _ => Err(SwiftHost::unsupported("unknown closure return slot")),
        }
    }
}
