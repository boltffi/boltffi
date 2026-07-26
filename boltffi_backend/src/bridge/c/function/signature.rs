use boltffi_binding::{
    CallableScope, ClosureParameter as BindingClosureParameter, ClosureReturn, ClosureSignature,
    DeclarationId, DirectValueType, DirectVectorElementType, Direction, ErrorDecl, ExecutionDecl,
    ExportedCallable, ForeignBody, HandlePresence, HandleTarget, IncomingParam, IntoRust, Native,
    NativeSymbol, OutOfRust, OutgoingParam, ParamDecl, ParamDirection, ParamPlan, ParamPlanRender,
    Primitive, Receive, ReturnPlan, ReturnPlanRender, ReturnValueSlot, RustBody, TypeRef, native,
};

use crate::core::{Error, Result};

use super::{Function, ReturnChannel, poll::PollHandleSymbols};
use crate::bridge::c::{
    C_BRIDGE_LAYER, DirectVectorElementAbi, Parameter, Type, name, names::Names,
};

#[derive(Clone, Debug)]
pub struct Signature {
    names: Names,
    receiver: Vec<Parameter>,
}

struct ValueParameter {
    signature: Signature,
    name: String,
}

struct ReturnParameters {
    signature: Signature,
    success_out: bool,
}

struct CallableReturnType {
    signature: Signature,
}

struct InfallibleCallbackReturnType {
    signature: Signature,
}

struct AsyncCallbackPayloadType {
    signature: Signature,
}

struct FallibleAsyncCallbackSuccess;

trait ReceiveAbi {
    fn needs_encoded_writeback(self) -> bool;
    fn needs_mutable_pointer(self) -> bool;
    fn direct_param_type(self, ty: &DirectValueType, value: Type) -> Type;
    fn direct_vector_pointer(
        self,
        name: &str,
        element: DirectVectorElementAbi,
    ) -> Result<Parameter>;
}

/// C ABI projection for the callable body behind a closure handle.
pub trait ClosureInvokeScope: CallableScope
where
    Self::ParamDirection: ParamDirection<Native>,
{
    fn parameters(
        signature: &Signature,
        params: &[ParamDecl<Native, Self::ParamDirection>],
    ) -> Result<Vec<Parameter>>;
}

impl ReceiveAbi for Receive {
    fn needs_encoded_writeback(self) -> bool {
        self == Receive::ByMutRef
    }

    fn needs_mutable_pointer(self) -> bool {
        self == Receive::ByMutRef
    }

    fn direct_param_type(self, ty: &DirectValueType, value: Type) -> Type {
        match (ty, self) {
            (DirectValueType::Record(_), Receive::ByRef) => Type::ConstPointer(Box::new(value)),
            (DirectValueType::Record(_), Receive::ByMutRef) => Type::MutPointer(Box::new(value)),
            _ => value,
        }
    }

    fn direct_vector_pointer(
        self,
        name: &str,
        element: DirectVectorElementAbi,
    ) -> Result<Parameter> {
        match self {
            Receive::ByMutRef => Parameter::mutable_direct_vector_pointer(name, element),
            _ => Parameter::direct_vector_pointer(name, element),
        }
    }
}

impl ReceiveAbi for () {
    fn needs_encoded_writeback(self) -> bool {
        false
    }

    fn needs_mutable_pointer(self) -> bool {
        false
    }

    fn direct_param_type(self, _: &DirectValueType, value: Type) -> Type {
        value
    }

    fn direct_vector_pointer(
        self,
        name: &str,
        element: DirectVectorElementAbi,
    ) -> Result<Parameter> {
        Parameter::direct_vector_pointer(name, element)
    }
}

impl ClosureInvokeScope for ForeignBody {
    fn parameters(
        signature: &Signature,
        params: &[ParamDecl<Native, Self::ParamDirection>],
    ) -> Result<Vec<Parameter>> {
        signature.imported_params(params)
    }
}

