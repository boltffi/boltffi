use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use boltffi_ast::FnTraitKind;

use crate::{
    AsyncProtocolIntrospect, BindingError, BindingErrorKind, BufferShapeRules, BuiltinType,
    CallableScope, CanonicalName, ClosureRegistrationIntrospect, ClosureSignature, DirectValueType,
    DirectVectorElementType, Direction, ElementMeta, ForeignBody, HandlePresence, HandleTarget,
    IntegerRepr, IntoRust, NativeSymbol, OutOfRust, Primitive, RustBody, Surface, TypeRef,
};

/// One call shape ready to be turned into target code.
///
/// Carries the receiver mode, the parameter crossings, the return
/// crossing, the error channel, and the execution kind. The call site
/// (linker symbol or vtable slot) lives on the owning declaration, not
/// on the callable.
///
/// `S` is the target surface. `K` is the body scope; its
/// `ParamDirection` flows into every parameter and its `ReturnDirection`
/// flows into the return and the error channel.
///
/// # Example
///
/// `fn add(a: i32, b: i32) -> i32` lowers to a
/// `CallableDecl<S, RustBody>` with no receiver, two
/// `ParamPlan::Direct` parameters, a `ReturnPlan::DirectViaReturnSlot`
/// return, `ErrorDecl::None`, and synchronous execution.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, S::IncomingClosureRegistration: Serialize, S::OutgoingClosureRegistration: Serialize, K::ParamDirection: ParamDirection<S>, K::ReturnDirection: Direction, <K::ParamDirection as ParamDirection<S>>::Payload: Serialize, <K::ReturnDirection as Direction>::Codec: Serialize, <K::ReturnDirection as Direction>::Receive: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, S::IncomingClosureRegistration: serde::de::DeserializeOwned, S::OutgoingClosureRegistration: serde::de::DeserializeOwned, K::ParamDirection: ParamDirection<S>, K::ReturnDirection: Direction, <K::ParamDirection as ParamDirection<S>>::Payload: serde::de::DeserializeOwned, <K::ReturnDirection as Direction>::Codec: serde::de::DeserializeOwned, <K::ReturnDirection as Direction>::Receive: serde::de::DeserializeOwned"
))]
pub struct CallableDecl<S: Surface, K: CallableScope>
where
    K::ParamDirection: ParamDirection<S>,
{
    receiver: Option<Receive>,
    params: Vec<ParamDecl<S, K::ParamDirection>>,
    returns: ReturnDecl<S, K::ReturnDirection>,
    error: ErrorDecl<S, K::ReturnDirection>,
    execution: ExecutionDecl<S>,
}

impl<S: Surface, K: CallableScope> CallableDecl<S, K>
where
    K::ParamDirection: ParamDirection<S>,
{
    pub(crate) fn new(
        receiver: Option<Receive>,
        params: Vec<ParamDecl<S, K::ParamDirection>>,
        returns: ReturnDecl<S, K::ReturnDirection>,
        error: ErrorDecl<S, K::ReturnDirection>,
        execution: ExecutionDecl<S>,
    ) -> Result<Self, BindingError> {
        let callable = Self {
            receiver,
            params,
            returns,
            error,
            execution,
        };
        callable.validate()?;
        Ok(callable)
    }

    /// Checks the slot-conflict and buffer-shape invariants.
    ///
    /// Fails when:
    /// - both the return and the error channel use the native return
    ///   slot;
    /// - an encoded param has a buffer shape forbidden on params for
    ///   this surface (e.g. `wasm32::BufferShape::Packed`);
    /// - an encoded return or error has a buffer shape forbidden on
    ///   return slots (e.g. any `Slice`).
    ///
    /// `Bindings::validate` calls this on every callable.
    pub fn validate(&self) -> Result<(), BindingError> {
        if self.returns.plan().uses_return_slot() && self.error.uses_return_slot() {
            return Err(BindingError::new(BindingErrorKind::ReturnSlotConflict));
        }
        for param in &self.params {
            if let Some(shape) = param.buffer_shape()
                && !shape.is_valid_in_param()
            {
                return Err(BindingError::new(BindingErrorKind::PackedInParamPosition));
            }
        }
        if let Some(shape) = self.returns.plan().buffer_shape()
            && !shape.is_valid_in_return()
        {
            return Err(BindingError::new(BindingErrorKind::SliceInReturnPosition));
        }
        if let Some(shape) = self.error.buffer_shape()
            && !shape.is_valid_in_return()
        {
            return Err(BindingError::new(BindingErrorKind::SliceInReturnPosition));
        }
        Ok(())
    }

    /// Returns the receiver mode, or `None` for free functions and
    /// static methods.
    pub const fn receiver(&self) -> Option<Receive> {
        self.receiver
    }

    /// Returns the parameters in call order.
    pub fn params(&self) -> &[ParamDecl<S, K::ParamDirection>] {
        &self.params
    }

    /// Returns the return shape.
    pub fn returns(&self) -> &ReturnDecl<S, K::ReturnDirection> {
        &self.returns
    }

    /// Returns the error transport.
    pub fn error(&self) -> &ErrorDecl<S, K::ReturnDirection> {
        &self.error
    }

    /// Returns the execution mode.
    pub fn execution(&self) -> &ExecutionDecl<S> {
        &self.execution
    }

    /// Iterates the native symbols this callable references.
    ///
    /// Covers symbols carried on the receiver, parameter, return, and
    /// execution lanes so a closure handed out through a return slot
    /// pulls its registration symbols (`OutgoingClosureRegistration`'s
    /// `call` and `free` exports on wasm32, for example) into the
    /// declaration's symbol set.
    pub fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        let param_symbols = self.params.iter().flat_map(ParamDecl::native_symbols);
        let return_symbols = self.returns.native_symbols();
        let execution_symbols: Box<dyn Iterator<Item = &NativeSymbol> + '_> = match &self.execution
        {
            ExecutionDecl::Synchronous(_) => Box::new(std::iter::empty()),
            ExecutionDecl::Asynchronous(protocol) => protocol.native_symbols(),
        };
        Box::new(param_symbols.chain(return_symbols).chain(execution_symbols))
    }

    /// Returns whether any value crossing in this callable uses a result codec.
    pub fn uses_result_codec(&self) -> bool {
        self.params.iter().any(ParamDecl::uses_result_codec)
            || self.returns.uses_result_codec()
            || self.error.uses_result_codec()
    }

    /// Returns whether any value crossing in this callable uses the given builtin codec.
    pub fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.params
            .iter()
            .any(|param| param.uses_builtin_codec(kind))
            || self.returns.uses_builtin_codec(kind)
            || self.error.uses_builtin_codec(kind)
    }

    /// Returns whether any value crossing in this callable uses a direct record vector.
    pub fn uses_direct_record_vector(&self) -> bool {
        self.params.iter().any(ParamDecl::uses_direct_record_vector)
            || self.returns.uses_direct_record_vector()
    }

    /// Returns whether this callable uses an asynchronous execution protocol.
    pub fn uses_async_execution(&self) -> bool {
        self.execution.uses_async_execution()
    }
}

