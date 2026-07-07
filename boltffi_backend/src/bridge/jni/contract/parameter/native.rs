//! Parameters accepted by generated JNI native methods.
//!
//! A Java call enters the bridge through a `Java_*` function, but Rust is still
//! reached through the lower C bridge. One Java parameter can expand to one C
//! argument, several C arguments, or a grouped protocol such as bytes, direct
//! records, callback handles, closures, continuations, or direct vectors.
//!
//! This module owns that method-parameter contract. It keeps the Java-facing
//! parameter beside the exact C expressions passed to the lower bridge, so
//! method rendering does not need to inspect C parameter groups again.

use crate::{
    bridge::{
        c::{Expression, Identifier, TypeFragment},
        jni::{
            BytesParameter, CallbackParameter, ClosureParameter, ContinuationParameter,
            DirectVectorParameter, RecordParameter, ScalarParameter,
        },
    },
    core::Result,
};

/// JNI parameter accepted by one native method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeParameter {
    kind: NativeParameterKind,
}

impl NativeParameter {
    pub(in crate::bridge::jni::contract::parameter) fn new(kind: NativeParameterKind) -> Self {
        Self { kind }
    }

    /// Returns the generated C parameter name.
    pub fn name(&self) -> &Identifier {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.name(),
            NativeParameterKind::Bytes(parameter) => parameter.name(),
            NativeParameterKind::DirectVector(parameter) => parameter.name(),
            NativeParameterKind::Record(parameter) => parameter.name(),
            NativeParameterKind::Callback(parameter) => parameter.name(),
            NativeParameterKind::Closure(parameter) => parameter.name(),
            NativeParameterKind::Continuation(parameter) => parameter.name(),
        }
    }

    /// Returns the parameter shape selected by the JNI bridge.
    pub fn kind(&self) -> &NativeParameterKind {
        &self.kind
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.ty().as_type_fragment(),
            NativeParameterKind::Bytes(_) => TypeFragment::new("jbyteArray"),
            NativeParameterKind::DirectVector(parameter) => parameter.array_type(),
            NativeParameterKind::Record(_) => TypeFragment::new("jbyteArray"),
            NativeParameterKind::Callback(parameter) => parameter.ty(),
            NativeParameterKind::Closure(parameter) => parameter.ty(),
            NativeParameterKind::Continuation(parameter) => parameter.ty(),
        }
    }

    /// Returns C bridge call arguments produced from this JNI parameter.
    pub fn c_arguments(&self) -> Result<Vec<Expression>> {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => {
                parameter.c_argument().map(|value| vec![value])
            }
            NativeParameterKind::Bytes(parameter) => Ok(vec![
                Expression::cast(
                    TypeFragment::new("const uint8_t *"),
                    Expression::identifier(parameter.pointer().clone()),
                ),
                Expression::cast(
                    TypeFragment::new("uintptr_t"),
                    Expression::identifier(parameter.length().clone()),
                ),
            ]
            .into_iter()
            .chain(parameter.writeback().map(|writeback| {
                Expression::address_of(Expression::identifier(writeback.local().clone()))
            }))
            .collect()),
            NativeParameterKind::DirectVector(parameter) => Ok(parameter.c_arguments()),
            NativeParameterKind::Record(parameter) => Ok(std::iter::once(parameter.c_argument())
                .chain(parameter.writeback().map(|writeback| {
                    Expression::address_of(Expression::identifier(writeback.local().clone()))
                }))
                .collect()),
            NativeParameterKind::Callback(parameter) => Ok(vec![parameter.c_argument()]),
            NativeParameterKind::Closure(parameter) => Ok(parameter.c_arguments()),
            NativeParameterKind::Continuation(parameter) => parameter.c_arguments(),
        }
    }

    /// Returns byte-array parameter details when this parameter carries bytes.
    pub fn bytes(&self) -> Option<&BytesParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_)
            | NativeParameterKind::DirectVector(_)
            | NativeParameterKind::Record(_)
            | NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => None,
            NativeParameterKind::Bytes(parameter) => Some(parameter),
        }
    }

    /// Returns direct-record parameter details when this parameter carries a record.
    pub fn record(&self) -> Option<&RecordParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_)
            | NativeParameterKind::Bytes(_)
            | NativeParameterKind::DirectVector(_)
            | NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => None,
            NativeParameterKind::Record(parameter) => Some(parameter),
        }
    }

    /// Returns direct-vector parameter details when this parameter carries a Java array.
    pub fn direct_vector(&self) -> Option<&DirectVectorParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_)
            | NativeParameterKind::Bytes(_)
            | NativeParameterKind::Record(_)
            | NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => None,
            NativeParameterKind::DirectVector(parameter) => Some(parameter),
        }
    }

    /// Returns whether this parameter supplies a C poll continuation.
    pub fn is_continuation(&self) -> bool {
        matches!(self.kind, NativeParameterKind::Continuation(_))
    }
}

/// JNI parameter shape selected from one or more C ABI parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NativeParameterKind {
    /// A scalar JNI parameter passed directly to the C bridge.
    Scalar(ScalarParameter),
    /// A `jbyteArray` expanded to pointer and length C bridge arguments.
    Bytes(BytesParameter),
    /// A Java primitive array expanded to pointer and length C bridge arguments.
    DirectVector(DirectVectorParameter),
    /// A `jbyteArray` copied into one direct C record value.
    Record(RecordParameter),
    /// A `jlong` Java callback handle converted through a C callback constructor.
    Callback(CallbackParameter),
    /// A `jlong` Java closure handle expanded to call, context, and release C ABI parameters.
    Closure(ClosureParameter),
    /// A `jlong` callback data value paired with the generated JNI continuation callback.
    Continuation(ContinuationParameter),
}