impl ClosureInvokeScope for RustBody {
    fn parameters(
        signature: &Signature,
        params: &[ParamDecl<Native, Self::ParamDirection>],
    ) -> Result<Vec<Parameter>> {
        signature.exported_params(params)
    }
}

impl CallableReturnType {
    fn direct_slot(&self, slot: ReturnValueSlot, ty: &DirectValueType) -> Result<Type> {
        match slot {
            ReturnValueSlot::ReturnSlot => self.signature.names.direct_value(ty),
            ReturnValueSlot::OutPointer => Ok(Type::Status),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct return slot",
            }),
        }
    }

    fn encoded_slot(&self, slot: ReturnValueSlot, shape: native::BufferShape) -> Result<Type> {
        match slot {
            ReturnValueSlot::ReturnSlot => self.signature.encoded_return(shape),
            ReturnValueSlot::OutPointer => Ok(Type::Status),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded return slot",
            }),
        }
    }

    fn handle_slot(
        &self,
        slot: ReturnValueSlot,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
    ) -> Result<Type> {
        match slot {
            ReturnValueSlot::ReturnSlot => Type::handle_target(target, carrier),
            ReturnValueSlot::OutPointer => Ok(Type::Status),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle return slot",
            }),
        }
    }

    fn buffer(&self) -> Type {
        Type::Buffer
    }

    fn status(&self) -> Type {
        Type::Status
    }
}

impl<'plan, D> ParamPlanRender<'plan, Native, D> for ValueParameter
where
    D: Direction,
    D::Receive: ReceiveAbi,
{
    type Output = Result<Vec<Parameter>>;

    fn direct(&mut self, ty: &'plan DirectValueType, receive: D::Receive) -> Self::Output {
        let value = self.signature.names.direct_value(ty)?;
        Ok(vec![Parameter::new(
            self.name.as_str(),
            receive.direct_param_type(ty, value),
        )?])
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
        receive: D::Receive,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => {
                // Only the in-place `&mut [u8]` case (no writeback out-param) takes a
                // mutable input pointer. Other `&mut` encoded params (e.g. `&mut [u64]`,
                // `&mut <record>`) keep a const input pointer plus an encoded writeback
                // out-param, matching the Rust extern and host renderers.
                let pointer = if receive.needs_mutable_pointer() && matches!(ty, TypeRef::Bytes) {
                    Parameter::mutable_byte_pointer(&self.name)?
                } else {
                    Parameter::byte_pointer(&self.name)?
                };
                let mut parameters = vec![pointer, Parameter::byte_length(&self.name)?];
                if receive.needs_encoded_writeback() && !matches!(ty, TypeRef::Bytes) {
                    parameters.push(Parameter::encoded_writeback(&self.name)?);
                }
                Ok(parameters)
            }
            native::BufferShape::Buffer | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded parameter shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "native encoded parameter shape",
            }),
        }
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
        _: D::Receive,
    ) -> Self::Output {
        Ok(vec![Parameter::new(
            self.name.as_str(),
            Type::handle_target(target, carrier)?,
        )?])
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(vec![
            Parameter::byte_pointer(&self.name)?,
            Parameter::byte_length(&self.name)?,
        ])
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        receive: D::Receive,
    ) -> Self::Output {
        self.signature
            .direct_vec_param(&self.name, element, receive)
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for ReturnParameters
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
    D::InvokeScope: ClosureInvokeScope,
{
    type Output = Result<Vec<Parameter>>;

    fn void(&mut self) -> Self::Output {
        Ok(Vec::new())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => {
                let ty = Type::MutPointer(Box::new(self.signature.names.direct_value(ty)?));
                self.out_parameter(ty).map(|parameter| vec![parameter])
            }
            ReturnValueSlot::ReturnSlot => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct return slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => match self.success_out {
                true => self.signature.encoded_return_out(shape),
                false => self.signature.encoded_out("return_out", shape),
            },
            ReturnValueSlot::ReturnSlot => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded return slot",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => {
                let ty = Type::MutPointer(Box::new(Type::handle_target(target, carrier)?));
                self.out_parameter(ty).map(|parameter| vec![parameter])
            }
            ReturnValueSlot::ReturnSlot => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle return slot",
            }),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(Vec::new())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(Vec::new())
    }

    fn closure(&mut self, closure: &'plan ClosureReturn<Native, D>) -> Self::Output {
        self.signature
            .closure_return_out(closure)
            .map(|param| vec![param])
    }
}