/// A callable whose body is implemented in Rust. Foreign code calls
/// in. Used for free functions, record/enum/class methods, and
/// initializers.
pub type ExportedCallable<S> = CallableDecl<S, RustBody>;

/// A callable whose body is implemented in foreign code. Rust calls
/// out. Used for callback trait methods and inline closure
/// invocations.
pub type ImportedCallable<S> = CallableDecl<S, ForeignBody>;

/// Direction-specific payload carried by a parameter declaration.
pub trait ParamDirection<S: Surface>: Direction {
    /// Concrete payload shape admitted by this direction.
    type Payload: Clone + Debug + Eq + Hash + PartialEq + Serialize + for<'de> Deserialize<'de>;

    /// Wraps a value crossing as this direction's parameter payload.
    fn value_payload(plan: ParamPlan<S, Self>) -> Self::Payload;

    /// Returns the encoded buffer shape when the payload carries one.
    fn buffer_shape(payload: &Self::Payload) -> Option<S::BufferShape>;

    /// Iterates over native symbols referenced by the payload.
    fn native_symbols(payload: &Self::Payload) -> Box<dyn Iterator<Item = &NativeSymbol> + '_>;

    /// Returns whether the payload carries a result codec.
    fn uses_result_codec(payload: &Self::Payload) -> bool;

    /// Returns whether the payload carries the given builtin codec.
    fn uses_builtin_codec(payload: &Self::Payload, kind: BuiltinType) -> bool;

    /// Returns whether the payload carries a direct record vector.
    fn uses_direct_record_vector(payload: &Self::Payload) -> bool;
}

/// One incoming parameter crossing.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::IncomingClosureRegistration: Serialize, S::OutgoingClosureRegistration: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::IncomingClosureRegistration: serde::de::DeserializeOwned, S::OutgoingClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub enum IncomingParam<S: Surface> {
    /// One value crossing into Rust.
    Value(ParamPlan<S, IntoRust>),
    /// Inline closure callback crossing into Rust.
    Closure(ClosureParameter<S, IntoRust>),
}

impl<S: Surface> IncomingParam<S> {
    /// Returns the value crossing plan if this payload carries one.
    pub fn as_value(&self) -> Option<&ParamPlan<S, IntoRust>> {
        match self {
            Self::Value(plan) => Some(plan),
            Self::Closure(_) => None,
        }
    }

    /// Returns the incoming closure if this payload carries one.
    pub fn as_closure(&self) -> Option<&ClosureParameter<S, IntoRust>> {
        match self {
            Self::Closure(closure) => Some(closure),
            Self::Value(_) => None,
        }
    }
}

/// One outgoing parameter crossing.
///
/// Mirrors [`IncomingParam`] in the opposite direction. Outgoing
/// closures appear when Rust hands a closure handle out through a
/// foreign-implemented callable (callback trait method); the closure
/// body lives on the Rust side.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::IncomingClosureRegistration: Serialize, S::OutgoingClosureRegistration: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::IncomingClosureRegistration: serde::de::DeserializeOwned, S::OutgoingClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub enum OutgoingParam<S: Surface> {
    /// One value crossing out of Rust.
    Value(ParamPlan<S, OutOfRust>),
    /// Inline closure callback crossing out of Rust.
    Closure(ClosureParameter<S, OutOfRust>),
}

impl<S: Surface> OutgoingParam<S> {
    pub(crate) fn from_value(plan: ParamPlan<S, OutOfRust>) -> Self {
        Self::Value(plan)
    }

    /// Returns the value crossing plan if this payload carries one.
    pub fn as_value(&self) -> Option<&ParamPlan<S, OutOfRust>> {
        match self {
            Self::Value(plan) => Some(plan),
            Self::Closure(_) => None,
        }
    }

    /// Returns the outgoing closure if this payload carries one.
    pub fn as_closure(&self) -> Option<&ClosureParameter<S, OutOfRust>> {
        match self {
            Self::Closure(closure) => Some(closure),
            Self::Value(_) => None,
        }
    }
}

impl<S: Surface> ParamDirection<S> for IntoRust {
    type Payload = IncomingParam<S>;

    fn value_payload(plan: ParamPlan<S, Self>) -> Self::Payload {
        IncomingParam::Value(plan)
    }

    fn buffer_shape(payload: &Self::Payload) -> Option<S::BufferShape> {
        match payload {
            IncomingParam::Value(plan) => plan.buffer_shape(),
            IncomingParam::Closure(_) => None,
        }
    }

    fn native_symbols(payload: &Self::Payload) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        match payload {
            IncomingParam::Value(_) => Box::new(std::iter::empty()),
            IncomingParam::Closure(closure) => closure.native_symbols(),
        }
    }

    fn uses_result_codec(payload: &Self::Payload) -> bool {
        match payload {
            IncomingParam::Value(plan) => plan.uses_result_codec(),
            IncomingParam::Closure(closure) => closure.uses_result_codec(),
        }
    }

    fn uses_builtin_codec(payload: &Self::Payload, kind: BuiltinType) -> bool {
        match payload {
            IncomingParam::Value(plan) => plan.uses_builtin_codec(kind),
            IncomingParam::Closure(closure) => closure.uses_builtin_codec(kind),
        }
    }

    fn uses_direct_record_vector(payload: &Self::Payload) -> bool {
        match payload {
            IncomingParam::Value(plan) => plan.uses_direct_record_vector(),
            IncomingParam::Closure(closure) => closure.uses_direct_record_vector(),
        }
    }
}

impl<S: Surface> ParamDirection<S> for OutOfRust {
    type Payload = OutgoingParam<S>;

    fn value_payload(plan: ParamPlan<S, Self>) -> Self::Payload {
        OutgoingParam::from_value(plan)
    }

    fn buffer_shape(payload: &Self::Payload) -> Option<S::BufferShape> {
        match payload {
            OutgoingParam::Value(plan) => plan.buffer_shape(),
            OutgoingParam::Closure(_) => None,
        }
    }

    fn native_symbols(payload: &Self::Payload) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        match payload {
            OutgoingParam::Value(_) => Box::new(std::iter::empty()),
            OutgoingParam::Closure(closure) => closure.native_symbols(),
        }
    }

    fn uses_result_codec(payload: &Self::Payload) -> bool {
        match payload {
            OutgoingParam::Value(plan) => plan.uses_result_codec(),
            OutgoingParam::Closure(closure) => closure.uses_result_codec(),
        }
    }

    fn uses_builtin_codec(payload: &Self::Payload, kind: BuiltinType) -> bool {
        match payload {
            OutgoingParam::Value(plan) => plan.uses_builtin_codec(kind),
            OutgoingParam::Closure(closure) => closure.uses_builtin_codec(kind),
        }
    }

    fn uses_direct_record_vector(payload: &Self::Payload) -> bool {
        match payload {
            OutgoingParam::Value(plan) => plan.uses_direct_record_vector(),
            OutgoingParam::Closure(closure) => closure.uses_direct_record_vector(),
        }
    }
}

/// One named parameter crossing.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "D: ParamDirection<S>, D::Payload: Serialize",
    deserialize = "D: ParamDirection<S>, D::Payload: serde::de::DeserializeOwned"
))]
pub struct ParamDecl<S: Surface, D: ParamDirection<S>> {
    name: CanonicalName,
    meta: ElementMeta,
    payload: D::Payload,
}

impl<S: Surface, D: ParamDirection<S>> ParamDecl<S, D> {
    /// Returns the parameter name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    /// Returns the direction-specific payload.
    pub fn payload(&self) -> &D::Payload {
        &self.payload
    }

    pub(crate) fn value(name: CanonicalName, meta: ElementMeta, plan: ParamPlan<S, D>) -> Self {
        Self {
            name,
            meta,
            payload: D::value_payload(plan),
        }
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        D::buffer_shape(&self.payload)
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        D::native_symbols(&self.payload)
    }

    fn uses_result_codec(&self) -> bool {
        D::uses_result_codec(&self.payload)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        D::uses_builtin_codec(&self.payload, kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        D::uses_direct_record_vector(&self.payload)
    }
}

impl<S: Surface> ParamDecl<S, IntoRust> {
    pub(crate) fn as_value(&self) -> Option<&ParamPlan<S, IntoRust>> {
        self.payload.as_value()
    }

    pub(crate) fn as_closure(&self) -> Option<&ClosureParameter<S, IntoRust>> {
        self.payload.as_closure()
    }

    pub(crate) fn closure(
        name: CanonicalName,
        meta: ElementMeta,
        closure: ClosureParameter<S, IntoRust>,
    ) -> Self {
        Self {
            name,
            meta,
            payload: IncomingParam::Closure(closure),
        }
    }
}

impl<S: Surface> ParamDecl<S, OutOfRust> {
    pub(crate) fn as_value(&self) -> Option<&ParamPlan<S, OutOfRust>> {
        self.payload.as_value()
    }

    pub(crate) fn as_closure(&self) -> Option<&ClosureParameter<S, OutOfRust>> {
        self.payload.as_closure()
    }