impl ReturnParameters {
    fn out_parameter(&self, ty: Type) -> Result<Parameter> {
        match self.success_out {
            true => Parameter::success_out("return_out", ty),
            false => Parameter::new("return_out", ty),
        }
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for CallableReturnType
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Type>;

    fn void(&mut self) -> Self::Output {
        Ok(Type::Void)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        self.direct_slot(slot, ty)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        self.encoded_slot(slot, shape)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        self.handle_slot(slot, target, carrier)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(self.buffer())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(self.buffer())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Ok(self.status())
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for InfallibleCallbackReturnType
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Type>;

    fn void(&mut self) -> Self::Output {
        Ok(Type::Void)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        CallableReturnType {
            signature: self.signature.clone(),
        }
        .direct_slot(slot, ty)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        CallableReturnType {
            signature: self.signature.clone(),
        }
        .encoded_slot(slot, shape)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        CallableReturnType {
            signature: self.signature.clone(),
        }
        .handle_slot(slot, target, carrier)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(CallableReturnType {
            signature: self.signature.clone(),
        }
        .buffer())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(CallableReturnType {
            signature: self.signature.clone(),
        }
        .buffer())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Ok(CallableReturnType {
            signature: self.signature.clone(),
        }
        .status())
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for AsyncCallbackPayloadType
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Option<Type>>;

    fn void(&mut self) -> Self::Output {
        Ok(None)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Some(self.signature.names.direct_value(ty)?)),
            ReturnValueSlot::OutPointer => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct async callback return slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Some(self.signature.encoded_return(shape)?)),
            ReturnValueSlot::OutPointer => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded async callback return slot",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Some(Type::handle_target(target, carrier)?)),
            ReturnValueSlot::OutPointer => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle async callback return slot",
            }),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(Some(Type::Buffer))
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(Some(Type::Buffer))
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "async callback closure return",
        })
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for FallibleAsyncCallbackSuccess
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<()>;

    fn void(&mut self) -> Self::Output {
        Ok(())
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct fallible async callback success slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        _: native::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded fallible async callback success slot",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle fallible async callback success slot",
            }),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "fallible async callback success slot",
        })
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "fallible async callback success slot",
        })
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "async callback closure success",
        })
    }
}

impl Signature {
    pub fn new(names: &Names, receiver: impl IntoIterator<Item = Parameter>) -> Self {
        Self {
            names: names.clone(),
            receiver: receiver.into_iter().collect(),
        }
    }