    pub(crate) fn closure(
        name: CanonicalName,
        meta: ElementMeta,
        closure: ClosureParameter<S, OutOfRust>,
    ) -> Self {
        Self {
            name,
            meta,
            payload: OutgoingParam::Closure(closure),
        }
    }
}

/// Closure payload at a parameter slot.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, D::Receive: Serialize, D::Codec: Serialize, <D as Direction>::ClosureRegistrationShape<S>: Serialize, <D::Opposite as Direction>::Codec: Serialize, <D::Opposite as Direction>::Receive: Serialize, <D::Opposite as ParamDirection<S>>::Payload: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned, D::Codec: serde::de::DeserializeOwned, <D as Direction>::ClosureRegistrationShape<S>: serde::de::DeserializeOwned, <D::Opposite as Direction>::Codec: serde::de::DeserializeOwned, <D::Opposite as Direction>::Receive: serde::de::DeserializeOwned, <D::Opposite as ParamDirection<S>>::Payload: serde::de::DeserializeOwned"
))]
pub struct ClosureParameter<S: Surface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    crossing: ClosureCrossing<S, D>,
}

impl<S: Surface, D: Direction> ClosureParameter<S, D>
where
    D::Opposite: ParamDirection<S>,
{
    pub(crate) fn new(
        form: ClosureForm,
        signature: ClosureSignature,
        presence: HandlePresence,
        registration: ClosureRegistration<S, D>,
        invoke: CallableDecl<S, D::InvokeScope>,
    ) -> Self {
        Self {
            crossing: ClosureCrossing::new(form, signature, presence, registration, invoke),
        }
    }

    /// Returns the source spelling.
    pub fn form(&self) -> ClosureForm {
        self.crossing.form()
    }

    /// Returns the closure invocation signature.
    pub fn signature(&self) -> &ClosureSignature {
        self.crossing.signature()
    }

    /// Returns whether the closure crossing may be absent.
    pub fn presence(&self) -> HandlePresence {
        self.crossing.presence()
    }

    /// Returns the handle registration.
    pub fn registration(&self) -> &ClosureRegistration<S, D> {
        self.crossing.registration()
    }

    /// Returns the invocation contract.
    pub fn invoke(&self) -> &CallableDecl<S, D::InvokeScope> {
        self.crossing.invoke()
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        self.crossing.native_symbols()
    }

    fn uses_result_codec(&self) -> bool {
        self.crossing.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.crossing.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.crossing.uses_direct_record_vector()
    }
}

/// Closure payload at a return slot.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, D::Receive: Serialize, D::Codec: Serialize, <D as Direction>::ClosureRegistrationShape<S>: Serialize, <D::Opposite as Direction>::Codec: Serialize, <D::Opposite as Direction>::Receive: Serialize, <D::Opposite as ParamDirection<S>>::Payload: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned, D::Codec: serde::de::DeserializeOwned, <D as Direction>::ClosureRegistrationShape<S>: serde::de::DeserializeOwned, <D::Opposite as Direction>::Codec: serde::de::DeserializeOwned, <D::Opposite as Direction>::Receive: serde::de::DeserializeOwned, <D::Opposite as ParamDirection<S>>::Payload: serde::de::DeserializeOwned"
))]
pub struct ClosureReturn<S: Surface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    crossing: ClosureCrossing<S, D>,
}

impl<S: Surface, D: Direction> ClosureReturn<S, D>
where
    D::Opposite: ParamDirection<S>,
{
    pub(crate) fn new(
        form: ClosureForm,
        signature: ClosureSignature,
        presence: HandlePresence,
        registration: ClosureRegistration<S, D>,
        invoke: CallableDecl<S, D::InvokeScope>,
    ) -> Self {
        Self {
            crossing: ClosureCrossing::new(form, signature, presence, registration, invoke),
        }
    }

    /// Returns the source spelling.
    pub fn form(&self) -> ClosureForm {
        self.crossing.form()
    }

    /// Returns the closure invocation signature.
    pub fn signature(&self) -> &ClosureSignature {
        self.crossing.signature()
    }

    /// Returns whether the closure crossing may be absent.
    pub fn presence(&self) -> HandlePresence {
        self.crossing.presence()
    }

    /// Returns the handle registration.
    pub fn registration(&self) -> &ClosureRegistration<S, D> {
        self.crossing.registration()
    }

    /// Returns the invocation contract.
    pub fn invoke(&self) -> &CallableDecl<S, D::InvokeScope> {
        self.crossing.invoke()
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        self.crossing.native_symbols()
    }

    fn uses_result_codec(&self) -> bool {
        self.crossing.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.crossing.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.crossing.uses_direct_record_vector()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, D::Receive: Serialize, D::Codec: Serialize, <D as Direction>::ClosureRegistrationShape<S>: Serialize, <D::Opposite as Direction>::Codec: Serialize, <D::Opposite as Direction>::Receive: Serialize, <D::Opposite as ParamDirection<S>>::Payload: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned, D::Codec: serde::de::DeserializeOwned, <D as Direction>::ClosureRegistrationShape<S>: serde::de::DeserializeOwned, <D::Opposite as Direction>::Codec: serde::de::DeserializeOwned, <D::Opposite as Direction>::Receive: serde::de::DeserializeOwned, <D::Opposite as ParamDirection<S>>::Payload: serde::de::DeserializeOwned"
))]
struct ClosureCrossing<S: Surface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    form: ClosureForm,
    signature: ClosureSignature,
    presence: HandlePresence,
    registration: ClosureRegistration<S, D>,
    invoke: Box<CallableDecl<S, D::InvokeScope>>,
}

impl<S: Surface, D: Direction> ClosureCrossing<S, D>
where
    D::Opposite: ParamDirection<S>,
{
    fn new(
        form: ClosureForm,
        signature: ClosureSignature,
        presence: HandlePresence,
        registration: ClosureRegistration<S, D>,
        invoke: CallableDecl<S, D::InvokeScope>,
    ) -> Self {
        Self {
            form,
            signature,
            presence,
            registration,
            invoke: Box::new(invoke),
        }
    }

    pub fn form(&self) -> ClosureForm {
        self.form
    }

    pub fn signature(&self) -> &ClosureSignature {
        &self.signature
    }

    pub fn presence(&self) -> HandlePresence {
        self.presence
    }

    pub fn registration(&self) -> &ClosureRegistration<S, D> {
        &self.registration
    }

    pub fn invoke(&self) -> &CallableDecl<S, D::InvokeScope> {
        &self.invoke
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        Box::new(
            self.registration
                .native_symbols()
                .chain(self.invoke.native_symbols()),
        )
    }

    fn uses_result_codec(&self) -> bool {
        self.invoke.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.invoke.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.invoke.uses_direct_record_vector()
    }
}

/// The source spelling of a closure parameter.
///
/// Every form crosses the wire the same way; renderers consult this
/// when emitting the Rust-side binding so the user-facing trait bound
/// (`Fn`, `FnMut`, `FnOnce`, or a bare function pointer) matches what
/// the user wrote.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ClosureForm {
    /// Bare `fn(...)` function-pointer parameter.
    FunctionPointer,
    /// `impl Fn(...)` parameter.
    Fn,
    /// `impl FnMut(...)` parameter.
    FnMut,
    /// `impl FnOnce(...)` parameter.
    FnOnce,
}

impl From<FnTraitKind> for ClosureForm {
    fn from(trait_kind: FnTraitKind) -> Self {
        match trait_kind {
            FnTraitKind::Fn => Self::Fn,
            FnTraitKind::FnMut => Self::FnMut,
            FnTraitKind::FnOnce => Self::FnOnce,
        }
    }
}

/// The handle crossing for a closure parameter.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "D: Direction, <D as Direction>::ClosureRegistrationShape<S>: Serialize, D::Receive: Serialize",
    deserialize = "D: Direction, <D as Direction>::ClosureRegistrationShape<S>: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned"
))]
pub struct ClosureRegistration<S: Surface, D: Direction> {
    shape: D::ClosureRegistrationShape<S>,
    receive: D::Receive,
}

impl<S: Surface, D: Direction> ClosureRegistration<S, D> {
    pub(crate) fn new(shape: D::ClosureRegistrationShape<S>, receive: D::Receive) -> Self {
        Self { shape, receive }
    }

    /// Returns the surface registration shape.
    pub fn shape(&self) -> &D::ClosureRegistrationShape<S> {
        &self.shape
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        self.shape.native_symbols()
    }

    /// Returns the receive mode of the registration slot.
    pub fn receive(&self) -> D::Receive {
        self.receive
    }
}

/// How one value crosses the boundary as a parameter slot in
/// direction `D`.
///
/// Each variant describes a distinct wire shape and is reachable
/// independently. `D::Codec` picks the foreign-side codec orientation
/// for encoded crossings, and `D::Receive` picks the Rust-side receive
/// mode for slots that have one.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, D: Direction, D::Codec: Serialize, D::Receive: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ParamPlan<S: Surface, D: Direction> {
    /// Value occupies a native call slot directly.
    Direct {
        /// Foreign-side spelling.
        ty: DirectValueType,
        /// Rust-side receive mode.
        receive: D::Receive,
    },
    /// Value crosses as encoded bytes.
    Encoded {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
        /// Rust-side receive mode.
        receive: D::Receive,
    },
    /// Value crosses as an opaque handle to a class or callback.
    Handle {
        /// Declaration the handle points to.
        target: HandleTarget,
        /// Wire carrier for the handle.
        carrier: S::HandleCarrier,
        /// Whether the slot may be null.
        presence: HandlePresence,
        /// Rust-side receive mode.
        receive: D::Receive,
    },
    /// `Option<P>` for primitive `P` through the surface's scalar-option
    /// path.
    ///
    /// Native packs through a wire buffer. Wasm32 uses one `f64` slot
    /// with `f64::NAN` as the `None` sentinel.
    ScalarOption {
        /// Inner primitive.
        primitive: Primitive,
    },
    /// `Vec<T>` for primitive or direct-record `T` through the
    /// surface's direct-vector path.
    ///
    /// Native uses `VecTransport::pack_vec` / `unpack_vec`. Wasm32 uses
    /// a `(ptr, len, cap, align)` quadruple.
    DirectVec {
        /// Element type.
        element: DirectVectorElementType,
    },
}

impl<S: Surface, D: Direction> ParamPlan<S, D> {
    /// Renders this parameter plan through the shared parameter-plan walker.
    pub fn render_with<'plan, R>(&'plan self, renderer: &mut R) -> R::Output
    where
        R: ParamPlanRender<'plan, S, D>,
    {
        match self {
            Self::Direct { ty, receive } => renderer.direct(ty, *receive),
            Self::Encoded {
                ty,
                codec,
                shape,
                receive,
            } => renderer.encoded(ty, codec, *shape, *receive),
            Self::Handle {
                target,
                carrier,
                presence,
                receive,
            } => renderer.handle(target, *carrier, *presence, *receive),
            Self::ScalarOption { primitive } => renderer.scalar_option(*primitive),
            Self::DirectVec { element } => renderer.direct_vector(element),
        }
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::Encoded { shape, .. } => Some(*shape),
            _ => None,
        }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::Encoded { codec, .. } => D::codec_uses_result(codec),
            _ => false,
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Encoded { codec, .. } => D::codec_uses_builtin(codec, kind),
            _ => false,
        }
    }

    fn uses_direct_record_vector(&self) -> bool {
        matches!(
            self,
            Self::DirectVec {
                element: DirectVectorElementType::Record(_)
            }
        )
    }
}