    pub fn exported(
        self,
        declaration: DeclarationId,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
    ) -> Result<Vec<Function>> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => self
                .synchronous(declaration, symbol, callable)
                .map(|function| vec![function]),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => self.async_poll_handle(
                declaration,
                callable,
                PollHandleSymbols::new(symbol, poll, complete, cancel, free, panic_message),
            ),
            ExecutionDecl::Asynchronous(
                native::AsyncProtocol::NativeFuture | native::AsyncProtocol::Continuation { .. },
            ) => Err(Error::UnsupportedCAbi {
                shape: "native async protocol",
            }),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::CallbackCompletion) => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "callback completion protocol on exported callable",
                })
            }
            ExecutionDecl::Asynchronous(_) => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native async protocol",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown execution declaration",
            }),
        }
    }

    fn synchronous(
        &self,
        declaration: DeclarationId,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
    ) -> Result<Function> {
        let params: Vec<Parameter> = self
            .receiver
            .clone()
            .into_iter()
            .chain(self.exported_params(callable.params())?)
            .chain(self.return_params(callable.returns().plan(), callable.error())?)
            .chain(self.error_params(callable.error())?)
            .collect();
        let returns = match (
            callable.returns().plan(),
            callable.error(),
            params.is_empty(),
        ) {
            (ReturnPlan::Void, ErrorDecl::None(_), true) => Type::Void,
            (ReturnPlan::Void, ErrorDecl::None(_), false) => Type::Status,
            (plan, error, _) => self.return_type(plan, error)?,
        };
        Function::exported_with_channel(
            declaration,
            symbol,
            params,
            returns,
            self.return_channel(callable.error()),
        )
    }

    fn async_poll_handle(
        &self,
        declaration: DeclarationId,
        callable: &ExportedCallable<Native>,
        symbols: PollHandleSymbols,
    ) -> Result<Vec<Function>> {
        let start = Function::exported(
            declaration,
            &symbols.start,
            self.receiver
                .clone()
                .into_iter()
                .chain(self.exported_params(callable.params())?)
                .collect(),
            Type::FutureHandle,
        )?;
        let complete_params = std::iter::once(Parameter::new("handle", Type::FutureHandle)?)
            .chain([Parameter::completion_status_out("out_status")?])
            .chain(self.return_params(callable.returns().plan(), callable.error())?)
            .collect();
        Ok(vec![
            start,
            Function::exported(
                declaration,
                &symbols.poll,
                vec![
                    Parameter::new("handle", Type::FutureHandle)?,
                    Parameter::continuation_data("callback")?,
                    Parameter::continuation_callback("callback", Type::Int8)?,
                ],
                Type::Void,
            )?,
            Function::exported_with_channel(
                declaration,
                &symbols.complete,
                complete_params,
                self.async_complete_return(callable.returns().plan(), callable.error())?,
                self.return_channel(callable.error()),
            )?,
            Function::exported(
                declaration,
                &symbols.panic_message,
                vec![Parameter::new("handle", Type::FutureHandle)?],
                Type::Buffer,
            )?,
            Function::exported(
                declaration,
                &symbols.cancel,
                vec![Parameter::new("handle", Type::FutureHandle)?],
                Type::Void,
            )?,
            Function::exported(
                declaration,
                &symbols.free,
                vec![Parameter::new("handle", Type::FutureHandle)?],
                Type::Void,
            )?,
        ])
    }

    fn exported_params(&self, params: &[ParamDecl<Native, IntoRust>]) -> Result<Vec<Parameter>> {
        params
            .iter()
            .map(|param| {
                let name = name::Spelling::new(param.name()).parameter();
                match param.payload() {
                    IncomingParam::Value(plan) => self.value_param(&name, plan),
                    IncomingParam::Closure(closure) => self.incoming_closure_param(&name, closure),
                }
            })
            .collect::<Result<Vec<_>>>()
            .map(Vec::into_iter)
            .map(|parameters| parameters.flatten().collect())
    }

    pub fn imported_params(
        &self,
        params: &[ParamDecl<Native, OutOfRust>],
    ) -> Result<Vec<Parameter>> {
        params
            .iter()
            .map(|param| {
                let name = name::Spelling::new(param.name()).parameter();
                match param.payload() {
                    OutgoingParam::Value(plan) => self.value_param(&name, plan),
                    OutgoingParam::Closure(closure) => self.outgoing_closure_param(&name, closure),
                }
            })
            .collect::<Result<Vec<_>>>()
            .map(Vec::into_iter)
            .map(|parameters| parameters.flatten().collect())
    }

    fn value_param<D>(&self, name: &str, plan: &ParamPlan<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Receive: ReceiveAbi,
    {
        plan.render_with(&mut ValueParameter {
            signature: self.clone(),
            name: name.to_owned(),
        })
    }

    fn direct_vec_param(
        &self,
        name: &str,
        element: &DirectVectorElementType,
        receive: impl ReceiveAbi,
    ) -> Result<Vec<Parameter>> {
        let element = DirectVectorElementAbi::from_binding(element)?;
        Ok(vec![
            receive.direct_vector_pointer(name, element.clone())?,
            Parameter::direct_vector_length(name, &element)?,
        ])
    }

    fn incoming_closure_param(
        &self,
        name: &str,
        closure: &BindingClosureParameter<Native, IntoRust>,
    ) -> Result<Vec<Parameter>> {
        let invoke = closure.invoke();
        self.closure_param(
            name,
            closure.signature(),
            self.imported_params(invoke.params())?,
            invoke.returns().plan(),
            invoke.error(),
        )
    }

    fn outgoing_closure_param(
        &self,
        name: &str,
        closure: &BindingClosureParameter<Native, OutOfRust>,
    ) -> Result<Vec<Parameter>> {
        let invoke = closure.invoke();
        self.closure_param(
            name,
            closure.signature(),
            self.exported_params(invoke.params())?,
            invoke.returns().plan(),
            invoke.error(),
        )
    }

    fn closure_param<D>(
        &self,
        name: &str,
        signature: &ClosureSignature,
        params: Vec<Parameter>,
        returns: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
        D::InvokeScope: ClosureInvokeScope,
    {
        let call_params = params;
        let return_params = self.callback_return_params(returns, error)?;
        let closure_params = call_params
            .iter()
            .chain(return_params.iter())
            .cloned()
            .collect::<Vec<_>>();
        Ok(vec![
            Parameter::closure_call(
                name,
                signature,
                Type::FunctionPointer {
                    returns: Box::new(self.callback_return_type(returns, error)?),
                    params: std::iter::once(Type::MutPointer(Box::new(Type::Void)))
                        .chain(call_params.iter().map(|parameter| parameter.ty().clone()))
                        .chain(return_params.iter().map(|parameter| parameter.ty().clone()))
                        .collect(),
                },
                closure_params,
            )?,
            Parameter::closure_context(name)?,
            Parameter::closure_release(name)?,
        ])
    }

    fn closure_return_out<D>(&self, closure: &ClosureReturn<Native, D>) -> Result<Parameter>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
        D::InvokeScope: ClosureInvokeScope,
    {
        let invoke = closure.invoke();
        let call_params = D::InvokeScope::parameters(self, invoke.params())?;
        let return_params = self.callback_return_params(invoke.returns().plan(), invoke.error())?;
        let closure_params = call_params
            .iter()
            .chain(return_params.iter())
            .cloned()
            .collect::<Vec<_>>();
        let call_type = Type::FunctionPointer {
            returns: Box::new(self.callback_return_type(invoke.returns().plan(), invoke.error())?),
            params: std::iter::once(Type::MutPointer(Box::new(Type::Void)))
                .chain(call_params.iter().map(|parameter| parameter.ty().clone()))
                .chain(return_params.iter().map(|parameter| parameter.ty().clone()))
                .collect(),
        };
        Parameter::closure_return("return_out", closure.signature(), call_type, closure_params)
    }

    fn return_params<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
        D::InvokeScope: ClosureInvokeScope,
    {
        self.return_params_with(
            plan,
            matches!(
                error,
                ErrorDecl::StatusViaReturnSlot { .. } | ErrorDecl::EncodedViaReturnSlot { .. }
            ),
        )
    }

    fn return_params_with<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        success_out: bool,
    ) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
        D::InvokeScope: ClosureInvokeScope,
    {
        plan.render_with(&mut ReturnParameters {
            signature: self.clone(),
            success_out,
        })
    }

    fn error_params<D>(&self, error: &ErrorDecl<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
    {
        match error {
            ErrorDecl::StatusViaOutPointer { .. } => Ok(vec![Parameter::new(
                "error_out",
                Type::MutPointer(Box::new(Type::Status)),
            )?]),
            ErrorDecl::EncodedViaOutPointer { shape, .. } => self.encoded_out("error_out", *shape),
            ErrorDecl::None(_)
            | ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::EncodedViaReturnSlot { .. } => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown error declaration",
            }),
        }
    }

    fn return_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::StatusViaReturnSlot { repr } => Type::primitive(repr.primitive()),
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => self.encoded_return(*shape),
            ErrorDecl::None(_)
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => self.return_slot_type(plan),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown error declaration",
            }),
        }
    }

    fn return_channel<D>(&self, error: &ErrorDecl<Native, D>) -> ReturnChannel
    where
        D: Direction,
    {
        match error {
            ErrorDecl::EncodedViaReturnSlot { .. } => ReturnChannel::EncodedError,
            _ => ReturnChannel::Value,
        }
    }

    fn return_slot_type<D>(&self, plan: &ReturnPlan<Native, D>) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut CallableReturnType {
            signature: self.clone(),
        })
    }

    fn async_complete_return<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => self.encoded_return(*shape),
            ErrorDecl::None(_) => plan.render_with(&mut InfallibleCallbackReturnType {
                signature: self.clone(),
            }),
            ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => Err(Error::UnsupportedCAbi {
                shape: "async error channel",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown async error channel",
            }),
        }
    }

    pub fn encoded_return(&self, shape: native::BufferShape) -> Result<Type> {
        match shape {
            native::BufferShape::Buffer => Ok(Type::Buffer),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded return shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native encoded return shape",
            }),
        }
    }

    fn encoded_out(&self, name: &str, shape: native::BufferShape) -> Result<Vec<Parameter>> {
        match shape {
            native::BufferShape::Buffer => Ok(vec![Parameter::new(
                name,
                Type::MutPointer(Box::new(Type::Buffer)),
            )?]),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded out-pointer shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native encoded out-pointer shape",
            }),
        }
    }

    fn encoded_return_out(&self, shape: native::BufferShape) -> Result<Vec<Parameter>> {
        match shape {
            native::BufferShape::Buffer => Ok(vec![Parameter::success_out(
                "return_out",
                Type::MutPointer(Box::new(Type::Buffer)),
            )?]),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded return out-pointer shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native encoded return out-pointer shape",
            }),
        }
    }

    pub fn callback_return_params<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
        D::InvokeScope: ClosureInvokeScope,
    {
        self.return_params(plan, error)
    }

    pub fn callback_return_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::None(_) => plan.render_with(&mut InfallibleCallbackReturnType {
                signature: self.clone(),
            }),
            _ => self.return_type(plan, error),
        }
    }

    pub fn async_completion<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        let result = self.async_callback_payload_type(plan, error)?;
        Ok(Type::FunctionPointer {
            returns: Box::new(Type::Void),
            params: std::iter::once(Type::MutPointer(Box::new(Type::Void)))
                .chain([Type::Status])
                .chain(result)
                .collect(),
        })
    }

    fn async_callback_payload_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Option<Type>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::None(_) => self.infallible_async_callback_payload_type(plan),
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => {
                self.encoded_return(*shape)?;
                self.validate_fallible_async_callback_success(plan)?;
                Ok(Some(Type::Buffer))
            }
            ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => Err(Error::UnsupportedCAbi {
                shape: "async callback error channel",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown async callback error channel",
            }),
        }
    }

    fn infallible_async_callback_payload_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
    ) -> Result<Option<Type>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut AsyncCallbackPayloadType {
            signature: self.clone(),
        })
    }

    fn validate_fallible_async_callback_success<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
    ) -> Result<()>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut FallibleAsyncCallbackSuccess)
    }
}