/// Target-language rendering for parameter plans.
///
/// The shared walker owns the `ParamPlan` variant traversal. Backends
/// implement the rendering leaves, so direct, encoded, handle, scalar
/// option, and direct-vector cases do not drift into separate local
/// enum walks.
pub trait ParamPlanRender<'plan, S: Surface, D: Direction> {
    /// Value produced by the renderer.
    type Output;

    /// Renders a directly-carried parameter.
    fn direct(&mut self, ty: &'plan DirectValueType, receive: D::Receive) -> Self::Output;

    /// Renders an encoded parameter.
    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan D::Codec,
        shape: S::BufferShape,
        receive: D::Receive,
    ) -> Self::Output;

    /// Renders a handle parameter.
    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: S::HandleCarrier,
        presence: HandlePresence,
        receive: D::Receive,
    ) -> Self::Output;

    /// Renders a scalar-option parameter.
    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output;

    /// Renders a direct-vector parameter.
    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output;
}

/// A callable's return slot.
///
/// `meta` carries doc and default metadata that the source method
/// declared. `plan` is the wire shape of the value. A callable that
/// returns nothing carries `ReturnPlan::Void`; there is no separate
/// absence-of-return state.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, D: Direction, D::Codec: Serialize, S::IncomingClosureRegistration: Serialize, S::OutgoingClosureRegistration: Serialize, S::AsyncProtocol: Serialize, D::Receive: Serialize, <D::Opposite as Direction>::Codec: Serialize, <D::Opposite as Direction>::Receive: Serialize, <D::Opposite as ParamDirection<S>>::Payload: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned, S::IncomingClosureRegistration: serde::de::DeserializeOwned, S::OutgoingClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned, <D::Opposite as Direction>::Codec: serde::de::DeserializeOwned, <D::Opposite as Direction>::Receive: serde::de::DeserializeOwned, <D::Opposite as ParamDirection<S>>::Payload: serde::de::DeserializeOwned"
))]
pub struct ReturnDecl<S: Surface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    meta: ElementMeta,
    plan: ReturnPlan<S, D>,
}

impl<S: Surface, D: Direction> ReturnDecl<S, D>
where
    D::Opposite: ParamDirection<S>,
{
    pub(crate) fn new(meta: ElementMeta, plan: ReturnPlan<S, D>) -> Self {
        Self { meta, plan }
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    /// Returns the return plan.
    pub fn plan(&self) -> &ReturnPlan<S, D> {
        &self.plan
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        self.plan.native_symbols()
    }

    fn uses_result_codec(&self) -> bool {
        self.plan.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.plan.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.plan.uses_direct_record_vector()
    }
}

/// How a return value crosses the boundary in direction `D`.
///
/// The `*ViaReturnSlot` variants occupy the native return slot. The
/// `*ViaOutPointer` variants write the value through a caller-supplied
/// out-pointer parameter while the return slot carries an error status
/// instead. `Void` names the no-value case explicitly.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, D: Direction, D::Codec: Serialize, S::IncomingClosureRegistration: Serialize, S::OutgoingClosureRegistration: Serialize, S::AsyncProtocol: Serialize, D::Receive: Serialize, <D::Opposite as Direction>::Codec: Serialize, <D::Opposite as Direction>::Receive: Serialize, <D::Opposite as ParamDirection<S>>::Payload: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned, S::IncomingClosureRegistration: serde::de::DeserializeOwned, S::OutgoingClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned, <D::Opposite as Direction>::Codec: serde::de::DeserializeOwned, <D::Opposite as Direction>::Receive: serde::de::DeserializeOwned, <D::Opposite as ParamDirection<S>>::Payload: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ReturnPlan<S: Surface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    /// No return value.
    Void,
    /// Direct value in the return slot.
    DirectViaReturnSlot {
        /// Foreign-side spelling.
        ty: DirectValueType,
    },
    /// Encoded value in the return slot.
    EncodedViaReturnSlot {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
    /// Handle in the return slot.
    HandleViaReturnSlot {
        /// Declaration the handle points to.
        target: HandleTarget,
        /// Wire carrier for the handle.
        carrier: S::HandleCarrier,
        /// Whether the slot may be null.
        presence: HandlePresence,
    },
    /// Scalar-option primitive in the return slot.
    ScalarOptionViaReturnSlot {
        /// Inner primitive.
        primitive: Primitive,
    },
    /// Direct-vector in the return slot.
    DirectVecViaReturnSlot {
        /// Element type.
        element: DirectVectorElementType,
    },
    /// Direct value through an out-pointer (return slot carries the
    /// error status).
    DirectViaOutPointer {
        /// Foreign-side spelling.
        ty: DirectValueType,
    },
    /// Encoded value through an out-pointer.
    EncodedViaOutPointer {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
    /// Handle through an out-pointer.
    HandleViaOutPointer {
        /// Declaration the handle points to.
        target: HandleTarget,
        /// Wire carrier for the handle.
        carrier: S::HandleCarrier,
        /// Whether the slot may be null.
        presence: HandlePresence,
    },
    /// Closure handle written through a caller-supplied out-pointer.
    ///
    /// Closure returns always cross via out-pointer on every surface.
    /// The rendered ABI is uniform:
    /// `extern "C" fn(..., out: *mut ClosureReturnStorage)` — the
    /// caller allocates space for the closure registration value and
    /// the callee writes through the pointer. The return slot is
    /// free for an error status, so `Result<closure, E>` lowers
    /// naturally with the closure success going through the
    /// out-pointer and the error status taking the return slot.
    ///
    /// One ABI for every surface, including platforms whose C ABI
    /// (Win64) would otherwise force a hidden sret pointer on wide
    /// struct returns. No backend guessing, no platform split.
    ClosureViaOutPointer(ClosureReturn<S, D>),
}

/// Where a returned value is delivered in the native ABI.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ReturnValueSlot {
    /// The value is the native function's return value.
    ReturnSlot,
    /// The value is written through a caller-provided out pointer.
    OutPointer,
}

/// Target-language rendering for return plans.
///
/// The shared walker owns the `ReturnPlan` variant traversal. Backends
/// implement the rendering leaves, so return-slot and out-pointer cases
/// cannot drift into parallel local enum walks.
pub trait ReturnPlanRender<'plan, S: Surface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    /// Value produced by the renderer.
    type Output;

    /// Renders a void return.
    fn void(&mut self) -> Self::Output;

    /// Renders a directly-carried value.
    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output;

    /// Renders an encoded value.
    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan D::Codec,
        shape: S::BufferShape,
    ) -> Self::Output;

    /// Renders a handle value.
    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: S::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output;

    /// Renders a scalar-option return.
    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output;

    /// Renders a direct-vector return.
    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output;

    /// Renders a closure return.
    fn closure(&mut self, closure: &'plan ClosureReturn<S, D>) -> Self::Output;
}

impl<S: Surface, D: Direction> ReturnPlan<S, D>
where
    D::Opposite: ParamDirection<S>,
{
    /// Renders this return plan through the shared return-plan walker.
    pub fn render_with<'plan, R>(&'plan self, renderer: &mut R) -> R::Output
    where
        R: ReturnPlanRender<'plan, S, D>,
    {
        match self {
            Self::Void => renderer.void(),
            Self::DirectViaReturnSlot { ty } => renderer.direct(ReturnValueSlot::ReturnSlot, ty),
            Self::EncodedViaReturnSlot { ty, codec, shape } => {
                renderer.encoded(ReturnValueSlot::ReturnSlot, ty, codec, *shape)
            }
            Self::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => renderer.handle(ReturnValueSlot::ReturnSlot, target, *carrier, *presence),
            Self::ScalarOptionViaReturnSlot { primitive } => renderer.scalar_option(*primitive),
            Self::DirectVecViaReturnSlot { element } => renderer.direct_vector(element),
            Self::DirectViaOutPointer { ty } => renderer.direct(ReturnValueSlot::OutPointer, ty),
            Self::EncodedViaOutPointer { ty, codec, shape } => {
                renderer.encoded(ReturnValueSlot::OutPointer, ty, codec, *shape)
            }
            Self::HandleViaOutPointer {
                target,
                carrier,
                presence,
            } => renderer.handle(ReturnValueSlot::OutPointer, target, *carrier, *presence),
            Self::ClosureViaOutPointer(closure) => renderer.closure(closure),
        }
    }

    pub(crate) fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        match self {
            Self::ClosureViaOutPointer(closure) => closure.native_symbols(),
            Self::Void
            | Self::DirectViaReturnSlot { .. }
            | Self::EncodedViaReturnSlot { .. }
            | Self::HandleViaReturnSlot { .. }
            | Self::ScalarOptionViaReturnSlot { .. }
            | Self::DirectVecViaReturnSlot { .. }
            | Self::DirectViaOutPointer { .. }
            | Self::EncodedViaOutPointer { .. }
            | Self::HandleViaOutPointer { .. } => Box::new(std::iter::empty()),
        }
    }

    pub(crate) const fn uses_return_slot(&self) -> bool {
        matches!(
            self,
            Self::DirectViaReturnSlot { .. }
                | Self::EncodedViaReturnSlot { .. }
                | Self::HandleViaReturnSlot { .. }
                | Self::ScalarOptionViaReturnSlot { .. }
                | Self::DirectVecViaReturnSlot { .. }
        )
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::EncodedViaReturnSlot { shape, .. } | Self::EncodedViaOutPointer { shape, .. } => {
                Some(*shape)
            }
            _ => None,
        }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::EncodedViaReturnSlot { codec, .. } | Self::EncodedViaOutPointer { codec, .. } => {
                D::codec_uses_result(codec)
            }
            Self::ClosureViaOutPointer(closure) => closure.uses_result_codec(),
            _ => false,
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::EncodedViaReturnSlot { codec, .. } | Self::EncodedViaOutPointer { codec, .. } => {
                D::codec_uses_builtin(codec, kind)
            }
            Self::ClosureViaOutPointer(closure) => closure.uses_builtin_codec(kind),
            _ => false,
        }
    }

    fn uses_direct_record_vector(&self) -> bool {
        match self {
            Self::DirectVecViaReturnSlot {
                element: DirectVectorElementType::Record(_),
            } => true,
            Self::ClosureViaOutPointer(closure) => closure.uses_direct_record_vector(),
            _ => false,
        }
    }

    /// Switches a `*ViaReturnSlot` variant to its `*ViaOutPointer`
    /// counterpart. Called when the matching error channel takes the
    /// return slot.
    ///
    /// A closure return has no out-pointer counterpart (the wire shape
    /// is always a handle in the return slot), so a closure-bearing
    /// return paired with a fallible error channel is rejected at the
    /// lowering step before reaching here.
    pub(crate) fn into_out(self) -> Self {
        match self {
            Self::DirectViaReturnSlot { ty } => Self::DirectViaOutPointer { ty },
            Self::EncodedViaReturnSlot { ty, codec, shape } => {
                Self::EncodedViaOutPointer { ty, codec, shape }
            }
            Self::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => Self::HandleViaOutPointer {
                target,
                carrier,
                presence,
            },
            other => other,
        }
    }
}

/// How a fallible callable reports its error in direction `D`.
///
/// `None` means the callable cannot fail across the boundary.
/// `Status*` carries an integer where one value is success and the
/// others map to specific failures. `Encoded*` carries the failure as
/// a typed payload. The variant suffix names the delivery slot:
/// `ViaReturnSlot` claims the native return slot, `ViaOutPointer`
/// writes through a trailing out-pointer parameter.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, D: Direction, D::Codec: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ErrorDecl<S: Surface, D: Direction> {
    /// No error channel.
    None(#[serde(skip)] PhantomData<(S, D)>),
    /// Status integer in the return slot.
    StatusViaReturnSlot {
        /// Status integer representation.
        repr: IntegerRepr,
    },
    /// Status integer in an out-pointer.
    StatusViaOutPointer {
        /// Status integer representation.
        repr: IntegerRepr,
    },
    /// Encoded error in the return slot.
    EncodedViaReturnSlot {
        /// Error type.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
    /// Encoded error in an out-pointer.
    EncodedViaOutPointer {
        /// Error type.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
}

/// The error transport selected for one callable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorChannel<'error, S: Surface, D: Direction> {
    /// The callable has no error transport.
    None,
    /// The callable reports errors through a status value.
    Status,
    /// The callable reports errors through an encoded payload.
    Encoded {
        /// The slot used by the encoded payload.
        placement: ErrorPlacement,
        /// The source error type.
        ty: &'error TypeRef,
        /// The codec selected for the encoded payload.
        codec: &'error D::Codec,
        /// The buffer shape used by the payload.
        shape: S::BufferShape,
    },
}

/// Where an error value is transported in the C-facing call shape.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorPlacement {
    /// The error uses the return slot.
    ReturnSlot,
    /// The error uses an out-pointer parameter.
    OutPointer,
}

impl<S: Surface, D: Direction> ErrorDecl<S, D> {
    /// Builds the `None` variant.
    pub fn none() -> Self {
        Self::None(PhantomData)
    }

    /// Returns the selected error transport.
    pub fn channel<'error>(&'error self) -> ErrorChannel<'error, S, D> {
        match self {
            Self::None(_) => ErrorChannel::None,
            Self::StatusViaReturnSlot { .. } | Self::StatusViaOutPointer { .. } => {
                ErrorChannel::Status
            }
            Self::EncodedViaReturnSlot { ty, codec, shape } => ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ty,
                codec,
                shape: *shape,
            },
            Self::EncodedViaOutPointer { ty, codec, shape } => ErrorChannel::Encoded {
                placement: ErrorPlacement::OutPointer,
                ty,
                codec,
                shape: *shape,
            },
        }
    }

    pub(crate) const fn uses_return_slot(&self) -> bool {
        matches!(
            self,
            Self::StatusViaReturnSlot { .. } | Self::EncodedViaReturnSlot { .. }
        )
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::EncodedViaReturnSlot { shape, .. } | Self::EncodedViaOutPointer { shape, .. } => {
                Some(*shape)
            }
            _ => None,
        }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::EncodedViaReturnSlot { codec, .. } | Self::EncodedViaOutPointer { codec, .. } => {
                D::codec_uses_result(codec)
            }
            _ => false,
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::EncodedViaReturnSlot { codec, .. } | Self::EncodedViaOutPointer { codec, .. } => {
                D::codec_uses_builtin(codec, kind)
            }
            _ => false,
        }
    }
}

/// Whether a callable returns immediately or through an async protocol.
///
/// `Synchronous` means control returns when the call returns.
/// `Asynchronous` carries the surface's chosen async protocol value
/// (poll handle on native, synchronous-poll on wasm, and so on).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::AsyncProtocol: Serialize",
    deserialize = "S::AsyncProtocol: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ExecutionDecl<S: Surface> {
    /// Control returns when the call returns.
    Synchronous(#[serde(skip)] PhantomData<S>),
    /// Control returns through an async protocol.
    Asynchronous(S::AsyncProtocol),
}

impl<S: Surface> ExecutionDecl<S> {
    /// Returns the synchronous variant.
    pub fn synchronous() -> Self {
        Self::Synchronous(PhantomData)
    }

    /// Returns the asynchronous variant carrying the surface's async
    /// protocol value.
    pub fn asynchronous(protocol: S::AsyncProtocol) -> Self {
        Self::Asynchronous(protocol)
    }

    /// Returns whether control returns through an asynchronous protocol.
    pub fn uses_async_execution(&self) -> bool {
        matches!(self, Self::Asynchronous(_))
    }
}

/// How the inner Rust function receives a parameter or receiver.
///
/// Names what the source wrote: `ByValue` for `T`, `ByRef` for `&T`,
/// `ByMutRef` for `&mut T`. The native call slot does not change shape
/// based on this value; the extern wrapper reconciles ownership when
/// invoking the inner Rust function. Generated host APIs may still
/// surface the distinction in the rendered language (Swift `inout`,
/// Kotlin receiver semantics for handles, and so on), so renderers are
/// free to consult it.
///
/// # Example
///
/// `fn area(rect: &Rectangle)` records its parameter as
/// `Receive::ByRef`. `fn finalize(self)` records its receiver as
/// `Receive::ByValue`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Receive {
    /// `self` or by-value parameter. Rust takes ownership.
    ByValue,
    /// `&self` or `&T`.
    ByRef,
    /// `&mut self` or `&mut T`.
    ByMutRef,
}
